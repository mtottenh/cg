//! Player MM stats repository trait.

use crate::entities::player_mm_stats::PlayerMmStats;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{DomainError, GameId, PlayerId};

/// Stats to accumulate from a single match.
pub struct AccumulateMatchStats {
    pub is_win: bool,
    pub is_loss: bool,
    pub is_draw: bool,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub headshots: i32,
    pub mvps: i32,
    pub score: i32,
    pub entry_3k: i32,
    pub entry_4k: i32,
    pub entry_5k: i32,
    pub duration_secs: i32,
    pub match_time: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait PlayerMmStatsRepository: Send + Sync + 'static {
    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<PlayerMmStats>, DomainError>;

    /// Upsert: create the row if it doesn't exist, then atomically add
    /// the match stats to the running totals.
    async fn accumulate_match_stats(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        stats: &AccumulateMatchStats,
    ) -> Result<PlayerMmStats, DomainError>;
}
