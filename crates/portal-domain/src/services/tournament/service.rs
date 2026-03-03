//! Tournament service for managing tournament lifecycle.

use std::sync::Arc;

use portal_core::types::{
    BracketType, MatchFormat, StageFormat, StageStatus, TournamentFormat,
    TournamentRegistrationStatus, TournamentStatus,
};
use portal_core::{DomainError, TournamentBracketId, TournamentId, TournamentMatchId, UserId};

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

use super::bracket_generator::{BracketGenerator, CrossLinkType, GeneratedBracket};

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
        let stage_format = match tournament.format {
            TournamentFormat::SingleElimination => StageFormat::SingleElimination,
            TournamentFormat::DoubleElimination => StageFormat::DoubleElimination,
            _ => StageFormat::SingleElimination,
        };

        let stages = self.stage_repo.list_by_tournament(id).await?;
        let stage = if stages.is_empty() {
            self.stage_repo
                .create(CreateTournamentStage {
                    tournament_id: id,
                    name: "Main Bracket".to_string(),
                    stage_order: 1,
                    format: stage_format,
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

        let seeded_participants = BracketGenerator::prepare_participants(participants);

        match tournament.format {
            TournamentFormat::SingleElimination => {
                self.start_single_elimination(id, &tournament, &stage, seeded_participants)
                    .await?;
            }
            TournamentFormat::DoubleElimination => {
                self.start_double_elimination(id, &tournament, &stage, seeded_participants)
                    .await?;
            }
            _ => {
                return Err(DomainError::InvalidState(format!(
                    "Tournament format {} is not yet supported",
                    tournament.format
                )));
            }
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
            tournament.default_match_format,
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

        Ok(())
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    /// Build a position → match ID mapping from a list of matches.
    fn build_position_map(
        matches: &[TournamentMatch],
    ) -> std::collections::HashMap<String, TournamentMatchId> {
        matches
            .iter()
            .map(|m| (m.bracket_position.clone(), m.id))
            .collect()
    }

    /// Apply initial participant assignments to matches.
    async fn apply_initial_assignments(
        &self,
        assignments: &[super::bracket_generator::InitialAssignment],
        position_to_match: &std::collections::HashMap<String, TournamentMatchId>,
    ) -> Result<(), DomainError> {
        for assignment in assignments {
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
                        assignment.participant.participant_name.clone(),
                        assignment.participant.participant_logo_url.clone(),
                        Some(assignment.participant.seed),
                    )
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
        for bye in byes {
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
                        bye.participant.participant_name.clone(),
                        bye.participant.participant_logo_url.clone(),
                        Some(bye.participant.seed),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    /// Parse a bracket position like "WR2M3" or "LR1M2" into (round, match_in_round).
    /// Returns (0, 0) if parsing fails.
    fn parse_round_match(position: &str, prefix: &str) -> (i32, i32) {
        let stripped = position.strip_prefix(prefix).unwrap_or("");
        let parts: Vec<&str> = stripped.split('M').collect();
        if parts.len() != 2 {
            return (0, 0);
        }
        let round: i32 = parts[0].parse().unwrap_or(0);
        let match_in_round: i32 = parts[1].parse().unwrap_or(0);
        (round, match_in_round)
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
