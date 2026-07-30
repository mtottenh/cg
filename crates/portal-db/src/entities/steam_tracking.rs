//! Steam tracking database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `steam_tracking` table.
#[derive(Debug, Clone, FromRow)]
pub struct SteamTrackingRow {
    pub id: Uuid,
    pub player_id: Uuid,
    pub game_id: Uuid,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    pub last_known_code: Option<String>,
    pub is_active: bool,
    pub poll_errors: i32,
    pub last_poll_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub next_poll_at: DateTime<Utc>,
    pub poll_state: String,
    pub paused_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
