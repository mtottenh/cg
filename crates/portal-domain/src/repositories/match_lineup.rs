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

    /// Distinct players to credit for `registration_id` in `match_id`, across
    /// all maps — taken from the MOST AUTHORITATIVE lineup source present:
    /// `demo` > `admin` > `evidence` > `declared`.
    ///
    /// P-58 originally queried `source = 'demo'` alone. That fixed the headline
    /// defect (team matches credited nobody) only for demo-covered matches: a
    /// team registration's `player_id` is `None`, so the caller's fallback
    /// credited nobody, and a team that played without a parsed demo still had
    /// its participation silently dropped — reported by a `warn!` line and
    /// nothing else.
    ///
    /// The inconsistency is what settles it. For an INDIVIDUAL registration the
    /// caller credits the registrant with no proof they played at all. Teams
    /// were being held to a stricter evidentiary standard than individuals for
    /// the same statistic, and the penalty for failing it was silence.
    ///
    /// Exactly one source is used — never a union — so a stale `declared`
    /// promise cannot dilute an authoritative `demo` record. Empty only when no
    /// lineup of any source exists (the caller then falls back to the
    /// registration's own player).
    ///
    /// NOTE this is deliberately more permissive than eligibility enforcement,
    /// which must keep reading `demo` alone via `list_players_by_source`:
    /// judging a roster-rule violation against a pre-match promise would punish
    /// teams for intentions. Crediting participation and policing eligibility
    /// want different evidence bars, and only the former is this method.
    async fn distinct_participants(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<portal_core::PlayerId>, DomainError>;

    /// Whether the match's season opted into lineups
    /// (`league_seasons.lineup_required`). `false` for a match with no season
    /// (standalone tournament) — the pre-cutover default (§9).
    async fn is_lineup_required_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<bool, DomainError>;
}
