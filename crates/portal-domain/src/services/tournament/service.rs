//! Tournament service for managing tournament lifecycle.

use std::sync::Arc;

use portal_core::types::{
    BracketType, MatchFormat, StageFormat, StageStatus, TournamentRegistrationStatus,
    TournamentStatus,
};
use portal_core::{DomainError, TournamentBracketId, TournamentId, UserId};

use crate::entities::tournament::{
    CreateTournamentCommand, Tournament, TournamentBracket, TournamentMatch, TournamentRegistration,
    TournamentStage, UpdateTournamentCommand,
};
use crate::repositories::tournament::{
    CreateTournament, CreateTournamentBracket, CreateTournamentRegistration, CreateTournamentStage,
    ParticipantSlot, TournamentBracketRepository, TournamentFilters, TournamentMatchRepository,
    TournamentRegistrationRepository, TournamentRepository, TournamentStageRepository,
    UpdateTournament,
};

use super::bracket_generator::BracketGenerator;

/// Service for tournament management.
pub struct TournamentService<TR, TSR, TBR, TRR, TMR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TMR: TournamentMatchRepository,
{
    tournament_repo: Arc<TR>,
    stage_repo: Arc<TSR>,
    bracket_repo: Arc<TBR>,
    registration_repo: Arc<TRR>,
    match_repo: Arc<TMR>,
}

impl<TR, TSR, TBR, TRR, TMR> TournamentService<TR, TSR, TBR, TRR, TMR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TMR: TournamentMatchRepository,
{
    /// Create a new tournament service.
    pub const fn new(
        tournament_repo: Arc<TR>,
        stage_repo: Arc<TSR>,
        bracket_repo: Arc<TBR>,
        registration_repo: Arc<TRR>,
        match_repo: Arc<TMR>,
    ) -> Self {
        Self {
            tournament_repo,
            stage_repo,
            bracket_repo,
            registration_repo,
            match_repo,
        }
    }

    /// Create a new tournament.
    pub async fn create_tournament(
        &self,
        cmd: CreateTournamentCommand,
        created_by: UserId,
    ) -> Result<Tournament, DomainError> {
        // Check slug uniqueness
        if self.tournament_repo.slug_exists(&cmd.slug).await? {
            return Err(DomainError::Conflict(format!(
                "Tournament with slug '{}' already exists",
                cmd.slug
            )));
        }

        // Create the tournament
        let tournament = self
            .tournament_repo
            .create(CreateTournament {
                game_id: cmd.game_id,
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

        Ok(tournament)
    }

    /// Get a tournament by ID.
    pub async fn get_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        self.tournament_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(id.to_string()))
    }

    /// Get a tournament by slug.
    pub async fn get_tournament_by_slug(&self, slug: &str) -> Result<Tournament, DomainError> {
        self.tournament_repo
            .find_by_slug(slug)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(slug.to_string()))
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

        // Check slug uniqueness if changing
        if let Some(ref new_slug) = cmd.slug {
            if new_slug != &tournament.slug && self.tournament_repo.slug_exists(new_slug).await? {
                return Err(DomainError::Conflict(format!(
                    "Tournament with slug '{new_slug}' already exists"
                )));
            }
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
                    settings: cmd.settings,
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

        if !tournament.status.can_transition_to(TournamentStatus::Registration) {
            return Err(DomainError::InvalidTournamentTransition {
                from: tournament.status.to_string(),
                to: TournamentStatus::Registration.to_string(),
            });
        }

        self.tournament_repo
            .update_status(id, TournamentStatus::Registration)
            .await
    }

    /// Create a stage for a tournament.
    pub async fn create_stage(
        &self,
        tournament_id: TournamentId,
        name: String,
        stage_order: i32,
        format: StageFormat,
        format_settings: Option<serde_json::Value>,
        advancement_count: Option<i32>,
        match_format: Option<MatchFormat>,
    ) -> Result<TournamentStage, DomainError> {
        let tournament = self.get_tournament(tournament_id).await?;

        // Only allow adding stages before tournament starts
        if tournament.has_started() {
            return Err(DomainError::TournamentAlreadyStarted);
        }

        self.stage_repo
            .create(CreateTournamentStage {
                tournament_id,
                name,
                stage_order,
                format,
                format_settings: format_settings.unwrap_or_else(|| serde_json::json!({})),
                advancement_count,
                advancement_rule: portal_core::types::AdvancementRule::TopN,
                match_format,
                map_veto_format: None,
                starts_at: None,
                ends_at: None,
            })
            .await
    }

