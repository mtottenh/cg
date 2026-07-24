//! Repository trait for match lineup persistence.

use async_trait::async_trait;
use portal_core::types::LineupSource;
use portal_core::{DomainError, MatchLineupId, TournamentMatchId, TournamentRegistrationId};

use crate::entities::match_lineup::{
    DeclareLineupCommand, LineupPlayerInput, MatchLineup, MatchLineupWithPlayers,
};

/// A demo-derived lineup to materialize for one registration and map (§0b, Phase C).
#[derive(Debug, Clone)]
pub struct MaterializeDemoLineup {
    pub match_id: TournamentMatchId,
    pub registration_id: TournamentRegistrationId,
    pub game_number: Option<i32>,
    /// Resolved players who appear in the demo for this registration's side.
    pub players: Vec<LineupPlayerInput>,
}

/// Repository trait for match lineup persistence.
#[async_trait]
pub trait MatchLineupRepository: Send + Sync {
    /// Declare (create or replace) the provisional `declared` lineup for a
    /// registration in a match. Replaces any existing `declared` player rows.
    async fn declare(&self, cmd: DeclareLineupCommand) -> Result<MatchLineup, DomainError>;

    /// Find a lineup by (match, registration).
    async fn find_by_match_registration(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Option<MatchLineup>, DomainError>;

    /// List all lineups for a match, each with its player rows.
    async fn list_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchLineupWithPlayers>, DomainError>;

    /// Get one lineup with its player rows.
    async fn get_with_players(
        &self,
        id: MatchLineupId,
    ) -> Result<Option<MatchLineupWithPlayers>, DomainError>;

    /// Lock every lineup for a match (status = `locked`, stamp `locked_at`).
    /// Idempotent: already-locked lineups keep their original `locked_at`.
    async fn lock_for_match(&self, match_id: TournamentMatchId) -> Result<u64, DomainError>;

    /// Materialize the authoritative demo-derived lineup for one registration
    /// and map (Phase C). Upserts the lineup row and replaces `demo` player rows
    /// for the given `game_number`. Returns the lineup.
    async fn materialize_demo(
        &self,
        cmd: MaterializeDemoLineup,
    ) -> Result<MatchLineup, DomainError>;

    /// List the players in the authoritative lineup for a match, filtered to a
    /// source (typically `Demo`). Used by eligibility enforcement (Phase D).
    async fn list_players_by_source(
        &self,
        match_id: TournamentMatchId,
        source: LineupSource,
    ) -> Result<Vec<crate::entities::match_lineup::MatchLineupPlayer>, DomainError>;

    /// Whether the match's season opted into lineups
    /// (`league_seasons.lineup_required`). `false` for a match with no season
    /// (standalone tournament) — the pre-cutover default (§9).
    async fn is_lineup_required_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<bool, DomainError>;
}
