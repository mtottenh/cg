//! Audit trail database entity types.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for entity_changes table.
#[derive(Debug, Clone, FromRow)]
pub struct EntityChangeRow {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub change_type: String,
    pub field_name: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub changed_by: Uuid,
    pub reverted_at: Option<DateTime<Utc>>,
    pub reverted_by: Option<Uuid>,
    pub revert_reason: Option<String>,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Data for inserting a new entity change record.
#[derive(Debug, Clone)]
pub struct NewEntityChange {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub change_type: String,
    pub field_name: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub changed_by: Uuid,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}
