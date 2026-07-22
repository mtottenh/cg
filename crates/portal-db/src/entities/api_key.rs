//! API key database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `api_keys` table.
#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub service_name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
