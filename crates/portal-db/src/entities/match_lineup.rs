//! Match lineup database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `match_lineups` table.
#[derive(Debug, Clone, FromRow)]
pub struct MatchLineupRow {
    pub id: Uuid,
    pub match_id: Uuid,
    pub registration_id: Uuid,
    pub status: String,
    pub declared_by: Option<Uuid>,
    pub declared_at: Option<DateTime<Utc>>,
    pub locked_at: Option<DateTime<Utc>>,
    pub short_handed: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Database row for the `match_lineup_players` table.
#[derive(Debug, Clone, FromRow)]
pub struct MatchLineupPlayerRow {
    pub id: Uuid,
    pub lineup_id: Uuid,
    pub player_id: Uuid,
    pub source: String,
    pub game_number: Option<i32>,
    pub is_substitute: bool,
    pub was_rostered: bool,
    pub participation_status: String,
    pub created_at: DateTime<Utc>,
}
