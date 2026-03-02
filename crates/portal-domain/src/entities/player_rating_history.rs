//! Player rating history entity.

use chrono::{DateTime, Utc};
use portal_core::{GameId, PlayerId, PlayerRatingHistoryId};

/// A single rating observation for a player in a game.
///
/// These records are created by external services (e.g. a Steam bot)
/// that periodically observe a player's in-game rating and submit it
/// to the portal API.
pub struct PlayerRatingHistory {
    pub id: PlayerRatingHistoryId,
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub rating: i32,
    /// Where this rating came from (e.g. "mm_demo", "manual", "bot_sync").
    pub source: String,
    /// When the rating was observed in-game.
    pub recorded_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
