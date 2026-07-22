//! Steam tracking domain entity.

use chrono::{DateTime, Utc};
use portal_core::{GameId, PlayerId, SteamTrackingId};

/// A player's opt-in to Steam match tracking for a specific game.
#[derive(Debug, Clone)]
pub struct SteamTracking {
    pub id: SteamTrackingId,
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    pub last_known_code: Option<String>,
    pub is_active: bool,
    pub poll_errors: i32,
    pub last_poll_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Command to register for steam tracking.
#[derive(Debug, Clone)]
pub struct CreateSteamTrackingCommand {
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    /// Most recent share code — used as the starting cursor for the poller.
    pub initial_share_code: Option<String>,
}

/// Command to update a tracking entry's poll result.
#[derive(Debug, Clone)]
pub struct UpdatePollResultCommand {
    pub last_known_code: Option<String>,
    pub error: Option<String>,
}
