//! Player match history repository trait.

use crate::entities::player_match_history::PlayerMatchHistory;
use crate::repositories::player_mm_stats::AccumulateMatchStats;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{DiscoveredMatchId, DomainError, GameId, PlayerId};

/// Input for creating a match history entry.
pub struct CreatePlayerMatchHistory {
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub discovered_match_id: DiscoveredMatchId,
    pub map: String,
    pub match_time: Option<DateTime<Utc>>,
    pub team_scores: Vec<i32>,
    pub match_duration_secs: i32,
    pub match_result: String,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub score: i32,
    pub headshots: i32,
    pub mvps: i32,
    pub entry_3k: i32,
    pub entry_4k: i32,
    pub entry_5k: i32,
}

#[async_trait]
pub trait PlayerMatchHistoryRepository: Send + Sync + 'static {
    /// Insert a match history entry, deduped on `(player_id,
    /// discovered_match_id)`.
    ///
    /// Returns the row plus a flag that is `true` only when a NEW row was
    /// inserted (`false` when the entry already existed). Callers use this as a
    /// match-scoped idempotency ledger: accumulative side effects (e.g. the
    /// aggregate MM-stats bump) must run only when the flag is `true`, so a
    /// re-delivered enrichment does not double-count.
    async fn create(
        &self,
        input: CreatePlayerMatchHistory,
    ) -> Result<(PlayerMatchHistory, bool), DomainError>;

    /// Atomically claim the match-history row AND apply the aggregate MM-stats
    /// accumulate in ONE transaction.
    ///
    /// The history insert (`ON CONFLICT (player_id, discovered_match_id) DO
    /// NOTHING RETURNING`) is the idempotency claim; the `stats` accumulate is
    /// its effect. Running both in the same transaction makes the pair
    /// all-or-nothing:
    ///
    /// * Re-delivery — the history row already exists, no row is claimed, the
    ///   accumulate is skipped, and the caller sees `false` (idempotent).
    /// * Partial failure — if the accumulate errors (e.g. an INTEGER overflow),
    ///   the history claim ROLLS BACK with it, so a later retry re-claims the
    ///   row (`true`) and applies both effects exactly once. This closes the
    ///   split-autocommit gap where a committed-but-orphaned history row would
    ///   suppress the accumulate on every subsequent retry.
    ///
    /// Returns `true` when the row was newly claimed (accumulate ran), `false`
    /// on a re-delivery (accumulate skipped).
    async fn claim_and_accumulate(
        &self,
        input: CreatePlayerMatchHistory,
        stats: &AccumulateMatchStats,
    ) -> Result<bool, DomainError>;

    async fn list_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerMatchHistory>, DomainError>;
}
