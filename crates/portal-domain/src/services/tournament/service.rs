//! Tournament service for managing tournament lifecycle.

use std::sync::Arc;

use portal_core::types::{
    AdvancementRule, BracketType, MatchFormatPlan, StageFormat, StageStatus, TournamentFormat,
    TournamentMatchStatus, TournamentRegistrationStatus, TournamentStatus,
};
use portal_core::{
    DomainError, FieldError, LeagueId, LeagueSeasonId, LeagueTeamSeasonId, PlayerId,
    TournamentBracketId, TournamentId, TournamentInvitationId, TournamentMatchId, UserId,
    ValidationError,
};

use crate::entities::tournament::{
    CreateTournamentCommand, Tournament, TournamentBracket, TournamentInvitation, TournamentMatch,
    TournamentRegistration, TournamentStage, UpdateTournamentCommand,
};
use crate::repositories::tournament::{
    CreateTournament, CreateTournamentBracket, CreateTournamentInvitation,
    CreateTournamentRegistration, CreateTournamentStage, CreateTournamentStanding,
    TournamentBracketRepository, TournamentFilters, TournamentInvitationRepository,
    TournamentMapPoolRepository, TournamentMatchRepository, TournamentRegistrationRepository,
    TournamentRepository, TournamentStageRepository, TournamentStandingsRepository,
    UpdateTournament, UpsertTournamentMapPool,
};

use super::bracket_generator::{BracketGenerator, CrossLinkType};
use super::registration::initial_registration_status;

/// Service for tournament management.
pub struct TournamentService<TR, TSR, TBR, TRR, TMR, TSTR, TMPR, TIR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TMR: TournamentMatchRepository,
    TSTR: TournamentStandingsRepository,
    TMPR: TournamentMapPoolRepository,
    TIR: TournamentInvitationRepository,
{
    tournament_repo: Arc<TR>,
    stage_repo: Arc<TSR>,
    bracket_repo: Arc<TBR>,
    registration_repo: Arc<TRR>,
    match_repo: Arc<TMR>,
    standings_repo: Arc<TSTR>,
    map_pool_repo: Arc<TMPR>,
    invitation_repo: Arc<TIR>,
}

