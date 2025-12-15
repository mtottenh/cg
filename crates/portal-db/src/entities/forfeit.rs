//! Forfeit record database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `forfeit_records` table.
#[derive(Debug, Clone, FromRow)]
pub struct ForfeitRecordRow {
    pub id: Uuid,
    pub match_id: Uuid,
    pub forfeiting_registration_id: Uuid,
    pub forfeit_type: String,
    pub reason: Option<String>,
    pub triggered_by_user_id: Option<Uuid>,
    pub triggered_by_system: bool,
    pub forfeited_at: DateTime<Utc>,
}

/// Data for creating a new forfeit record.
#[derive(Debug, Clone)]
pub struct NewForfeitRecord {
    pub match_id: Uuid,
    pub forfeiting_registration_id: Uuid,
    pub forfeit_type: String,
    pub reason: Option<String>,
    pub triggered_by_user_id: Option<Uuid>,
    pub triggered_by_system: bool,
}