    /// Get stages for a tournament.
    pub async fn get_stages(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentStage>, DomainError> {
        self.stage_repo.list_by_tournament(tournament_id).await
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

        // Check not already registered
        if self
            .registration_repo
            .find_by_team_season(tournament_id, team_season_id)
            .await?
            .is_some()
        {
            return Err(DomainError::AlreadyRegisteredForTournament);
        }

        // Check capacity
        let current_count = self.tournament_repo.count_registrations(tournament_id).await?;
        if current_count >= i64::from(tournament.max_participants) {
            return Err(DomainError::TournamentFull);
        }

        // Create registration
        self.registration_repo
            .create(CreateTournamentRegistration {
                tournament_id,
                team_season_id: Some(team_season_id),
                player_id: None,
                adhoc_team_id: None,
                participant_name,
                participant_logo_url,
                registered_by,
                seed_rating: None,
            })
            .await
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

        // Check not already registered
        if self
            .registration_repo
            .find_by_player(tournament_id, player_id)
            .await?
            .is_some()
        {
            return Err(DomainError::AlreadyRegisteredForTournament);
        }

        // Check capacity
        let current_count = self.tournament_repo.count_registrations(tournament_id).await?;
        if current_count >= i64::from(tournament.max_participants) {
            return Err(DomainError::TournamentFull);
        }

        // Create registration
        self.registration_repo
            .create(CreateTournamentRegistration {
                tournament_id,
                team_season_id: None,
                player_id: Some(player_id),
                adhoc_team_id: None,
                participant_name,
                participant_logo_url: None,
                registered_by,
                seed_rating: None,
            })
            .await
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
            .ok_or_else(|| DomainError::TournamentRegistrationNotFound(registration_id.to_string()))?;

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

        self.registration_repo.check_in(registration_id, checked_in_by).await
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

        // Get or create the main stage
        let stages = self.stage_repo.list_by_tournament(id).await?;
        let stage = if stages.is_empty() {
            // Create a default stage for simple tournaments
            self.stage_repo
                .create(CreateTournamentStage {
                    tournament_id: id,
                    name: "Main Bracket".to_string(),
                    stage_order: 1,
                    format: StageFormat::SingleElimination,
                    format_settings: serde_json::json!({}),
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

        // Generate bracket
        let seeded_participants = BracketGenerator::prepare_participants(participants);

        // Create bracket entry
        let bracket = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id: stage.id,
                tournament_id: id,
                name: "Main Bracket".to_string(),
                bracket_type: BracketType::SingleElim,
                total_rounds: 0, // Will be updated after generation
                group_number: None,
            })
            .await?;

        // Generate the bracket structure
        let generated = BracketGenerator::single_elimination(
            id,
            stage.id,
            bracket.id,
            seeded_participants.clone(),
            tournament.default_match_format,
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
        let position_to_match: std::collections::HashMap<String, portal_core::TournamentMatchId> =
            matches
                .iter()
                .map(|m| (m.bracket_position.clone(), m.id))
                .collect();

        // Apply initial assignments
        for assignment in generated.initial_assignments {
            if let Some(&match_id) = position_to_match.get(&assignment.bracket_position) {
                let slot = if assignment.slot == 1 {
                    ParticipantSlot::One
                } else {
                    ParticipantSlot::Two
                };

                self.match_repo
                    .assign_participant(
                        match_id,
                        slot,
                        assignment.participant.registration_id,
                        assignment.participant.participant_name,
                        assignment.participant.participant_logo_url,
                        Some(assignment.participant.seed),
                    )
                    .await?;
            }
        }

        // Apply byes (advance participants directly to next round)
        for bye in generated.byes {
            if let Some(&match_id) = position_to_match.get(&bye.advances_to_position) {
                let slot = if bye.slot == 1 {
                    ParticipantSlot::One
                } else {
                    ParticipantSlot::Two
                };

                self.match_repo
                    .assign_participant(
                        match_id,
                        slot,
                        bye.participant.registration_id,
                        bye.participant.participant_name,
                        bye.participant.participant_logo_url,
                        Some(bye.participant.seed),
                    )
                    .await?;
            }
        }

        // Link matches: set winner_progresses_to for each match.
        // For SE brackets: R{r}M{m} winner → R{r+1}M{ceil(m/2)}.
        for match_ in &matches {
            // Parse bracket_position "R{round}M{match_in_round}"
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

            // Compute next round position
            let next_round = round + 1;
            let next_match_in_round = (match_in_round + 1) / 2;
            let next_pos = format!("R{next_round}M{next_match_in_round}");

            if let Some(&next_match_id) = position_to_match.get(&next_pos) {
                self.match_repo
                    .set_progression_links(match_.id, Some(next_match_id), None)
                    .await?;
            }
            // No next match means this is the final — winner_progresses_to stays None.
        }

        // Update stage status
        self.stage_repo
            .update_status(stage.id, StageStatus::Active)
            .await?;

        // Mark tournament as started
        self.tournament_repo.mark_started(id).await
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
impl<TR, TSR, TBR, TRR, TMR> Clone for TournamentService<TR, TSR, TBR, TRR, TMR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TMR: TournamentMatchRepository,
{
    fn clone(&self) -> Self {
        Self {
            tournament_repo: Arc::clone(&self.tournament_repo),
            stage_repo: Arc::clone(&self.stage_repo),
            bracket_repo: Arc::clone(&self.bracket_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            match_repo: Arc::clone(&self.match_repo),
        }
    }
}