impl<TR, TSR, TBR, TRR, TMR, TSTR, TMPR, TIR>
    TournamentService<TR, TSR, TBR, TRR, TMR, TSTR, TMPR, TIR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TMR: TournamentMatchRepository,
    TSTR: TournamentStandingsRepository,
    TMPR: TournamentMapPoolRepository,
    TIR: TournamentInvitationRepository,
{
    /// Create a new tournament service.
    pub const fn new(
        tournament_repo: Arc<TR>,
        stage_repo: Arc<TSR>,
        bracket_repo: Arc<TBR>,
        registration_repo: Arc<TRR>,
        match_repo: Arc<TMR>,
        standings_repo: Arc<TSTR>,
        map_pool_repo: Arc<TMPR>,
        invitation_repo: Arc<TIR>,
    ) -> Self {
        Self {
            tournament_repo,
            stage_repo,
            bracket_repo,
            registration_repo,
            match_repo,
            standings_repo,
            map_pool_repo,
            invitation_repo,
        }
    }

    /// Create a new tournament.
    pub async fn create_tournament(
        &self,
        cmd: CreateTournamentCommand,
        created_by: UserId,
    ) -> Result<Tournament, DomainError> {
        // Cross-field participant-range check. Previously this was left to
        // the `tournaments_participants_check` DB constraint, which surfaced
        // as a 500 (a constraint violation is an Internal error by the time
        // it reaches the API boundary). Validate up front so callers get a
        // 400 with a field-scoped message.
        if cmd.min_participants > cmd.max_participants {
            return Err(DomainError::Validation(ValidationError::field(
                FieldError::new(
                    "min_participants",
                    "min_participants cannot be greater than max_participants",
                    "invalid_range",
                ),
            )));
        }

        // Check slug uniqueness
        if self.tournament_repo.slug_exists(&cmd.slug).await? {
            return Err(DomainError::Conflict(format!(
                "Tournament with slug '{}' already exists",
                cmd.slug
            )));
        }

        // Captured before `cmd` is consumed by the insert below.
        let map_pool = cmd.map_pool.clone();
        let veto_format_id = cmd.default_map_veto_format.clone();

        // Create the tournament
        let tournament = self
            .tournament_repo
            .create(CreateTournament {
                game_id: cmd.game_id,
                kind: portal_core::types::TournamentKind::Standard,
                league_id: cmd.league_id,
                season_id: cmd.season_id,
                name: cmd.name,
                slug: cmd.slug,
                description: cmd.description,
                format: cmd.format,
                format_settings: cmd.format_settings.unwrap_or_else(|| serde_json::json!({})),
                participant_type: cmd.participant_type,
                team_size: cmd.team_size,
                min_participants: cmd.min_participants,
                max_participants: cmd.max_participants,
                registration_type: cmd.registration_type,
                registration_start: cmd.registration_start,
                registration_end: cmd.registration_end,
                check_in_required: cmd.check_in_required,
                check_in_start: cmd.check_in_start,
                check_in_end: cmd.check_in_end,
                scheduling_mode: cmd.scheduling_mode,
                starts_at: cmd.starts_at,
                default_match_format: cmd.default_match_format,
                default_map_veto_format: cmd.default_map_veto_format,
                withdrawal_policy: cmd.withdrawal_policy,
                rules_url: cmd.rules_url,
                settings: cmd.settings.unwrap_or_else(|| serde_json::json!({})),
                created_by,
            })
            .await?;

        // Persist the tournament's map pool. The repositories don't share a
        // transaction, so on failure we compensate by deleting the tournament
        // we just wrote — a tournament without a pool would break the
        // fail-closed map validation on result submission.
        if let Err(e) = self
            .map_pool_repo
            .upsert(UpsertTournamentMapPool {
                tournament_id: tournament.id,
                stage_id: None,
                maps: map_pool,
                veto_format_id,
            })
            .await
        {
            if let Err(cleanup_err) = self.tournament_repo.delete(tournament.id).await {
                tracing::error!(
                    error = %cleanup_err,
                    tournament_id = %tournament.id,
                    "failed to roll back tournament after map pool write failed"
                );
            }
            return Err(e);
        }

        Ok(tournament)
    }

    /// Get a tournament by ID.
    pub async fn get_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        self.tournament_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::TournamentNotFound(id))
    }

    /// Get a tournament by slug.
    pub async fn get_tournament_by_slug(&self, slug: &str) -> Result<Tournament, DomainError> {
        self.tournament_repo
            .find_by_slug(slug)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "tournament",
                query: slug.to_string(),
            })
    }

    /// Update a tournament.
    pub async fn update_tournament(
        &self,
        id: TournamentId,
        cmd: UpdateTournamentCommand,
    ) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        // Check if tournament can be updated
        if tournament.has_started() {
            return Err(DomainError::TournamentAlreadyStarted);
        }

        // Same cross-field participant-range guard as `create_tournament`,
        // resolved against the stored values for whichever bound is absent.
        let new_min = cmd.min_participants.unwrap_or(tournament.min_participants);
        let new_max = cmd.max_participants.unwrap_or(tournament.max_participants);
        if new_min > new_max {
            return Err(DomainError::Validation(ValidationError::field(
                FieldError::new(
                    "min_participants",
                    "min_participants cannot be greater than max_participants",
                    "invalid_range",
                ),
            )));
        }

        // Check slug uniqueness if changing
        if let Some(ref new_slug) = cmd.slug
            && new_slug != &tournament.slug
            && self.tournament_repo.slug_exists(new_slug).await?
        {
            return Err(DomainError::Conflict(format!(
                "Tournament with slug '{new_slug}' already exists"
            )));
        }

        self.tournament_repo
            .update(
                id,
                UpdateTournament {
                    name: cmd.name,
                    slug: cmd.slug,
                    description: cmd.description,
                    format_settings: cmd.format_settings,
                    min_participants: cmd.min_participants,
                    max_participants: cmd.max_participants,
                    registration_start: cmd.registration_start,
                    registration_end: cmd.registration_end,
                    check_in_required: cmd.check_in_required,
                    check_in_start: cmd.check_in_start,
                    check_in_end: cmd.check_in_end,
                    starts_at: cmd.starts_at,
                    ends_at: cmd.ends_at,
                    timezone_hint: cmd.timezone_hint,
                    default_match_format: cmd.default_match_format,
                    default_map_veto_format: cmd.default_map_veto_format,
                    prize_pool: cmd.prize_pool,
                    rules_url: cmd.rules_url,
                    // Merge-not-replace: a PATCH sending only one settings key
                    // (e.g. eligibility) must not erase the others.
                    settings: cmd.settings.map(|incoming| {
                        crate::services::settings_merge::shallow_merge(
                            &tournament.settings,
                            incoming,
                        )
                    }),
                    withdrawal_policy: cmd.withdrawal_policy,
                },
            )
            .await
    }

    /// List tournaments with filters.
    pub async fn list_tournaments(
        &self,
        filters: TournamentFilters,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError> {
        self.tournament_repo.list(filters, limit, offset).await
    }

    /// Publish a tournament (make it visible for registration).
    pub async fn publish_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if tournament.status != TournamentStatus::Draft {
            return Err(DomainError::InvalidState(format!(
                "Cannot publish tournament in {} status",
                tournament.status
            )));
        }

        self.tournament_repo.mark_published(id).await
    }

    /// Open registration for a tournament.
    pub async fn open_registration(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if !tournament
            .status
            .can_transition_to(TournamentStatus::Registration)
        {
            return Err(DomainError::InvalidTournamentTransition {
                from: tournament.status.to_string(),
                to: TournamentStatus::Registration.to_string(),
            });
        }

        self.tournament_repo
            .update_status(id, TournamentStatus::Registration)
            .await
    }

    /// Close registration for a tournament (transition to Scheduled).
    pub async fn close_registration(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if !tournament
            .status
            .can_transition_to(TournamentStatus::Scheduled)
        {
            return Err(DomainError::InvalidTournamentTransition {
                from: tournament.status.to_string(),
                to: TournamentStatus::Scheduled.to_string(),
            });
        }

        self.tournament_repo
            .update_status(id, TournamentStatus::Scheduled)
            .await
    }

    /// Reopen registration for a tournament (transition from Scheduled back to Registration).
    pub async fn reopen_registration(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if !tournament
            .status
            .can_transition_to(TournamentStatus::Registration)
        {
            return Err(DomainError::InvalidTournamentTransition {
                from: tournament.status.to_string(),
                to: TournamentStatus::Registration.to_string(),
            });
        }

        self.tournament_repo
            .update_status(id, TournamentStatus::Registration)
            .await
    }

    /// Cancel a tournament.
    pub async fn cancel_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if !tournament
            .status
            .can_transition_to(TournamentStatus::Cancelled)
        {
            return Err(DomainError::InvalidTournamentTransition {
                from: tournament.status.to_string(),
                to: TournamentStatus::Cancelled.to_string(),
            });
        }

        self.tournament_repo
            .update_status(id, TournamentStatus::Cancelled)
            .await
    }

    /// Move a tournament to another league and/or season.
    ///
    /// The repair for a tournament filed under the wrong league — which is
    /// only safe while nobody has entered it. A registration points at a
    /// *team season*, i.e. a team as it exists in one league's season, so
    /// moving a tournament that has registrations would leave it full of
    /// entrants belonging to a competition it is no longer part of. That is
    /// refused rather than quietly carried across.
    ///
    /// `None` for both detaches the tournament from any league (standalone).
    pub async fn move_tournament(
        &self,
        id: TournamentId,
        league_id: Option<LeagueId>,
        season_id: Option<LeagueSeasonId>,
    ) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if season_id.is_some() && league_id.is_none() {
            return Err(DomainError::InvalidState(
                "a season cannot be set without its league".to_string(),
            ));
        }

        // That the season actually belongs to the league is checked by the
        // repository, alongside the write — this service holds no season
        // repository, and a check made here could be raced by the season
        // moving between the look and the update.

        if tournament.league_id == league_id && tournament.season_id == season_id {
            return Err(DomainError::InvalidState(
                "tournament is already there".to_string(),
            ));
        }

        let registrations = self.tournament_repo.count_registrations(id).await?;
        if registrations > 0 {
            return Err(DomainError::InvalidState(format!(
                "tournament has {registrations} registration(s) tied to its current league; \
                 move refused rather than stranding them"
            )));
        }

        let moved = self
            .tournament_repo
            .set_league_and_season(id, league_id, season_id)
            .await?;

        tracing::info!(
            tournament_id = %id,
            from_league = ?tournament.league_id,
            to_league = ?league_id,
            to_season = ?season_id,
            "Tournament moved"
        );

        Ok(moved)
    }

    /// Archive a tournament: it stops appearing in player-facing listings.
    ///
    /// Orthogonal to status — a completed tournament that is put away is
    /// still completed when it comes back — and distinct from cancelling,
    /// which is a statement about the competition itself.
    pub async fn archive_tournament(
        &self,
        id: TournamentId,
        archived_by: UserId,
    ) -> Result<Tournament, DomainError> {
        self.tournament_repo
            .set_archived(id, Some(archived_by))
            .await
    }

    /// Restore an archived tournament.
    pub async fn restore_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        self.tournament_repo.set_archived(id, None).await
    }

    /// Complete a tournament.
    pub async fn complete_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if !tournament
            .status
            .can_transition_to(TournamentStatus::Completed)
        {
            return Err(DomainError::InvalidTournamentTransition {
                from: tournament.status.to_string(),
                to: TournamentStatus::Completed.to_string(),
            });
        }

        self.tournament_repo
            .update_status(id, TournamentStatus::Completed)
            .await
    }

    /// Finalize a tournament.
    pub async fn finalize_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        if !tournament
            .status
            .can_transition_to(TournamentStatus::Finalized)
        {
            return Err(DomainError::InvalidTournamentTransition {
                from: tournament.status.to_string(),
                to: TournamentStatus::Finalized.to_string(),
            });
        }

        self.tournament_repo
            .update_status(id, TournamentStatus::Finalized)
            .await
    }

    /// Create a stage for a tournament.
    pub async fn create_stage(
        &self,
        cmd: crate::entities::tournament::CreateTournamentStageCommand,
    ) -> Result<TournamentStage, DomainError> {
        let tournament = self.get_tournament(cmd.tournament_id).await?;

        // Only allow adding stages before tournament starts
        if tournament.has_started() {
            return Err(DomainError::TournamentAlreadyStarted);
        }

        // Reject malformed per-round format overrides at write time, not
        // when the bracket generates.
        MatchFormatPlan::from_settings(
            cmd.match_format.unwrap_or(tournament.default_match_format),
            cmd.format_settings.as_ref(),
        )
        .map_err(|e| DomainError::InvalidState(format!("invalid stage format settings: {e}")))?;

        self.stage_repo
            .create(CreateTournamentStage {
                tournament_id: cmd.tournament_id,
                name: cmd.name,
                stage_order: cmd.stage_order,
                format: cmd.format,
                format_settings: cmd.format_settings.unwrap_or_else(|| serde_json::json!({})),
                advancement_count: cmd.advancement_count,
                advancement_rule: cmd.advancement_rule,
                match_format: cmd.match_format,
                map_veto_format: cmd.map_veto_format,
                starts_at: cmd.starts_at,
                ends_at: cmd.ends_at,
            })
            .await
    }

    /// Update a pending stage's configuration.
    ///
    /// Allowed after the tournament starts — that is the point for
    /// groups+playoffs, where the pending playoff stage gets its best-of
    /// tuned while the groups run — but not once the stage itself has
    /// activated and generated matches.
    pub async fn update_stage(
        &self,
        tournament_id: TournamentId,
        stage_id: portal_core::TournamentStageId,
        update: crate::repositories::tournament::UpdateTournamentStage,
    ) -> Result<TournamentStage, DomainError> {
        let tournament = self.get_tournament(tournament_id).await?;
        let stage = self
            .stage_repo
            .find_by_id(stage_id)
            .await?
            .filter(|s| s.tournament_id == tournament_id)
            .ok_or(DomainError::TournamentStageNotFound(stage_id))?;

        if stage.status != StageStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "Cannot edit a stage in {} status — its matches already exist",
                stage.status
            )));
        }

        // Validate the configuration that would result from the merge
        // (COALESCE semantics: absent fields keep their current value).
        let effective_format = update
            .match_format
            .or(stage.match_format)
            .unwrap_or(tournament.default_match_format);
        let effective_settings = update
            .format_settings
            .clone()
            .unwrap_or_else(|| stage.format_settings.clone());
        MatchFormatPlan::from_settings(effective_format, Some(&effective_settings)).map_err(
            |e| DomainError::InvalidState(format!("invalid stage format settings: {e}")),
        )?;

        self.stage_repo.update(stage_id, update).await
    }

    /// Get stages for a tournament.
    pub async fn get_stages(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentStage>, DomainError> {
        self.stage_repo.list_by_tournament(tournament_id).await
    }

    /// Get a single stage by id.
    ///
    /// Point lookup for the per-match paths (veto bootstrap), which only need
    /// the match's own stage — listing every stage of the tournament once per
    /// match turns a bracket round's worth of check-ins into an N+1.
    pub async fn get_stage(
        &self,
        stage_id: portal_core::TournamentStageId,
    ) -> Result<Option<TournamentStage>, DomainError> {
        self.stage_repo.find_by_id(stage_id).await
    }

    /// Register a team for a tournament.
    pub async fn register_team(
        &self,
        tournament_id: TournamentId,
        team_season_id: portal_core::LeagueTeamSeasonId,
        participant_name: String,
        participant_logo_url: Option<String>,
        registered_by: UserId,
    ) -> Result<TournamentRegistration, DomainError> {
        let tournament = self.get_tournament(tournament_id).await?;

        // Check registration is open
        if !tournament.is_registration_open() {
            return Err(DomainError::TournamentNotOpen);
        }

        // Invite-only tournaments admit only teams on the invite list.
        // Until P-27 this check did not exist anywhere, so `invite_only`
        // was indistinguishable from `approval`.
        let invitation = self
            .require_team_invitation(&tournament, team_season_id)
            .await?;

        // Check not already registered (allow re-registration after withdrawal)
        let replace_terminal = match self
            .registration_repo
            .find_by_team_season(tournament_id, team_season_id)
            .await?
        {
            // Remove the old withdrawn/disqualified/etc. registration so a
            // fresh one can be created — done inside the capacity tx below.
            Some(existing) if existing.status.is_terminal() => Some(existing.id),
            Some(_) => return Err(DomainError::AlreadyRegisteredForTournament),
            None => None,
        };

        // Capacity check + insert happen in one transaction under a row
        // lock on the tournament (see `create_with_capacity_check`). The
        // previous count-then-create pair let concurrent registrations
        // overflow `max_participants`.
        let registration = self
            .registration_repo
            .create_with_capacity_check(
                CreateTournamentRegistration {
                    tournament_id,
                    team_season_id: Some(team_season_id),
                    player_id: None,
                    adhoc_team_id: None,
                    participant_name,
                    participant_logo_url,
                    registered_by,
                    seed_rating: None,
                    // `Open` tournaments auto-approve; the rest wait for an
                    // organiser. Previously the insert omitted `status`
                    // entirely and the `'pending'` column default always
                    // won, so `Open` never auto-approved (P-2).
                    status: initial_registration_status(tournament.registration_type),
                },
                replace_terminal,
            )
            .await?;

        self.consume_invitation(invitation).await;

        Ok(registration)
    }

    /// Register a player for an individual tournament.
    pub async fn register_player(
        &self,
        tournament_id: TournamentId,
        player_id: portal_core::PlayerId,
        participant_name: String,
        registered_by: UserId,
    ) -> Result<TournamentRegistration, DomainError> {
        let tournament = self.get_tournament(tournament_id).await?;

        // Check registration is open
        if !tournament.is_registration_open() {
            return Err(DomainError::TournamentNotOpen);
        }

        // Invite-only tournaments admit only invited users (see P-27).
        // The invite targets the *user* here, not the player, because the
        // organiser invites an account and the account registers its own
        // player (`register_player` takes `auth.player_id`).
        let invitation = self
            .require_user_invitation(&tournament, registered_by)
            .await?;

        // Check not already registered (allow re-registration after withdrawal)
        let replace_terminal = match self
            .registration_repo
            .find_by_player(tournament_id, player_id)
            .await?
        {
            Some(existing) if existing.status.is_terminal() => Some(existing.id),
            Some(_) => return Err(DomainError::AlreadyRegisteredForTournament),
            None => None,
        };

        // Capacity check + insert in one transaction — see `register_team`.
        let registration = self
            .registration_repo
            .create_with_capacity_check(
                CreateTournamentRegistration {
                    tournament_id,
                    team_season_id: None,
                    player_id: Some(player_id),
                    adhoc_team_id: None,
                    participant_name,
                    participant_logo_url: None,
                    registered_by,
                    seed_rating: None,
                    // See `register_team` — `Open` auto-approves (P-2).
                    status: initial_registration_status(tournament.registration_type),
                },
                replace_terminal,
            )
            .await?;

        self.consume_invitation(invitation).await;

        Ok(registration)
    }

    // =========================================================================
    // Invite list (registration_type = invite_only)
    // =========================================================================

    /// Resolve the invitation a team-season needs to enter `tournament`.
    ///
    /// Returns `Ok(None)` when the tournament does not gate on invitations,
    /// `Ok(Some(invitation))` when a live one exists (so the caller can
    /// consume it), and [`DomainError::TournamentInviteOnly`] otherwise.
    async fn require_team_invitation(
        &self,
        tournament: &Tournament,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Option<TournamentInvitation>, DomainError> {
        if !tournament.registration_type.requires_invitation() {
            return Ok(None);
        }

        let invitation = self
            .invitation_repo
            .find_for_team_season(tournament.id, team_season_id)
            .await?
            .filter(TournamentInvitation::permits_registration)
            .ok_or(DomainError::TournamentInviteOnly)?;

        Ok(Some(invitation))
    }

    /// Resolve the invitation a user needs to enter `tournament`.
    ///
    /// See [`Self::require_team_invitation`].
    async fn require_user_invitation(
        &self,
        tournament: &Tournament,
        user_id: UserId,
    ) -> Result<Option<TournamentInvitation>, DomainError> {
        if !tournament.registration_type.requires_invitation() {
            return Ok(None);
        }

        let invitation = self
            .invitation_repo
            .find_for_user(tournament.id, user_id)
            .await?
            .filter(TournamentInvitation::permits_registration)
            .ok_or(DomainError::TournamentInviteOnly)?;

        Ok(Some(invitation))
    }

    /// Mark an invitation as consumed once the registration exists.
    ///
    /// Best-effort on purpose: the registration is already committed, and
    /// an invitation stuck at `pending` still permits registration, so
    /// failing the caller here would report a failure for work that
    /// succeeded. Logged so the discrepancy is visible.
    async fn consume_invitation(&self, invitation: Option<TournamentInvitation>) {
        let Some(invitation) = invitation else {
            return;
        };

        if let Err(e) = self.invitation_repo.mark_accepted(invitation.id).await {
            tracing::warn!(
                invitation_id = %invitation.id,
                error = %e,
                "Failed to mark tournament invitation accepted after registration"
            );
        }
    }

    /// Invite a user or team-season to a tournament.
    ///
    /// Authorization (`tournament.participants.manage`) is enforced at the
    /// handler; this method validates the invite shape against the
    /// tournament itself.
    pub async fn invite_to_tournament(
        &self,
        tournament_id: TournamentId,
        user_id: Option<UserId>,
        team_season_id: Option<LeagueTeamSeasonId>,
        message: Option<String>,
        invited_by: UserId,
    ) -> Result<TournamentInvitation, DomainError> {
        let tournament = self.get_tournament(tournament_id).await?;

        // Exactly one target. The DB enforces this too; checking here turns
        // a constraint violation (500) into a validation error (400).
        match (user_id, team_season_id) {
            (Some(_), None) | (None, Some(_)) => {}
            _ => {
                return Err(DomainError::Validation(ValidationError::field(
                    FieldError::new(
                        "user_id",
                        "exactly one of user_id or team_season_id must be provided",
                        "invalid_target",
                    ),
                )));
            }
        }

        // A team invite is meaningless for an individual tournament and
        // vice versa — the corresponding register_* path would never
        // consult it.
        if tournament.participant_type.requires_team_size() && team_season_id.is_none() {
            return Err(DomainError::Validation(ValidationError::field(
                FieldError::new(
                    "team_season_id",
                    "team tournaments must be invited by team_season_id",
                    "invalid_target",
                ),
            )));
        }
        if !tournament.participant_type.requires_team_size() && user_id.is_none() {
            return Err(DomainError::Validation(ValidationError::field(
                FieldError::new(
                    "user_id",
                    "individual tournaments must be invited by user_id",
                    "invalid_target",
                ),
            )));
        }

        self.invitation_repo
            .create(CreateTournamentInvitation {
                tournament_id,
                user_id,
                team_season_id,
                message,
                invited_by,
            })
            .await
    }

    /// List a tournament's invitations.
    pub async fn list_invitations(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentInvitation>, DomainError> {
        // 404 rather than an empty list for a tournament that does not exist.
        let _ = self.get_tournament(tournament_id).await?;
        self.invitation_repo.list_by_tournament(tournament_id).await
    }

    /// Revoke an invitation.
    ///
    /// Revoking does not touch an existing registration: an organiser who
    /// wants someone out of a tournament they already entered uses
    /// reject/disqualify. Revoking only closes the door to a *future*
    /// registration.
    pub async fn revoke_invitation(
        &self,
        tournament_id: TournamentId,
        invitation_id: TournamentInvitationId,
    ) -> Result<TournamentInvitation, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::TournamentInvitationNotFound(invitation_id))?;

        // Resolve against the tournament that owns the invitation, so an
        // organiser of tournament A cannot revoke an invitation belonging
        // to tournament B by crafting the URL.
        if invitation.tournament_id != tournament_id {
            return Err(DomainError::TournamentInvitationNotFound(invitation_id));
        }

        self.invitation_repo.revoke(invitation_id).await
    }

    /// Get registrations for a tournament.
    pub async fn get_registrations(
        &self,
        tournament_id: TournamentId,
        status_filter: Option<TournamentRegistrationStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TournamentRegistration>, i64), DomainError> {
        self.registration_repo
            .list_by_tournament(tournament_id, status_filter, limit, offset)
            .await
    }

    /// Check in a participant.
    pub async fn check_in(
        &self,
        registration_id: portal_core::TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentRegistration, DomainError> {
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))?;

        let tournament = self.get_tournament(registration.tournament_id).await?;

        // Check if check-in is open
        if !tournament.is_check_in_open() {
            return Err(DomainError::InvalidState(
                "Check-in is not currently open".to_string(),
            ));
        }

        // Check if already checked in
        if registration.checked_in {
            return Err(DomainError::Conflict("Already checked in".to_string()));
        }

        self.registration_repo
            .check_in(registration_id, checked_in_by)
            .await
    }

    /// Start a tournament by generating brackets.
    pub async fn start_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let tournament = self.get_tournament(id).await?;

        // Validate tournament can start
        if tournament.status != TournamentStatus::Scheduled
            && tournament.status != TournamentStatus::Registration
        {
            return Err(DomainError::InvalidState(format!(
                "Cannot start tournament in {} status",
                tournament.status
            )));
        }

        // Get confirmed participants
        let participants = if tournament.check_in_required {
            self.registration_repo.list_checked_in(id).await?
        } else {
            self.registration_repo.list_seeded(id).await?
        };

        // Check minimum participants
        if participants.len() < tournament.min_participants as usize {
            return Err(DomainError::InsufficientParticipants);
        }

        // Atomically claim the start. Only one caller can win the transition
        // scheduled/registration -> in_progress; a concurrent second start (or
        // a retried one) gets None and must NOT build a second bracket set.
        let Some(claimed) = self.tournament_repo.try_claim_start(id).await? else {
            // Already claimed/started by another request — idempotent no-op.
            return self.get_tournament(id).await;
        };

        // Crash-recovery guard: a previous attempt may have built brackets
        // before it crashed (status was reset to a pre-start value, so the CAS
        // above succeeded). If a bracket set already exists, do NOT rebuild —
        // the start is idempotent on retry.
        let existing_brackets = self.bracket_repo.list_by_tournament(id).await?;
        if !existing_brackets.is_empty() {
            return Ok(claimed);
        }

        let seeded_participants = BracketGenerator::prepare_participants(participants);

        // Groups + Playoffs creates its own stages, so handle separately
        if tournament.format == TournamentFormat::GroupsAndPlayoffs {
            self.start_groups_and_playoffs(id, &tournament, seeded_participants)
                .await?;
            return self.tournament_repo.mark_started(id).await;
        }

        // Get or create the main stage (for non-Groups+Playoffs formats)
        let stage_format = match tournament.format {
            TournamentFormat::SingleElimination => StageFormat::SingleElimination,
            TournamentFormat::DoubleElimination => StageFormat::DoubleElimination,
            TournamentFormat::RoundRobin => StageFormat::RoundRobin,
            TournamentFormat::Swiss => StageFormat::Swiss,
            // Handled by the early start_groups_and_playoffs branch above —
            // reaching this arm would mean that guard was removed.
            TournamentFormat::GroupsAndPlayoffs => {
                unreachable!("GroupsAndPlayoffs is handled by start_groups_and_playoffs")
            }
        };

        let stages = self.stage_repo.list_by_tournament(id).await?;
        let stage = if stages.is_empty() {
            self.stage_repo
                .create(CreateTournamentStage {
                    tournament_id: id,
                    name: "Main Bracket".to_string(),
                    stage_order: 1,
                    format: stage_format,
                    // Inherit the tournament's format_settings so per-round
                    // overrides set at tournament creation (final_format,
                    // round_formats) reach the auto-created stage; the plan
                    // parser ignores foreign keys like swiss max_rounds.
                    format_settings: tournament.format_settings.clone(),
                    advancement_count: None,
                    advancement_rule: portal_core::types::AdvancementRule::TopN,
                    match_format: Some(tournament.default_match_format),
                    map_veto_format: tournament.default_map_veto_format.clone(),
                    starts_at: tournament.starts_at,
                    ends_at: None,
                })
                .await?
        } else {
            stages.into_iter().next().unwrap()
        };

        match tournament.format {
            TournamentFormat::SingleElimination => {
                self.start_single_elimination(id, &tournament, &stage, seeded_participants)
                    .await?;
            }
            TournamentFormat::DoubleElimination => {
                self.start_double_elimination(id, &tournament, &stage, seeded_participants)
                    .await?;
            }
            TournamentFormat::RoundRobin => {
                self.start_round_robin(id, &tournament, &stage, seeded_participants)
                    .await?;
            }
            TournamentFormat::Swiss => {
                self.start_swiss(id, &tournament, &stage, seeded_participants)
                    .await?;
            }
            TournamentFormat::GroupsAndPlayoffs => unreachable!("handled above"),
        }

        // Update stage status
        self.stage_repo
            .update_status(stage.id, StageStatus::Active)
            .await?;

        // Mark tournament as started
        self.tournament_repo.mark_started(id).await
    }

    /// Start a single elimination tournament.
    async fn start_single_elimination(
        &self,
        id: TournamentId,
        tournament: &Tournament,
        stage: &TournamentStage,
        seeded_participants: Vec<crate::entities::tournament::SeededParticipant>,
    ) -> Result<(), DomainError> {
        // Create bracket entry
        let bracket = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id: stage.id,
                tournament_id: id,
                name: "Main Bracket".to_string(),
                bracket_type: BracketType::SingleElim,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        // Generate the bracket structure
        let generated = BracketGenerator::single_elimination(
            id,
            stage.id,
            bracket.id,
            seeded_participants,
            &Self::stage_format_plan(tournament, stage)?,
        )?;

        // Update bracket with total rounds
        self.bracket_repo
            .update(
                bracket.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(generated.total_rounds),
                    current_round: Some(1),
                },
            )
            .await?;

        // Create matches
        let matches = self.match_repo.bulk_create(generated.matches).await?;

        // Create a position -> match ID mapping
        let position_to_match = Self::build_position_map(&matches);

        // Apply initial assignments and byes
        self.apply_initial_assignments(&generated.initial_assignments, &position_to_match)
            .await?;
        self.apply_byes(&generated.byes, &position_to_match).await?;

        // Link matches: R{r}M{m} winner → R{r+1}M{ceil(m/2)}
        for match_ in &matches {
            let pos = &match_.bracket_position;
            let parts: Vec<&str> = pos.split('M').collect();
            if parts.len() != 2 {
                continue;
            }
            let round: i32 = parts[0].trim_start_matches('R').parse().unwrap_or(0);
            let match_in_round: i32 = parts[1].parse().unwrap_or(0);
            if round == 0 || match_in_round == 0 {
                continue;
            }

            let next_round = round + 1;
            let next_match_in_round = (match_in_round + 1) / 2;
            let next_pos = format!("R{next_round}M{next_match_in_round}");

            if let Some(&next_match_id) = position_to_match.get(&next_pos) {
                self.match_repo
                    .set_progression_links(match_.id, Some(next_match_id), None)
                    .await?;
            }
        }

        // Mark matches with both participants as Ready
        self.mark_ready_matches(bracket.id).await?;

        Ok(())
    }

    /// Start a double elimination tournament.
    async fn start_double_elimination(
        &self,
        id: TournamentId,
        tournament: &Tournament,
        stage: &TournamentStage,
        seeded_participants: Vec<crate::entities::tournament::SeededParticipant>,
    ) -> Result<(), DomainError> {
        // Create 3 brackets: Winners, Losers, Grand Final
        let wb = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id: stage.id,
                tournament_id: id,
                name: "Winners Bracket".to_string(),
                bracket_type: BracketType::Winners,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        let lb = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id: stage.id,
                tournament_id: id,
                name: "Losers Bracket".to_string(),
                bracket_type: BracketType::Losers,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        let gf = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id: stage.id,
                tournament_id: id,
                name: "Grand Final".to_string(),
                bracket_type: BracketType::GrandFinal,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        // Generate the DE bracket structure
        let generated = BracketGenerator::double_elimination(
            id,
            stage.id,
            wb.id,
            lb.id,
            gf.id,
            seeded_participants,
            &Self::stage_format_plan(tournament, stage)?,
        )?;

        // Update bracket round counts
        self.bracket_repo
            .update(
                wb.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(generated.winners_bracket.total_rounds),
                    current_round: Some(1),
                },
            )
            .await?;

        if generated.losers_bracket.total_rounds > 0 {
            self.bracket_repo
                .update(
                    lb.id,
                    crate::repositories::tournament::UpdateTournamentBracket {
                        name: None,
                        total_rounds: Some(generated.losers_bracket.total_rounds),
                        current_round: Some(1),
                    },
                )
                .await?;
        }

        self.bracket_repo
            .update(
                gf.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(1),
                    current_round: Some(1),
                },
            )
            .await?;

        // Create matches for all 3 brackets
        let wb_matches = self
            .match_repo
            .bulk_create(generated.winners_bracket.matches)
            .await?;

        let lb_matches = if generated.losers_bracket.matches.is_empty() {
            Vec::new()
        } else {
            self.match_repo
                .bulk_create(generated.losers_bracket.matches)
                .await?
        };

        let gf_matches = self
            .match_repo
            .bulk_create(generated.grand_final.matches)
            .await?;

        // Build unified position → match ID map across all brackets
        let mut position_to_match = Self::build_position_map(&wb_matches);
        position_to_match.extend(Self::build_position_map(&lb_matches));
        position_to_match.extend(Self::build_position_map(&gf_matches));

        // Apply initial assignments and byes (WB only)
        self.apply_initial_assignments(
            &generated.winners_bracket.initial_assignments,
            &position_to_match,
        )
        .await?;
        self.apply_byes(&generated.winners_bracket.byes, &position_to_match)
            .await?;

        // =====================================================================
        // Build progression links
        // =====================================================================
        // We need to collect (winner_to, loser_to) for each match before writing,
        // because set_progression_links sets both fields atomically.

        let mut progression: std::collections::HashMap<
            TournamentMatchId,
            (Option<TournamentMatchId>, Option<TournamentMatchId>),
        > = std::collections::HashMap::new();

        // WB intra-bracket: WR{r}M{m} winner → WR{r+1}M{ceil(m/2)}
        for match_ in &wb_matches {
            let (round, match_in_round) = Self::parse_round_match(&match_.bracket_position, "WR");
            if round == 0 {
                continue;
            }
            let next_pos = format!("WR{}M{}", round + 1, (match_in_round + 1) / 2);
            if let Some(&next_id) = position_to_match.get(&next_pos) {
                let entry = progression.entry(match_.id).or_insert((None, None));
                entry.0 = Some(next_id);
            }
        }

        // LB intra-bracket progression
        for match_ in &lb_matches {
            let (lb_round, match_in_round) =
                Self::parse_round_match(&match_.bracket_position, "LR");
            if lb_round == 0 {
                continue;
            }

            // Determine next LB position
            let next_pos = if lb_round % 2 == 1 && lb_round > 1 {
                // Odd (survivor) round: feeds into next even (dropper) round, same index
                format!("LR{}M{match_in_round}", lb_round + 1)
            } else if lb_round == 1 {
                // LR1 feeds into LR2 at the same match index
                format!("LR2M{match_in_round}")
            } else {
                // Even (dropper) round: feeds into next odd (survivor) round
                // Two matches feed into one: match_in_round ceil(m/2)
                format!("LR{}M{}", lb_round + 1, (match_in_round + 1) / 2)
            };

            if let Some(&next_id) = position_to_match.get(&next_pos) {
                let entry = progression.entry(match_.id).or_insert((None, None));
                entry.0 = Some(next_id);
            }
        }

        // Cross-bracket links
        for link in &generated.cross_bracket_links {
            let source_id = position_to_match.get(&link.source_bracket_position);
            let target_id = position_to_match.get(&link.target_bracket_position);

            if let (Some(&src), Some(&tgt)) = (source_id, target_id) {
                let entry = progression.entry(src).or_insert((None, None));
                match link.link_type {
                    CrossLinkType::LoserDropsTo => {
                        entry.1 = Some(tgt);
                    }
                    CrossLinkType::WinnerAdvancesTo => {
                        entry.0 = Some(tgt);
                    }
                }
            }
        }

        // Write all progression links
        for (match_id, (winner_to, loser_to)) in &progression {
            self.match_repo
                .set_progression_links(*match_id, *winner_to, *loser_to)
                .await?;
        }

        // Mark matches with both participants as Ready
        self.mark_ready_matches(wb.id).await?;

        Ok(())
    }

    /// Start a round robin tournament.
    async fn start_round_robin(
        &self,
        id: TournamentId,
        tournament: &Tournament,
        stage: &TournamentStage,
        seeded_participants: Vec<crate::entities::tournament::SeededParticipant>,
    ) -> Result<(), DomainError> {
        // Create bracket entry
        let bracket = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id: stage.id,
                tournament_id: id,
                name: "Round Robin".to_string(),
                bracket_type: BracketType::RoundRobin,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        // Generate all RR matches
        let generated = BracketGenerator::round_robin(
            id,
            stage.id,
            bracket.id,
            seeded_participants.clone(),
            &Self::stage_format_plan(tournament, stage)?,
        )?;

        // Update bracket with total rounds
        self.bracket_repo
            .update(
                bracket.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(generated.total_rounds),
                    current_round: Some(1),
                },
            )
            .await?;

        // Create matches
        let matches = self.match_repo.bulk_create(generated.matches).await?;

        // Create position → match ID mapping
        let position_to_match = Self::build_position_map(&matches);

        // Apply all initial assignments (every match gets both participants)
        self.apply_initial_assignments(&generated.initial_assignments, &position_to_match)
            .await?;

        // Initialize standings for all participants
        let standings: Vec<CreateTournamentStanding> = seeded_participants
            .iter()
            .enumerate()
            .map(|(i, p)| CreateTournamentStanding {
                bracket_id: bracket.id,
                registration_id: p.registration_id,
                position: (i + 1) as i32,
            })
            .collect();
        self.standings_repo.bulk_create(standings).await?;

        // Mark matches with both participants as Ready
        self.mark_ready_matches(bracket.id).await?;

        Ok(())
    }

    /// Start a Swiss system tournament.
    async fn start_swiss(
        &self,
        id: TournamentId,
        tournament: &Tournament,
        stage: &TournamentStage,
        seeded_participants: Vec<crate::entities::tournament::SeededParticipant>,
    ) -> Result<(), DomainError> {
        let n = seeded_participants.len();

        // Determine max rounds from format_settings or default to ceil(log2(N))
        let max_rounds = tournament
            .format_settings
            .get("max_rounds")
            .and_then(serde_json::Value::as_i64)
            .map_or_else(|| (n as f64).log2().ceil() as i32, |v| v as i32);

        // Create bracket entry
        let bracket = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id: stage.id,
                tournament_id: id,
                name: "Swiss".to_string(),
                bracket_type: BracketType::Swiss,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        // Generate R1 matches (top-half vs bottom-half)
        let (generated, bye_participant) = BracketGenerator::swiss_initial_round(
            id,
            stage.id,
            bracket.id,
            seeded_participants.clone(),
            &Self::stage_format_plan(tournament, stage)?,
        )?;

        // Update bracket with total rounds and current round
        self.bracket_repo
            .update(
                bracket.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(max_rounds),
                    current_round: Some(1),
                },
            )
            .await?;

        // Create R1 matches
        let matches = self.match_repo.bulk_create(generated.matches).await?;
        let position_to_match = Self::build_position_map(&matches);
        self.apply_initial_assignments(&generated.initial_assignments, &position_to_match)
            .await?;

        // Initialize standings for all participants
        let standings: Vec<CreateTournamentStanding> = seeded_participants
            .iter()
            .enumerate()
            .map(|(i, p)| CreateTournamentStanding {
                bracket_id: bracket.id,
                registration_id: p.registration_id,
                position: (i + 1) as i32,
            })
            .collect();
        self.standings_repo.bulk_create(standings).await?;

        // Award the bye if the participant count is odd.
        //
        // A bye has no `tournament_matches` row, so the derived standings
        // recompute has nothing to reconstruct it from — it is recorded
        // explicitly (idempotently, keyed on bracket+registration+round)
        // and then folded in by the recompute.
        if let Some(bye_id) = bye_participant {
            self.standings_repo
                .record_bye(bracket.id, bye_id, 1)
                .await?;
            self.standings_repo.recompute_bracket(bracket.id).await?;
        }

        // Mark matches with both participants as Ready
        self.mark_ready_matches(bracket.id).await?;

        Ok(())
    }

    /// Start a Groups + Playoffs tournament.
    ///
    /// Creates two stages:
    /// 1. Group Stage (active): K groups, each a RR or Swiss bracket
    /// 2. Playoff Stage (pending): SE or DE bracket, generated when groups complete
    async fn start_groups_and_playoffs(
        &self,
        id: TournamentId,
        tournament: &Tournament,
        seeded_participants: Vec<crate::entities::tournament::SeededParticipant>,
    ) -> Result<(), DomainError> {
        use super::bracket_generator::groups::{
            self, GroupStageFormat, GroupsConfig, PlayoffFormat,
        };

        let config = GroupsConfig::from_format_settings(
            &tournament.format_settings,
            seeded_participants.len(),
        )?;

        // Validate minimum group sizes
        let min_per_group = seeded_participants.len() / config.group_count;
        if min_per_group < 2 {
            return Err(DomainError::InvalidState(format!(
                "Not enough participants ({}) for {} groups (need at least 2 per group)",
                seeded_participants.len(),
                config.group_count
            )));
        }

        if config.advance_per_group > min_per_group {
            return Err(DomainError::InvalidState(format!(
                "advance_per_group ({}) exceeds minimum group size ({})",
                config.advance_per_group, min_per_group
            )));
        }

        // Distribute into groups via snake-draft
        let group_participants =
            groups::distribute_into_groups(seeded_participants, config.group_count)?;

        // Determine group stage format
        let group_stage_format = match config.group_format {
            GroupStageFormat::RoundRobin => StageFormat::GroupStage,
            GroupStageFormat::Swiss => StageFormat::GroupStage,
        };

        // Per-phase formats: the groups config may split best-of between the
        // group stage and the playoffs; each phase's resolved format is
        // persisted onto its stage row so later stage-driven generation
        // (playoff progression) and the API read model see the same truth.
        let group_plan = config.group_format_plan(tournament.default_match_format);
        let playoff_default = config
            .playoff_match_format
            .unwrap_or(tournament.default_match_format);

        // Create Group Stage (stage_order=1)
        let group_stage = self
            .stage_repo
            .create(CreateTournamentStage {
                tournament_id: id,
                name: "Group Stage".to_string(),
                stage_order: 1,
                format: group_stage_format,
                format_settings: serde_json::json!({
                    "group_format": match config.group_format {
                        GroupStageFormat::RoundRobin => "round_robin",
                        GroupStageFormat::Swiss => "swiss",
                    }
                }),
                advancement_count: Some(config.advance_per_group as i32),
                advancement_rule: AdvancementRule::TopNPerGroup,
                match_format: Some(group_plan.default),
                map_veto_format: tournament.default_map_veto_format.clone(),
                starts_at: tournament.starts_at,
                ends_at: None,
            })
            .await?;

        // Determine playoff stage format
        let playoff_stage_format = match config.playoff_format {
            PlayoffFormat::SingleElimination => StageFormat::SingleElimination,
            PlayoffFormat::DoubleElimination => StageFormat::DoubleElimination,
        };

        // Create Playoff Stage (stage_order=2, pending). Its final/grand-final
        // overrides go into the stage's format_settings — progression builds
        // its format plan from the stage row when the playoffs generate.
        let _playoff_stage = self
            .stage_repo
            .create(CreateTournamentStage {
                tournament_id: id,
                name: "Playoffs".to_string(),
                stage_order: 2,
                format: playoff_stage_format,
                format_settings: config.playoff_stage_settings(),
                advancement_count: None,
                advancement_rule: AdvancementRule::TopN,
                match_format: Some(playoff_default),
                map_veto_format: tournament.default_map_veto_format.clone(),
                starts_at: None,
                ends_at: None,
            })
            .await?;

        // Create a bracket for each group
        for (group_idx, group) in group_participants.into_iter().enumerate() {
            let group_num = (group_idx + 1) as i32;
            let label = groups::group_label(group_idx);

            let bracket_type = match config.group_format {
                GroupStageFormat::RoundRobin => BracketType::RoundRobin,
                GroupStageFormat::Swiss => BracketType::Swiss,
            };

            let bracket = self
                .bracket_repo
                .create(CreateTournamentBracket {
                    stage_id: group_stage.id,
                    tournament_id: id,
                    name: format!("Group {label}"),
                    bracket_type,
                    total_rounds: 0,
                    group_number: Some(group_num),
                })
                .await?;

            let mut bye_to_record: Option<portal_core::TournamentRegistrationId> = None;

            match config.group_format {
                GroupStageFormat::RoundRobin => {
                    // Generate all RR matches for this group
                    let generated = BracketGenerator::round_robin(
                        id,
                        group_stage.id,
                        bracket.id,
                        group.clone(),
                        &group_plan,
                    )?;

                    self.bracket_repo
                        .update(
                            bracket.id,
                            crate::repositories::tournament::UpdateTournamentBracket {
                                name: None,
                                total_rounds: Some(generated.total_rounds),
                                current_round: Some(1),
                            },
                        )
                        .await?;

                    let matches = self.match_repo.bulk_create(generated.matches).await?;
                    let position_to_match = Self::build_position_map(&matches);
                    self.apply_initial_assignments(
                        &generated.initial_assignments,
                        &position_to_match,
                    )
                    .await?;
                }
                GroupStageFormat::Swiss => {
                    // Generate only R1 for Swiss groups
                    let n = group.len();
                    let max_rounds = (n as f64).log2().ceil() as i32;

                    let (generated, bye_participant) = BracketGenerator::swiss_initial_round(
                        id,
                        group_stage.id,
                        bracket.id,
                        group.clone(),
                        &group_plan,
                    )?;

                    self.bracket_repo
                        .update(
                            bracket.id,
                            crate::repositories::tournament::UpdateTournamentBracket {
                                name: None,
                                total_rounds: Some(max_rounds),
                                current_round: Some(1),
                            },
                        )
                        .await?;

                    let matches = self.match_repo.bulk_create(generated.matches).await?;
                    let position_to_match = Self::build_position_map(&matches);
                    self.apply_initial_assignments(
                        &generated.initial_assignments,
                        &position_to_match,
                    )
                    .await?;

                    // A bye has no `tournament_matches` row, so the
                    // derived standings recompute has nothing to
                    // reconstruct it from — it is recorded explicitly
                    // below, once the standings rows it applies to exist.
                    bye_to_record = bye_participant;
                }
            }

            // Initialize standings for group participants
            let standings: Vec<CreateTournamentStanding> = group
                .iter()
                .enumerate()
                .map(|(i, p)| CreateTournamentStanding {
                    bracket_id: bracket.id,
                    registration_id: p.registration_id,
                    position: (i + 1) as i32,
                })
                .collect();
            self.standings_repo.bulk_create(standings).await?;

            if let Some(bye_id) = bye_to_record {
                self.standings_repo
                    .record_bye(bracket.id, bye_id, 1)
                    .await?;
                self.standings_repo.recompute_bracket(bracket.id).await?;
            }

            // Mark matches with both participants as Ready
            self.mark_ready_matches(bracket.id).await?;
        }

        // Activate group stage (playoff stage stays Pending)
        self.stage_repo
            .update_status(group_stage.id, StageStatus::Active)
            .await?;

        Ok(())
    }

    /// Generate the next Swiss round for a tournament.
    ///
    /// Validates that all current-round matches are complete, then generates
    /// the next round of Swiss pairings based on current standings.
    pub async fn generate_next_swiss_round(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let tournament = self.get_tournament(tournament_id).await?;

        // Validate tournament is started and Swiss
        if tournament.status != TournamentStatus::InProgress {
            return Err(DomainError::InvalidState(
                "Tournament must be in InProgress status".to_string(),
            ));
        }
        if tournament.format != TournamentFormat::Swiss
            && tournament.format != TournamentFormat::GroupsAndPlayoffs
        {
            return Err(DomainError::InvalidState(
                "Tournament is not Swiss format".to_string(),
            ));
        }

        // Get brackets
        let brackets = self.bracket_repo.list_by_tournament(tournament_id).await?;
        let bracket = brackets
            .into_iter()
            .find(|b| b.bracket_type == BracketType::Swiss)
            .ok_or_else(|| DomainError::InvalidState("No Swiss bracket found".to_string()))?;

        // Verify all current-round matches are complete
        let all_matches = self.match_repo.list_by_bracket(bracket.id).await?;
        let current_round_matches: Vec<&TournamentMatch> = all_matches
            .iter()
            .filter(|m| m.round == bracket.current_round)
            .collect();

        if current_round_matches.is_empty() {
            return Err(DomainError::InvalidState(
                "No matches found for current round".to_string(),
            ));
        }

        let all_complete = current_round_matches.iter().all(|m| m.is_complete());
        if !all_complete {
            return Err(DomainError::InvalidState(
                "Not all current-round matches are complete".to_string(),
            ));
        }

        // Check max rounds
        let next_round = bracket.current_round + 1;
        if next_round > bracket.total_rounds {
            return Err(DomainError::InvalidState(
                "Maximum number of rounds reached".to_string(),
            ));
        }

        // Get current standings
        let standings = self.standings_repo.list_by_bracket(bracket.id).await?;

        // Build completed pairings set from all matches
        let completed_pairings: Vec<(
            portal_core::TournamentRegistrationId,
            portal_core::TournamentRegistrationId,
        )> = all_matches
            .iter()
            .filter_map(|m| {
                match (
                    m.participant1_registration_id,
                    m.participant2_registration_id,
                ) {
                    (Some(a), Some(b)) => Some((a, b)),
                    _ => None,
                }
            })
            .collect();

        // Get registrations to look up names/logos. Exhaustive fetch
        // (P-186): with a capped page, any approved participant past the
        // cap fell out of reg_map and the filter_map below silently
        // dropped them from the pairing — data loss, not display. With
        // every approved row present, the residual drops below are
        // legitimate by construction: a standing whose registration is no
        // longer approved (disqualified, withdrawn) must not be paired.
        let registrations = self
            .registration_repo
            .list_all_by_tournament(tournament_id, Some(TournamentRegistrationStatus::Approved))
            .await?;

        let reg_map: std::collections::HashMap<
            portal_core::TournamentRegistrationId,
            &crate::entities::tournament::TournamentRegistration,
        > = registrations.iter().map(|r| (r.id, r)).collect();

        // Build SwissParticipantStanding list
        let swiss_standings: Vec<super::bracket_generator::SwissParticipantStanding> = standings
            .iter()
            .filter_map(|s| {
                let reg = reg_map.get(&s.registration_id)?;
                // Count how many rounds this participant has been paired in
                let matches_in = all_matches
                    .iter()
                    .filter(|m| {
                        m.participant1_registration_id == Some(s.registration_id)
                            || m.participant2_registration_id == Some(s.registration_id)
                    })
                    .count();
                let total_rounds_so_far = bracket.current_round as usize;
                let had_bye = matches_in < total_rounds_so_far;

                Some(super::bracket_generator::SwissParticipantStanding {
                    registration_id: s.registration_id,
                    participant_name: reg.participant_name.clone(),
                    participant_logo_url: reg.participant_logo_url.clone(),
                    seed: reg.seed.unwrap_or(s.position),
                    points: s.points,
                    buchholz_score: s.buchholz_score.unwrap_or(0.0),
                    had_bye,
                })
            })
            .collect();

        // Later rounds must match the stage's format plan, not the raw
        // tournament default — the stage may override the best-of.
        let stage = self
            .stage_repo
            .find_by_id(bracket.stage_id)
            .await?
            .ok_or_else(|| {
                DomainError::Internal(format!("Stage {} not found", bracket.stage_id))
            })?;
        let format_plan = Self::stage_format_plan(&tournament, &stage)?;

        // Generate next round
        let (generated, bye_participant) = BracketGenerator::swiss_next_round(
            tournament_id,
            bracket.stage_id,
            bracket.id,
            next_round,
            swiss_standings,
            &completed_pairings,
            &format_plan,
        )?;

        // Create matches
        let new_matches = self.match_repo.bulk_create(generated.matches).await?;
        let position_to_match = Self::build_position_map(&new_matches);
        self.apply_initial_assignments(&generated.initial_assignments, &position_to_match)
            .await?;

        // Record the round's bye, if any. Byes have no match row, so the
        // derived standings recompute folds them in from this table
        // instead. Recording is keyed on (bracket, registration, round),
        // so re-running round generation cannot award it twice.
        if let Some(bye_id) = bye_participant {
            self.standings_repo
                .record_bye(bracket.id, bye_id, next_round)
                .await?;
        }
        self.standings_repo.recompute_bracket(bracket.id).await?;

        // Update bracket current_round
        self.bracket_repo
            .update(
                bracket.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: None,
                    current_round: Some(next_round),
                },
            )
            .await?;

        // Mark matches with both participants as Ready
        self.mark_ready_matches(bracket.id).await?;

        Ok(new_matches)
    }

    // =========================================================================
    // HELPERS (delegate to shared free functions in helpers module)
    // =========================================================================

    /// Resolve the match-format plan for a stage: the stage's own format
    /// override (else the tournament default) refined by any per-round /
    /// final overrides in the stage's `format_settings`.
    fn stage_format_plan(
        tournament: &Tournament,
        stage: &TournamentStage,
    ) -> Result<MatchFormatPlan, DomainError> {
        MatchFormatPlan::from_settings(
            stage.effective_match_format(tournament.default_match_format),
            Some(&stage.format_settings),
        )
        .map_err(|e| DomainError::InvalidState(format!("invalid stage format settings: {e}")))
    }

    /// Build a position → match ID mapping from a list of matches.
    fn build_position_map(
        matches: &[TournamentMatch],
    ) -> std::collections::HashMap<String, TournamentMatchId> {
        super::helpers::build_position_map(matches)
    }

    /// Apply initial participant assignments to matches.
    async fn apply_initial_assignments(
        &self,
        assignments: &[super::bracket_generator::InitialAssignment],
        position_to_match: &std::collections::HashMap<String, TournamentMatchId>,
    ) -> Result<(), DomainError> {
        super::helpers::apply_initial_assignments(
            self.match_repo.as_ref(),
            assignments,
            position_to_match,
        )
        .await
    }

    /// Scan a bracket for Pending matches that have both participants and mark them Ready.
    async fn mark_ready_matches(&self, bracket_id: TournamentBracketId) -> Result<(), DomainError> {
        let matches = self.match_repo.list_by_bracket(bracket_id).await?;
        for m in matches {
            if m.status == TournamentMatchStatus::Pending && m.has_both_participants() {
                self.match_repo
                    .update_status(m.id, TournamentMatchStatus::Ready)
                    .await?;
            }
        }
        Ok(())
    }

    /// Apply bye advancements (participants who auto-advance).
    async fn apply_byes(
        &self,
        byes: &[super::bracket_generator::ByeInfo],
        position_to_match: &std::collections::HashMap<String, TournamentMatchId>,
    ) -> Result<(), DomainError> {
        super::helpers::apply_byes(self.match_repo.as_ref(), byes, position_to_match).await
    }

    /// Parse a bracket position like "WR2M3" or "LR1M2" into (round, match_in_round).
    /// Returns (0, 0) if parsing fails.
    fn parse_round_match(position: &str, prefix: &str) -> (i32, i32) {
        super::helpers::parse_round_match(position, prefix)
    }

    /// Get bracket for a tournament.
    pub async fn get_bracket(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentBracket>, DomainError> {
        self.bracket_repo.list_by_tournament(tournament_id).await
    }

    /// Get matches for a bracket.
    pub async fn get_bracket_matches(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        self.match_repo.list_by_bracket(bracket_id).await
    }

    /// Get all matches for a tournament.
    pub async fn get_tournament_matches(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        self.match_repo.list_by_tournament(tournament_id).await
    }

    /// Get a single match by ID, verifying it belongs to the tournament.
    pub async fn get_tournament_match(
        &self,
        tournament_id: TournamentId,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(match_id))?;

        if match_.tournament_id != tournament_id {
            return Err(DomainError::TournamentMatchNotFound(match_id));
        }

        Ok(match_)
    }

    /// Get all matches for a player across tournaments.
    pub async fn get_player_matches(
        &self,
        player_id: PlayerId,
        status: Option<TournamentMatchStatus>,
        tournament_id: Option<TournamentId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        self.match_repo
            .list_by_player(player_id, status, tournament_id, limit, offset)
            .await
    }

    /// Delete a tournament (only if in draft status).
    pub async fn delete_tournament(&self, id: TournamentId) -> Result<(), DomainError> {
        let tournament = self.get_tournament(id).await?;

        if tournament.status != TournamentStatus::Draft {
            return Err(DomainError::InvalidState(
                "Can only delete tournaments in draft status".to_string(),
            ));
        }

        self.tournament_repo.delete(id).await
    }
}

// Manual Clone implementation since derive(Clone) doesn't work with generic bounds
impl<TR, TSR, TBR, TRR, TMR, TSTR, TMPR, TIR> Clone
    for TournamentService<TR, TSR, TBR, TRR, TMR, TSTR, TMPR, TIR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TMR: TournamentMatchRepository,
    TSTR: TournamentStandingsRepository,
    TMPR: TournamentMapPoolRepository,
    TIR: TournamentInvitationRepository,
{
    fn clone(&self) -> Self {
        Self {
            tournament_repo: Arc::clone(&self.tournament_repo),
            stage_repo: Arc::clone(&self.stage_repo),
            bracket_repo: Arc::clone(&self.bracket_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            match_repo: Arc::clone(&self.match_repo),
            standings_repo: Arc::clone(&self.standings_repo),
            map_pool_repo: Arc::clone(&self.map_pool_repo),
            invitation_repo: Arc::clone(&self.invitation_repo),
        }
    }
}
