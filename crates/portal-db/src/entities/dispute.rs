//! Dispute database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `disputes` table.
#[derive(Debug, Clone, FromRow)]
pub struct DisputeRow {
    pub id: Uuid,
    pub match_id: Uuid,
    pub result_claim_id: Option<Uuid>,
    pub disputed_by_registration_id: Uuid,
    pub disputed_by_user_id: Uuid,
    pub reason: String,
    pub description: String,
    pub evidence_ids: Vec<Uuid>,
    pub original_winner_registration_id: Option<Uuid>,
    pub original_participant1_score: Option<i32>,
    pub original_participant2_score: Option<i32>,
    pub status: String,
    pub priority: String,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by_user_id: Option<Uuid>,
    pub resolution_type: Option<String>,
    pub resolution_notes: Option<String>,
    pub new_winner_registration_id: Option<Uuid>,
    pub new_participant1_score: Option<i32>,
    pub new_participant2_score: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for creating a new dispute.
#[derive(Debug, Clone)]
pub struct NewDispute {
    pub match_id: Uuid,
    pub result_claim_id: Option<Uuid>,
    pub disputed_by_registration_id: Uuid,
    pub disputed_by_user_id: Uuid,
    pub reason: String,
    pub description: String,
    pub evidence_ids: Vec<Uuid>,
    pub original_winner_registration_id: Option<Uuid>,
    pub original_participant1_score: Option<i32>,
    pub original_participant2_score: Option<i32>,
    pub priority: String,
}

/// Database row for the `dispute_messages` table.
#[derive(Debug, Clone, FromRow)]
pub struct DisputeMessageRow {
    pub id: Uuid,
    pub dispute_id: Uuid,
    pub author_user_id: Uuid,
    pub author_type: String,
    pub message: String,
    pub evidence_ids: Vec<Uuid>,
    pub is_internal: bool,
    pub created_at: DateTime<Utc>,
}

/// Data for creating a new dispute message.
#[derive(Debug, Clone)]
pub struct NewDisputeMessage {
    pub dispute_id: Uuid,
    pub author_user_id: Uuid,
    pub author_type: String,
    pub message: String,
    pub evidence_ids: Vec<Uuid>,
    pub is_internal: bool,
}
