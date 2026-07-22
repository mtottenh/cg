//! Database row type for player_mm_stats.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct PlayerMmStatsRow {
    pub id: Uuid,
    pub player_id: Uuid,
    pub game_id: Uuid,
    pub matches_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub headshots: i32,
    pub mvps: i32,
    pub entry_3k: i32,
    pub entry_4k: i32,
    pub entry_5k: i32,
    pub total_score: i32,
    pub total_duration_secs: i32,
    pub first_match_at: Option<DateTime<Utc>>,
    pub last_match_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
