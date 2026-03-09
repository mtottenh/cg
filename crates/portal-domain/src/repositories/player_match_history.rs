//! Player match history repository trait.

use crate::entities::player_match_history::PlayerMatchHistory;
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
    /// Insert a match history entry. Returns the existing row on conflict.
    async fn create(
        &self,
        input: CreatePlayerMatchHistory,
    ) -> Result<PlayerMatchHistory, DomainError>;

    async fn list_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerMatchHistory>, DomainError>;
}
