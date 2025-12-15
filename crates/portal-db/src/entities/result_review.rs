//! Result review database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `result_reviews` table.
#[derive(Debug, Clone, FromRow)]
pub struct ResultReviewRow {
    pub id: Uuid,
    pub result_claim_id: Uuid,
    pub match_id: Uuid,
    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,
    pub demo_link_id: Option<Uuid>,
    pub validation_result: Option<sqlx::types::Json<serde_json::Value>>,
    pub unrecognized_players: sqlx::types::Json<Vec<serde_json::Value>>,
    pub status: String,
    pub captain1_registration_id: Uuid,
    pub captain1_acknowledged: bool,
    pub captain1_acknowledged_at: Option<DateTime<Utc>>,
    pub captain1_acknowledged_by_user_id: Option<Uuid>,
    pub captain2_registration_id: Uuid,
    pub captain2_acknowledged: bool,
    pub captain2_acknowledged_at: Option<DateTime<Utc>>,
    pub captain2_acknowledged_by_user_id: Option<Uuid>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for creating a new result review.
#[derive(Debug, Clone)]
pub struct NewResultReview {
    pub id: Uuid,
    pub result_claim_id: Uuid,
    pub match_id: Uuid,
    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,
    pub demo_link_id: Option<Uuid>,
    pub validation_result: Option<serde_json::Value>,
    pub unrecognized_players: Vec<serde_json::Value>,
    pub status: String,
    pub captain1_registration_id: Uuid,
    pub captain2_registration_id: Uuid,
}
