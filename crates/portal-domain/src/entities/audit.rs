//! Audit trail entities.

use chrono::{DateTime, Utc};
use portal_core::PlayerId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an entity change.
pub type EntityChangeId = Uuid;

/// Types of changes tracked in the audit trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    /// Entity was created.
    Create,
    /// Entity was updated.
    Update,
    /// Entity was deleted.
    Delete,
    /// A previous change was reverted.
    Revert,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => write!(f, "create"),
            Self::Update => write!(f, "update"),
            Self::Delete => write!(f, "delete"),
            Self::Revert => write!(f, "revert"),
        }
    }
}

impl std::str::FromStr for ChangeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "create" => Ok(Self::Create),
            "update" => Ok(Self::Update),
            "delete" => Ok(Self::Delete),
            "revert" => Ok(Self::Revert),
            _ => Err(format!("Unknown change type: {s}")),
        }
    }
}

/// A single change recorded in the audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityChange {
    /// Unique identifier for this change record.
    pub id: EntityChangeId,

    /// Type of entity that was changed (e.g., "team", "player").
    pub entity_type: String,

    /// ID of the entity that was changed.
    pub entity_id: Uuid,

    /// Type of change made.
    pub change_type: ChangeType,

    /// Name of the field that was changed (None for create/delete).
    pub field_name: Option<String>,

    /// Previous value as JSON (None for create).
    pub old_value: Option<serde_json::Value>,

    /// New value as JSON (None for delete).
    pub new_value: Option<serde_json::Value>,

    /// Player who made the change.
    pub changed_by: PlayerId,

    /// When this change was reverted (if applicable).
    pub reverted_at: Option<DateTime<Utc>>,

    /// Player who reverted the change.
    pub reverted_by: Option<PlayerId>,

    /// Reason for reverting.
    pub revert_reason: Option<String>,

    /// Request ID for correlation.
    pub request_id: Option<String>,

    /// IP address of the client.
    pub ip_address: Option<String>,

    /// User agent of the client.
    pub user_agent: Option<String>,

    /// When this change was made.
    pub created_at: DateTime<Utc>,
}

/// Summary of changes for a field over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChangeSummary {
    /// Name of the field.
    pub field_name: String,

    /// Number of times this field has been changed.
    pub change_count: i64,

    /// Most recent value.
    pub current_value: Option<serde_json::Value>,

    /// Most recent change.
    pub last_changed_at: DateTime<Utc>,

    /// Who made the most recent change.
    pub last_changed_by: PlayerId,
}

/// Entity history with all changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityHistory {
    /// Type of entity.
    pub entity_type: String,

    /// ID of the entity.
    pub entity_id: Uuid,

    /// All changes ordered by time (newest first).
    pub changes: Vec<EntityChange>,

    /// Total number of changes.
    pub total_changes: i64,
}
