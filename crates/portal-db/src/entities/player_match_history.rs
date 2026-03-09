//! Database row type for player_match_history.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct PlayerMatchHistoryRow {
    pub id: Uuid,
    pub player_id: Uuid,
    pub game_id: Uuid,
    pub discovered_match_id: Uuid,
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
