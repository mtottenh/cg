//! Database row type for player_rating_history.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Row from the `player_rating_history` table.
#[derive(Debug, Clone, FromRow)]
pub struct PlayerRatingHistoryRow {
    pub id: Uuid,
    pub player_id: Uuid,
    pub game_id: Uuid,
    pub rating: i32,
    pub source: String,
    pub rank_type_id: i32,
    pub recorded_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Aggregate stats computed from rating history + current profile.
#[derive(Debug, Clone, FromRow)]
pub struct RatingStatsRow {
    pub current_rating: i32,
    pub peak_rating: i32,
    pub average_rating: f64,
    pub median_rating: f64,
    pub data_points: i64,
}
