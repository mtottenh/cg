//! Demo upload attempt database entity.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `demo_upload_attempts` table.
#[derive(Debug, Clone, FromRow)]
pub struct DemoUploadAttemptRow {
    pub id: Uuid,
    pub received_at: DateTime<Utc>,
    pub server_id: Option<Uuid>,
    pub outcome: String,
    pub http_status: i16,
    pub file_name: Option<String>,
    pub matchzy_id: Option<i64>,
    pub map_number: Option<i32>,
    pub bytes: i64,
    pub duration_ms: i32,
    pub demo_id: Option<Uuid>,
    pub linked_match_id: Option<Uuid>,
    pub link_note: Option<String>,
    pub error: Option<String>,
}
