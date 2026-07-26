//! Repository traits for PUG and ad-hoc team persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::types::{PugMapSelectionMode, PugStatus};
use portal_core::{
    AdhocTeamId, DomainError, GameId, MatchFormat, PlayerId, PugId, SideSelectionMode,
    TournamentId, TournamentMatchId, UserId,
};

use crate::entities::pug::{
    AdhocTeam, AdhocTeamMember, Pug, PugPlayer, PugPlayerAggregates, PugWheelEntry, PugWheelSpin,
};

/// Command to create a new PUG lobby.
#[derive(Debug, Clone)]
pub struct CreatePug {
    pub game_id: GameId,
    pub created_by_user_id: UserId,
    pub join_code: String,
    pub match_format: MatchFormat,
    pub map_selection_mode: PugMapSelectionMode,
    pub side_selection_mode: SideSelectionMode,
    pub team_size: i32,
    pub region: Option<String>,
    pub map_pool: Option<Vec<String>>,
    pub listed: bool,
    pub expires_at: DateTime<Utc>,
}

/// Command to record a wheel spin.
#[derive(Debug, Clone)]
pub struct CreateWheelSpin {
    pub pug_id: PugId,
    pub game_number: i32,
    pub entries: serde_json::Value,
    pub winner_map_id: String,
    pub spin_seed: i64,
    pub spun_by_player_id: Option<PlayerId>,
}

/// Repository trait for PUG persistence.
#[async_trait]
pub trait PugRepository: Send + Sync {
    async fn create(&self, cmd: CreatePug) -> Result<Pug, DomainError>;

    async fn find_by_id(&self, id: PugId) -> Result<Option<Pug>, DomainError>;

    async fn find_by_join_code(&self, join_code: &str) -> Result<Option<Pug>, DomainError>;

    async fn find_by_match(&self, match_id: TournamentMatchId) -> Result<Option<Pug>, DomainError>;

    /// PUGs the player participates in (any status), newest first.
    async fn list_by_participant(
        &self,
        player_id: PlayerId,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError>;

    /// Non-terminal PUGs created by this user (for the active-pug cap).
    async fn count_active_created_by(&self, user_id: UserId) -> Result<i64, DomainError>;

    /// Publicly listed gathering lobbies, newest first (open-PUGs browser).
    async fn list_open_listed(
        &self,
        game_id: Option<GameId>,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError>;

    /// Recently completed PUGs, newest first (public results feed).
    async fn list_recent_completed(&self, limit: i64) -> Result<Vec<Pug>, DomainError>;

    /// Compare-and-set status transition. Returns the updated pug, or None if
    /// the pug was not in `expected` (lost race — caller decides what to do).
    async fn transition_status(
        &self,
        id: PugId,
        expected: PugStatus,
        next: PugStatus,
    ) -> Result<Option<Pug>, DomainError>;

    /// Unconditional status write (sweeper / server-event driven updates).
    async fn set_status(&self, id: PugId, status: PugStatus) -> Result<(), DomainError>;

    /// Rotate the join code.
    async fn set_join_code(&self, id: PugId, join_code: &str) -> Result<(), DomainError>;

    /// Record materialization: container tournament + match, status -> map_selection.
    async fn set_materialized(
        &self,
        id: PugId,
        tournament_id: TournamentId,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError>;

    /// Denormalize the series result and mark completed.
    async fn set_result(
        &self,
        id: PugId,
        winner_team: i16,
        team1_score: i32,
        team2_score: i32,
    ) -> Result<(), DomainError>;

    /// Gathering lobbies past their TTL (sweeper).
    async fn list_expired_gathering(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError>;

    /// Materialized, non-terminal pugs not updated since `cutoff` (sweeper:
    /// lobbies that locked but never went live).
    async fn list_stalled_materialized(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError>;

    // -- players ------------------------------------------------------------

    async fn add_player(&self, pug_id: PugId, player_id: PlayerId) -> Result<(), DomainError>;

    async fn remove_player(&self, pug_id: PugId, player_id: PlayerId) -> Result<(), DomainError>;

    async fn list_players(&self, pug_id: PugId) -> Result<Vec<PugPlayer>, DomainError>;

    async fn is_participant(&self, pug_id: PugId, player_id: PlayerId)
    -> Result<bool, DomainError>;

    async fn count_players(&self, pug_id: PugId) -> Result<i64, DomainError>;

    /// Assign a player to team 1, 2 or the bench (None).
    async fn set_player_team(
        &self,
        pug_id: PugId,
        player_id: PlayerId,
        team: Option<i16>,
    ) -> Result<(), DomainError>;

    async fn set_player_captain(
        &self,
        pug_id: PugId,
        player_id: PlayerId,
        is_captain: bool,
    ) -> Result<(), DomainError>;

    /// Bulk team assignment (shuffle) — atomic across all rows.
    async fn assign_teams(
        &self,
        pug_id: PugId,
        assignments: &[(PlayerId, Option<i16>)],
    ) -> Result<(), DomainError>;

    /// Mirror the rosters: everyone on team 1 moves to 2 and vice versa.
    async fn swap_teams(&self, pug_id: PugId) -> Result<(), DomainError>;

    // -- wheel ---------------------------------------------------------------

    /// Upsert the player's nomination.
    async fn upsert_wheel_entry(
        &self,
        pug_id: PugId,
        player_id: PlayerId,
        map_id: &str,
    ) -> Result<(), DomainError>;

    async fn list_wheel_entries(&self, pug_id: PugId) -> Result<Vec<PugWheelEntry>, DomainError>;

    async fn record_spin(&self, cmd: CreateWheelSpin) -> Result<PugWheelSpin, DomainError>;

    async fn list_spins(&self, pug_id: PugId) -> Result<Vec<PugWheelSpin>, DomainError>;

    // -- stats (separate PUG feed) --------------------------------------------

    /// Career PUG aggregates: W/L from completed pugs; combat stats from
    /// demo_players rows on demos categorized 'pug'.
    async fn player_aggregates(
        &self,
        player_id: PlayerId,
    ) -> Result<PugPlayerAggregates, DomainError>;
}

/// Repository trait for ad-hoc team persistence.
#[async_trait]
pub trait AdhocTeamRepository: Send + Sync {
    /// Create a team with its members in one shot.
    async fn create(
        &self,
        tournament_id: TournamentId,
        name: &str,
        members: &[(PlayerId, bool)],
    ) -> Result<AdhocTeam, DomainError>;

    async fn find_by_id(&self, id: AdhocTeamId) -> Result<Option<AdhocTeam>, DomainError>;

    async fn list_members(&self, id: AdhocTeamId) -> Result<Vec<AdhocTeamMember>, DomainError>;

    async fn is_member(&self, id: AdhocTeamId, player_id: PlayerId) -> Result<bool, DomainError>;

    async fn is_captain(&self, id: AdhocTeamId, player_id: PlayerId) -> Result<bool, DomainError>;
}
