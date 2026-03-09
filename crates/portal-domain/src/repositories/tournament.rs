//! Tournament repository traits.
//!
//! These repositories handle tournament data persistence:
//!   - `TournamentRepository`: Core tournament CRUD
//!   - `TournamentStageRepository`: Multi-stage tournament stages
//!   - `TournamentBracketRepository`: Bracket structures
//!   - `TournamentRegistrationRepository`: Participant registrations
//!   - `TournamentMatchRepository`: Matches within brackets
//!   - `TournamentMatchGameRepository`: Individual games in a match series

use crate::entities::tournament::{
    GameStatus, Tournament, TournamentBracket, TournamentMapPool, TournamentMatch,
    TournamentMatchGame, TournamentRegistration, TournamentStage, TournamentStanding,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::types::{
    AdvancementRule, BracketStatus, BracketType, MatchFormat, MatchParticipantSource,
    RegistrationType, SchedulingMode, StageFormat, StageStatus, TournamentFormat,
    TournamentMatchStatus, TournamentParticipantType, TournamentRegistrationStatus,
    TournamentStatus, WithdrawalPolicy,
};
use portal_core::{
    DemoMatchLinkId, DomainError, GameId, LeagueId, LeagueSeasonId, LeagueTeamSeasonId, PlayerId,
    TournamentBracketId, TournamentId, TournamentMapPoolId, TournamentMatchGameId,
    TournamentMatchId, TournamentRegistrationId, TournamentStageId, UserId,
};

// =============================================================================
// TOURNAMENT REPOSITORY
// =============================================================================

/// Repository trait for tournament operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentRepository: Send + Sync {
    /// Find a tournament by ID.
    async fn find_by_id(&self, id: TournamentId) -> Result<Option<Tournament>, DomainError>;

    /// Find a tournament by slug.
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Tournament>, DomainError>;

    /// Create a new tournament.
    async fn create(&self, tournament: CreateTournament) -> Result<Tournament, DomainError>;

    /// Update a tournament.
    async fn update(
        &self,
        id: TournamentId,
        update: UpdateTournament,
    ) -> Result<Tournament, DomainError>;

    /// Update tournament status.
    async fn update_status(
        &self,
        id: TournamentId,
        status: TournamentStatus,
    ) -> Result<Tournament, DomainError>;

    /// Update tournament logo URL.
    async fn update_logo(
        &self,
        id: TournamentId,
        logo_url: Option<String>,
    ) -> Result<Tournament, DomainError>;

    /// Update tournament banner URL.
    async fn update_banner(
        &self,
        id: TournamentId,
        banner_url: Option<String>,
    ) -> Result<Tournament, DomainError>;

    /// Set tournament started timestamp.
    async fn mark_started(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    /// Set tournament completed timestamp.
    async fn mark_completed(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    /// Set tournament published timestamp.
    async fn mark_published(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    /// List tournaments with filters and pagination.
    async fn list(
        &self,
        filters: TournamentFilters,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError>;

    /// List tournaments for a game.
    async fn list_by_game(
        &self,
        game_id: GameId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError>;

    /// List tournaments for a league.
    async fn list_by_league(
        &self,
        league_id: LeagueId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError>;

    /// List tournaments created by a user.
    async fn list_by_creator(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError>;

    /// Check if a slug is available.
    async fn slug_exists(&self, slug: &str) -> Result<bool, DomainError>;

    /// Count registrations for a tournament.
    async fn count_registrations(&self, id: TournamentId) -> Result<i64, DomainError>;

    /// Delete a tournament (only if draft status).
    async fn delete(&self, id: TournamentId) -> Result<(), DomainError>;
}

/// Data for creating a tournament.
#[derive(Debug, Clone)]
pub struct CreateTournament {
    pub game_id: GameId,
    pub league_id: Option<LeagueId>,
    pub season_id: Option<LeagueSeasonId>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub format: TournamentFormat,
    pub format_settings: serde_json::Value,
    pub participant_type: TournamentParticipantType,
    pub team_size: Option<i32>,
    pub min_participants: i32,
    pub max_participants: i32,
    pub registration_type: RegistrationType,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_required: bool,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub scheduling_mode: SchedulingMode,
    pub starts_at: Option<DateTime<Utc>>,
    pub default_match_format: MatchFormat,
    pub default_map_veto_format: Option<String>,
    pub withdrawal_policy: WithdrawalPolicy,
    pub rules_url: Option<String>,
    pub settings: serde_json::Value,
    pub created_by: UserId,
}

/// Data for updating a tournament.
#[derive(Debug, Clone, Default)]
pub struct UpdateTournament {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub format_settings: Option<serde_json::Value>,
    pub min_participants: Option<i32>,
    pub max_participants: Option<i32>,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_required: Option<bool>,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub timezone_hint: Option<String>,
    pub default_match_format: Option<MatchFormat>,
    pub default_map_veto_format: Option<String>,
    pub prize_pool: Option<serde_json::Value>,
    pub rules_url: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub withdrawal_policy: Option<WithdrawalPolicy>,
}

/// Filters for listing tournaments.
#[derive(Debug, Clone, Default)]
pub struct TournamentFilters {
    pub game_id: Option<GameId>,
    pub league_id: Option<LeagueId>,
    pub season_id: Option<LeagueSeasonId>,
    pub status: Option<TournamentStatus>,
    pub format: Option<TournamentFormat>,
    pub participant_type: Option<TournamentParticipantType>,
    pub search: Option<String>,
    pub upcoming: Option<bool>,
    pub active: Option<bool>,
}

// =============================================================================
// TOURNAMENT STAGE REPOSITORY
// =============================================================================

/// Repository trait for tournament stage operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentStageRepository: Send + Sync {
    /// Find a stage by ID.
    async fn find_by_id(
        &self,
        id: TournamentStageId,
    ) -> Result<Option<TournamentStage>, DomainError>;

    /// Create a new stage.
    async fn create(&self, stage: CreateTournamentStage) -> Result<TournamentStage, DomainError>;

    /// Update a stage.
    async fn update(
        &self,
        id: TournamentStageId,
        update: UpdateTournamentStage,
    ) -> Result<TournamentStage, DomainError>;

    /// Update stage status.
    async fn update_status(
        &self,
        id: TournamentStageId,
        status: StageStatus,
    ) -> Result<TournamentStage, DomainError>;

    /// List all stages for a tournament (ordered by `stage_order`).
    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentStage>, DomainError>;

    /// Get the next stage in order.
    async fn find_next_stage(
        &self,
        tournament_id: TournamentId,
        current_order: i32,
    ) -> Result<Option<TournamentStage>, DomainError>;

    /// Delete a stage.
    async fn delete(&self, id: TournamentStageId) -> Result<(), DomainError>;
}

/// Data for creating a tournament stage.
#[derive(Debug, Clone)]
pub struct CreateTournamentStage {
    pub tournament_id: TournamentId,
    pub name: String,
    pub stage_order: i32,
    pub format: StageFormat,
    pub format_settings: serde_json::Value,
    pub advancement_count: Option<i32>,
    pub advancement_rule: AdvancementRule,
    pub match_format: Option<MatchFormat>,
    pub map_veto_format: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

/// Data for updating a tournament stage.
#[derive(Debug, Clone, Default)]
pub struct UpdateTournamentStage {
    pub name: Option<String>,
    pub format_settings: Option<serde_json::Value>,
    pub advancement_count: Option<i32>,
    pub advancement_rule: Option<AdvancementRule>,
    pub match_format: Option<MatchFormat>,
    pub map_veto_format: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TOURNAMENT BRACKET REPOSITORY
// =============================================================================

/// Repository trait for tournament bracket operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentBracketRepository: Send + Sync {
    /// Find a bracket by ID.
    async fn find_by_id(
        &self,
        id: TournamentBracketId,
    ) -> Result<Option<TournamentBracket>, DomainError>;

    /// Create a new bracket.
    async fn create(&self, bracket: CreateTournamentBracket) -> Result<TournamentBracket, DomainError>;

    /// Update a bracket.
    async fn update(
        &self,
        id: TournamentBracketId,
        update: UpdateTournamentBracket,
    ) -> Result<TournamentBracket, DomainError>;

    /// Update bracket status.
    async fn update_status(
        &self,
        id: TournamentBracketId,
        status: BracketStatus,
    ) -> Result<TournamentBracket, DomainError>;

    /// Advance current round.
    async fn advance_round(&self, id: TournamentBracketId) -> Result<TournamentBracket, DomainError>;

    /// List all brackets for a stage.
    async fn list_by_stage(
        &self,
        stage_id: TournamentStageId,
    ) -> Result<Vec<TournamentBracket>, DomainError>;

    /// List all brackets for a tournament.
    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentBracket>, DomainError>;

    /// Delete a bracket.
    async fn delete(&self, id: TournamentBracketId) -> Result<(), DomainError>;
}

/// Data for creating a tournament bracket.
#[derive(Debug, Clone)]
pub struct CreateTournamentBracket {
    pub stage_id: TournamentStageId,
    pub tournament_id: TournamentId,
    pub name: String,
    pub bracket_type: BracketType,
    pub total_rounds: i32,
    pub group_number: Option<i32>,
}

/// Data for updating a tournament bracket.
#[derive(Debug, Clone, Default)]
pub struct UpdateTournamentBracket {
    pub name: Option<String>,
    pub total_rounds: Option<i32>,
    pub current_round: Option<i32>,
}

// =============================================================================
// TOURNAMENT REGISTRATION REPOSITORY
// =============================================================================

/// Repository trait for tournament registration operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentRegistrationRepository: Send + Sync {
    /// Find a registration by ID.
    async fn find_by_id(
        &self,
        id: TournamentRegistrationId,
    ) -> Result<Option<TournamentRegistration>, DomainError>;

    /// Find a registration for a team-season.
    async fn find_by_team_season(
        &self,
        tournament_id: TournamentId,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Option<TournamentRegistration>, DomainError>;

    /// Find a registration for a player.
    async fn find_by_player(
        &self,
        tournament_id: TournamentId,
        player_id: PlayerId,
    ) -> Result<Option<TournamentRegistration>, DomainError>;

    /// Create a new registration.
    async fn create(
        &self,
        registration: CreateTournamentRegistration,
    ) -> Result<TournamentRegistration, DomainError>;

    /// Update a registration.
    async fn update(
        &self,
        id: TournamentRegistrationId,
        update: UpdateTournamentRegistration,
    ) -> Result<TournamentRegistration, DomainError>;

    /// Update registration status.
    async fn update_status(
        &self,
        id: TournamentRegistrationId,
        status: TournamentRegistrationStatus,
    ) -> Result<TournamentRegistration, DomainError>;

    /// Check in a participant.
    async fn check_in(
        &self,
        id: TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentRegistration, DomainError>;

    /// Update seed.
    async fn update_seed(
        &self,
        id: TournamentRegistrationId,
        seed: i32,
    ) -> Result<TournamentRegistration, DomainError>;

    /// Withdraw a registration.
    async fn withdraw(
        &self,
        id: TournamentRegistrationId,
    ) -> Result<TournamentRegistration, DomainError>;

    /// List all registrations for a tournament.
    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
        status_filter: Option<TournamentRegistrationStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TournamentRegistration>, i64), DomainError>;

    /// List checked-in registrations (for bracket generation).
    async fn list_checked_in(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentRegistration>, DomainError>;

    /// List seeded participants (ordered by seed).
    async fn list_seeded(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentRegistration>, DomainError>;

    /// Count registrations by status.
    async fn count_by_status(
        &self,
        tournament_id: TournamentId,
        status: TournamentRegistrationStatus,
    ) -> Result<i64, DomainError>;

    /// Bulk update seeds.
    async fn bulk_update_seeds(
        &self,
        seeds: Vec<(TournamentRegistrationId, i32)>,
    ) -> Result<(), DomainError>;

    /// Clear all seeds for a tournament.
    async fn clear_seeds(&self, tournament_id: TournamentId) -> Result<(), DomainError>;

    /// Delete a registration.
    async fn delete(&self, id: TournamentRegistrationId) -> Result<(), DomainError>;
}

/// Data for creating a tournament registration.
#[derive(Debug, Clone)]
pub struct CreateTournamentRegistration {
    pub tournament_id: TournamentId,
    pub team_season_id: Option<LeagueTeamSeasonId>,
    pub player_id: Option<PlayerId>,
    pub adhoc_team_id: Option<uuid::Uuid>,
    pub participant_name: String,
    pub participant_logo_url: Option<String>,
    pub registered_by: UserId,
    pub seed_rating: Option<i32>,
}

/// Data for updating a tournament registration.
#[derive(Debug, Clone, Default)]
pub struct UpdateTournamentRegistration {
    pub participant_name: Option<String>,
    pub participant_logo_url: Option<String>,
    pub seed: Option<i32>,
    pub seed_rating: Option<i32>,
    pub admin_notes: Option<String>,
}

// =============================================================================
// TOURNAMENT MATCH REPOSITORY
// =============================================================================

/// Repository trait for tournament match operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentMatchRepository: Send + Sync {
    /// Find a match by ID.
    async fn find_by_id(
        &self,
        id: TournamentMatchId,
    ) -> Result<Option<TournamentMatch>, DomainError>;

    /// Find a match by bracket position.
    async fn find_by_position(
        &self,
        bracket_id: TournamentBracketId,
        position: &str,
    ) -> Result<Option<TournamentMatch>, DomainError>;

    /// Create a new match.
    async fn create(&self, match_: CreateTournamentMatch) -> Result<TournamentMatch, DomainError>;

    /// Update a match.
    async fn update(
        &self,
        id: TournamentMatchId,
        update: UpdateTournamentMatch,
    ) -> Result<TournamentMatch, DomainError>;

    /// Update match status.
    async fn update_status(
        &self,
        id: TournamentMatchId,
        status: TournamentMatchStatus,
    ) -> Result<TournamentMatch, DomainError>;

    /// Assign a participant to a match slot.
    async fn assign_participant(
        &self,
        id: TournamentMatchId,
        slot: ParticipantSlot,
        registration_id: TournamentRegistrationId,
        name: String,
        logo_url: Option<String>,
        seed: Option<i32>,
    ) -> Result<TournamentMatch, DomainError>;

    /// Submit match result.
    async fn submit_result(
        &self,
        id: TournamentMatchId,
        participant1_score: i32,
        participant2_score: i32,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Schedule a match.
    async fn schedule(
        &self,
        id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
    ) -> Result<TournamentMatch, DomainError>;

    /// Start a match.
    async fn start(&self, id: TournamentMatchId) -> Result<TournamentMatch, DomainError>;

    /// Record a participant check-in.
    async fn check_in_participant(
        &self,
        id: TournamentMatchId,
        slot: ParticipantSlot,
        checked_in_by: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Complete a match.
    async fn complete(&self, id: TournamentMatchId) -> Result<TournamentMatch, DomainError>;

    /// Forfeit a match.
    async fn forfeit(
        &self,
        id: TournamentMatchId,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<TournamentMatch, DomainError>;

    /// File a dispute.
    async fn file_dispute(
        &self,
        id: TournamentMatchId,
        reason: String,
    ) -> Result<TournamentMatch, DomainError>;

    /// Resolve a dispute.
    async fn resolve_dispute(
        &self,
        id: TournamentMatchId,
        resolved_by: UserId,
        resolution: String,
    ) -> Result<TournamentMatch, DomainError>;

    /// List all matches for a bracket.
    async fn list_by_bracket(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// List all matches for a stage.
    async fn list_by_stage(
        &self,
        stage_id: TournamentStageId,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// List all matches for a tournament.
    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// List matches for a round.
    async fn list_by_round(
        &self,
        bracket_id: TournamentBracketId,
        round: i32,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// List matches by status.
    async fn list_by_status(
        &self,
        tournament_id: TournamentId,
        status: TournamentMatchStatus,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// List matches for a participant.
    async fn list_by_participant(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// List matches for a player across all tournaments.
    ///
    /// Finds matches where the player is a participant via either direct solo
    /// registration or team registration (through `league_team_members`).
    async fn list_by_player(
        &self,
        player_id: PlayerId,
        status: Option<TournamentMatchStatus>,
        tournament_id: Option<TournamentId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// List upcoming scheduled matches.
    async fn list_upcoming(
        &self,
        tournament_id: TournamentId,
        limit: i64,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// Bulk create matches (for bracket generation).
    async fn bulk_create(
        &self,
        matches: Vec<CreateTournamentMatch>,
    ) -> Result<Vec<TournamentMatch>, DomainError>;

    /// Delete a match.
    async fn delete(&self, id: TournamentMatchId) -> Result<(), DomainError>;

    /// Delete all matches for a bracket (for regeneration).
    async fn delete_by_bracket(&self, bracket_id: TournamentBracketId) -> Result<(), DomainError>;

    /// Set progression links on a match (winner_progresses_to / loser_progresses_to).
    async fn set_progression_links(
        &self,
        id: TournamentMatchId,
        winner_progresses_to: Option<TournamentMatchId>,
        loser_progresses_to: Option<TournamentMatchId>,
    ) -> Result<(), DomainError>;
}

/// Participant slot (1 or 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantSlot {
    One,
    Two,
}

/// Data for creating a tournament match.
#[derive(Debug, Clone)]
pub struct CreateTournamentMatch {
    pub bracket_id: TournamentBracketId,
    pub stage_id: TournamentStageId,
    pub tournament_id: TournamentId,
    pub round: i32,
    pub match_number: i32,
    pub bracket_position: String,
    pub participant1_registration_id: Option<TournamentRegistrationId>,
    pub participant2_registration_id: Option<TournamentRegistrationId>,
    pub participant1_name: Option<String>,
    pub participant1_logo_url: Option<String>,
    pub participant1_seed: Option<i32>,
    pub participant2_name: Option<String>,
    pub participant2_logo_url: Option<String>,
    pub participant2_seed: Option<i32>,
    pub participant1_source: Option<MatchParticipantSource>,
    pub participant2_source: Option<MatchParticipantSource>,
    pub match_format: MatchFormat,
    pub maps_required: i32,
    pub winner_progresses_to: Option<TournamentMatchId>,
    pub loser_progresses_to: Option<TournamentMatchId>,
}

/// Data for updating a tournament match.
#[derive(Debug, Clone, Default)]
pub struct UpdateTournamentMatch {
    pub scheduled_at: Option<DateTime<Utc>>,
    pub schedule_deadline: Option<DateTime<Utc>>,
    pub stream_url: Option<String>,
    pub vod_url: Option<String>,
}

// =============================================================================
// TOURNAMENT MATCH GAME REPOSITORY
// =============================================================================

/// Repository trait for tournament match game operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentMatchGameRepository: Send + Sync {
    /// Find a game by ID.
    async fn find_by_id(
        &self,
        id: TournamentMatchGameId,
    ) -> Result<Option<TournamentMatchGame>, DomainError>;

    /// Find a game by match and game number.
    async fn find_by_number(
        &self,
        match_id: TournamentMatchId,
        game_number: i32,
    ) -> Result<Option<TournamentMatchGame>, DomainError>;

    /// Create a new game.
    async fn create(&self, game: CreateTournamentMatchGame) -> Result<TournamentMatchGame, DomainError>;

    /// Update a game.
    async fn update(
        &self,
        id: TournamentMatchGameId,
        update: UpdateTournamentMatchGame,
    ) -> Result<TournamentMatchGame, DomainError>;

    /// Update game status.
    async fn update_status(
        &self,
        id: TournamentMatchGameId,
        status: GameStatus,
    ) -> Result<TournamentMatchGame, DomainError>;

    /// Set map for a game.
    async fn set_map(
        &self,
        id: TournamentMatchGameId,
        map_id: String,
        picked_by: Option<TournamentRegistrationId>,
    ) -> Result<TournamentMatchGame, DomainError>;

    /// Submit game result.
    async fn submit_result(
        &self,
        id: TournamentMatchGameId,
        participant1_score: i32,
        participant2_score: i32,
        winner_id: TournamentRegistrationId,
        duration_seconds: Option<i32>,
        game_data: Option<serde_json::Value>,
    ) -> Result<TournamentMatchGame, DomainError>;

    /// Start a game.
    async fn start(&self, id: TournamentMatchGameId) -> Result<TournamentMatchGame, DomainError>;

    /// List all games for a match.
    async fn list_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<TournamentMatchGame>, DomainError>;

    /// Count completed games for a match.
    async fn count_completed(&self, match_id: TournamentMatchId) -> Result<i64, DomainError>;

    /// Create games for a match (based on format).
    async fn create_for_match(
        &self,
        match_id: TournamentMatchId,
        maps_required: i32,
    ) -> Result<Vec<TournamentMatchGame>, DomainError>;

    /// Delete a game.
    async fn delete(&self, id: TournamentMatchGameId) -> Result<(), DomainError>;

    /// Delete all games for a match.
    async fn delete_by_match(&self, match_id: TournamentMatchId) -> Result<(), DomainError>;
}

/// Data for creating a tournament match game.
#[derive(Debug, Clone)]
pub struct CreateTournamentMatchGame {
    pub match_id: TournamentMatchId,
    pub game_number: i32,
    pub map_id: Option<String>,
}

/// Data for updating a tournament match game.
#[derive(Debug, Clone, Default)]
pub struct UpdateTournamentMatchGame {
    pub map_id: Option<String>,
    pub map_picked_by: Option<TournamentRegistrationId>,
    pub side_selection_by: Option<TournamentRegistrationId>,
    pub game_data: Option<serde_json::Value>,
}

// =============================================================================
// TOURNAMENT STANDINGS REPOSITORY
// =============================================================================

/// Repository trait for tournament standings (round robin/swiss).
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentStandingsRepository: Send + Sync {
    /// Find standings by bracket and registration.
    async fn find(
        &self,
        bracket_id: TournamentBracketId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Option<TournamentStanding>, DomainError>;

    /// Create standings entry.
    async fn create(
        &self,
        standing: CreateTournamentStanding,
    ) -> Result<TournamentStanding, DomainError>;

    /// Update standings after a match.
    async fn update_after_match(
        &self,
        standing: UpdateTournamentStanding,
    ) -> Result<TournamentStanding, DomainError>;

    /// Recalculate positions for a bracket.
    async fn recalculate_positions(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError>;

    /// List standings for a bracket (ordered by position).
    async fn list_by_bracket(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError>;

    /// Bulk create standings (for initialization).
    async fn bulk_create(
        &self,
        standings: Vec<CreateTournamentStanding>,
    ) -> Result<Vec<TournamentStanding>, DomainError>;
}

/// Data for creating tournament standings.
#[derive(Debug, Clone)]
pub struct CreateTournamentStanding {
    pub bracket_id: TournamentBracketId,
    pub registration_id: TournamentRegistrationId,
    pub position: i32,
}

/// Data for updating tournament standings after a match.
#[derive(Debug, Clone)]
pub struct UpdateTournamentStanding {
    pub bracket_id: TournamentBracketId,
    pub registration_id: TournamentRegistrationId,
    pub matches_won_delta: i32,
    pub matches_lost_delta: i32,
    pub matches_drawn_delta: i32,
    pub game_wins_delta: i32,
    pub game_losses_delta: i32,
    pub points_delta: i32,
}

// =============================================================================
// TOURNAMENT MAP POOL REPOSITORY
// =============================================================================

/// Repository trait for tournament map pool operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TournamentMapPoolRepository: Send + Sync {
    /// Find a map pool by ID.
    async fn find_by_id(
        &self,
        id: TournamentMapPoolId,
    ) -> Result<Option<TournamentMapPool>, DomainError>;

    /// Find the map pool for a tournament (default pool).
    async fn find_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Option<TournamentMapPool>, DomainError>;

    /// Find the map pool for a specific stage.
    async fn find_by_stage(
        &self,
        stage_id: TournamentStageId,
    ) -> Result<Option<TournamentMapPool>, DomainError>;

    /// Get effective map pool for a stage (stage-specific or tournament default).
    async fn get_effective(
        &self,
        tournament_id: TournamentId,
        stage_id: Option<TournamentStageId>,
    ) -> Result<Option<TournamentMapPool>, DomainError>;

    /// Create or update map pool.
    async fn upsert(&self, pool: UpsertTournamentMapPool) -> Result<TournamentMapPool, DomainError>;

    /// Delete a map pool.
    async fn delete(&self, id: TournamentMapPoolId) -> Result<(), DomainError>;
}

/// Data for upserting a tournament map pool.
#[derive(Debug, Clone)]
pub struct UpsertTournamentMapPool {
    pub tournament_id: TournamentId,
    pub stage_id: Option<TournamentStageId>,
    pub maps: Vec<String>,
    pub veto_format_id: Option<String>,
}

// =============================================================================
// VETO SESSION REPOSITORY
// =============================================================================

use crate::entities::veto::{VetoAction, VetoActionType, VetoSession, VetoStatus};
use portal_core::{VetoActionId, VetoSessionId};

/// Repository trait for veto session operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait VetoSessionRepository: Send + Sync {
    /// Find a veto session by ID.
    async fn find_by_id(&self, id: VetoSessionId) -> Result<Option<VetoSession>, DomainError>;

    /// Find a veto session by match ID.
    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<VetoSession>, DomainError>;

    /// Create a new veto session.
    async fn create(&self, session: CreateVetoSession) -> Result<VetoSession, DomainError>;

    /// Update a veto session.
    async fn update(
        &self,
        id: VetoSessionId,
        update: UpdateVetoSession,
    ) -> Result<VetoSession, DomainError>;

    /// Update veto session status.
    async fn update_status(
        &self,
        id: VetoSessionId,
        status: VetoStatus,
    ) -> Result<VetoSession, DomainError>;

    /// Find sessions with expired action deadlines.
    async fn find_timed_out(&self) -> Result<Vec<VetoSession>, DomainError>;

    /// Delete a veto session.
    async fn delete(&self, id: VetoSessionId) -> Result<(), DomainError>;
}

/// Data for creating a veto session.
#[derive(Debug, Clone)]
pub struct CreateVetoSession {
    pub match_id: TournamentMatchId,
    pub veto_format_id: String,
    pub map_pool: Vec<String>,
    pub timeout_seconds: u32,
    pub side_selection_mode: crate::entities::veto::SideSelectionMode,
}

/// Data for updating a veto session.
#[derive(Debug, Clone, Default)]
pub struct UpdateVetoSession {
    pub first_action_registration_id: Option<TournamentRegistrationId>,
    pub coin_flip_winner_registration_id: Option<TournamentRegistrationId>,
    pub current_action_number: Option<u32>,
    pub current_team_turn: Option<Option<TournamentRegistrationId>>,
    pub remaining_maps: Option<Vec<String>>,
    pub selected_maps: Option<Vec<String>>,
    pub status: Option<VetoStatus>,
    pub action_deadline: Option<Option<DateTime<Utc>>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

// =============================================================================
// VETO ACTION REPOSITORY
// =============================================================================

/// Repository trait for veto action operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait VetoActionRepository: Send + Sync {
    /// Find a veto action by ID.
    async fn find_by_id(&self, id: VetoActionId) -> Result<Option<VetoAction>, DomainError>;

    /// Find a veto action by session and action number.
    async fn find_by_session_and_number(
        &self,
        session_id: VetoSessionId,
        action_number: u32,
    ) -> Result<Option<VetoAction>, DomainError>;

    /// List all actions for a session (ordered by action number).
    async fn list_by_session(&self, session_id: VetoSessionId)
        -> Result<Vec<VetoAction>, DomainError>;

    /// Create a new veto action.
    async fn create(&self, action: CreateVetoAction) -> Result<VetoAction, DomainError>;

    /// Update side selection for an action.
    async fn update_side_selection(
        &self,
        id: VetoActionId,
        side: String,
        selected_by: TournamentRegistrationId,
    ) -> Result<VetoAction, DomainError>;
}

/// Data for creating a veto action.
#[derive(Debug, Clone)]
pub struct CreateVetoAction {
    pub session_id: VetoSessionId,
    pub action_number: u32,
    pub action_type: VetoActionType,
    pub map_id: String,
    pub performed_by_registration_id: Option<TournamentRegistrationId>,
    pub performed_by_user_id: Option<UserId>,
    pub was_auto_action: bool,
    pub auto_action_reason: Option<String>,
}

// =============================================================================
// RESULT CLAIM REPOSITORY
// =============================================================================

use crate::entities::result_claim::{ClaimStatus, GameResult, ResultClaim};
use portal_core::{EvidenceId, ResultClaimId};

/// Repository trait for result claim operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ResultClaimRepository: Send + Sync {
    /// Find a result claim by ID.
    async fn find_by_id(&self, id: ResultClaimId) -> Result<Option<ResultClaim>, DomainError>;

    /// Find the pending claim for a match.
    async fn find_pending_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultClaim>, DomainError>;

    /// List all claims for a match (ordered by created_at desc).
    async fn list_by_match(&self, match_id: TournamentMatchId)
        -> Result<Vec<ResultClaim>, DomainError>;

    /// Create a new result claim.
    async fn create(&self, claim: CreateResultClaim) -> Result<ResultClaim, DomainError>;

    /// Update a result claim.
    async fn update(
        &self,
        id: ResultClaimId,
        update: UpdateResultClaim,
    ) -> Result<ResultClaim, DomainError>;

    /// Update result claim status.
    async fn update_status(
        &self,
        id: ResultClaimId,
        status: ClaimStatus,
    ) -> Result<ResultClaim, DomainError>;

    /// Confirm a result claim.
    async fn confirm(
        &self,
        id: ResultClaimId,
        confirmed_by_registration_id: TournamentRegistrationId,
        confirmed_by_user_id: UserId,
        was_auto: bool,
    ) -> Result<ResultClaim, DomainError>;

    /// Supersede all pending claims for a match (when a new claim is submitted).
    async fn supersede_pending_claims(
        &self,
        match_id: TournamentMatchId,
        except_claim_id: ResultClaimId,
    ) -> Result<(), DomainError>;

    /// Find claims ready for auto-confirmation.
    async fn find_ready_for_auto_confirm(&self) -> Result<Vec<ResultClaim>, DomainError>;
}

/// Data for creating a result claim.
#[derive(Debug, Clone)]
pub struct CreateResultClaim {
    pub match_id: TournamentMatchId,
    pub submitted_by_registration_id: TournamentRegistrationId,
    pub submitted_by_user_id: UserId,
    pub claimed_winner_registration_id: TournamentRegistrationId,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub game_results: Vec<GameResult>,
    pub auto_confirm_at: DateTime<Utc>,
    pub evidence_ids: Vec<EvidenceId>,
    pub demo_link_ids: Vec<DemoMatchLinkId>,
    pub notes: Option<String>,
}

/// Data for updating a result claim.
#[derive(Debug, Clone, Default)]
pub struct UpdateResultClaim {
    pub status: Option<ClaimStatus>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub confirmed_by_registration_id: Option<TournamentRegistrationId>,
    pub confirmed_by_user_id: Option<UserId>,
    pub was_auto_confirmed: Option<bool>,
}
