//! Player match history entity.

use chrono::{DateTime, Utc};
use portal_core::{DiscoveredMatchId, GameId, PlayerId, PlayerMatchHistoryId};

/// A single public match result for a player.
pub struct PlayerMatchHistory {
    pub id: PlayerMatchHistoryId,
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
    pub created_at: DateTime<Utc>,
}
