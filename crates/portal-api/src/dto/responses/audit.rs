//! Audit trail response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::audit::{ChangeType, EntityChange};
use serde::Serialize;
use utoipa::ToSchema;

/// One recorded change from the entity audit trail (P-149).
///
/// Until this existed the `entity_changes` table — the audit spine every
/// override writes to (roster-lock overrides, score corrections, …) — was
/// readable only through `portal-cli`, so ops tooling and admins had no HTTP
/// answer to "who changed this, when, from what, to what, and why".
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EntityChangeResponse {
    /// Change record ID.
    pub id: String,
    /// Entity type the change targeted (e.g. "league_season", "tournament_match").
    pub entity_type: String,
    /// ID of the changed entity.
    pub entity_id: String,
    /// Kind of change.
    pub change_type: ChangeType,
    /// Changed field, when the change is field-scoped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    /// Value before the change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<serde_json::Value>,
    /// Value after the change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<serde_json::Value>,
    /// Player who made the change.
    pub changed_by: String,
    /// Their display name, resolved server-side — an audit row that names its
    /// actor only by UUID is not usable audit (the P-115/P-123 lesson).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_by_display_name: Option<String>,
    /// When a later change reverted this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverted_at: Option<DateTime<Utc>>,
    /// Why it was reverted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revert_reason: Option<String>,
    /// When the change was made.
    pub created_at: DateTime<Utc>,
}

impl EntityChangeResponse {
    /// Build from the domain change plus a resolved actor name.
    #[must_use]
    pub fn from_change(change: EntityChange, changed_by_display_name: Option<String>) -> Self {
        Self {
            id: change.id.to_string(),
            entity_type: change.entity_type,
            entity_id: change.entity_id.to_string(),
            change_type: change.change_type,
            field_name: change.field_name,
            old_value: change.old_value,
            new_value: change.new_value,
            changed_by: change.changed_by.to_string(),
            changed_by_display_name,
            reverted_at: change.reverted_at,
            revert_reason: change.revert_reason,
            created_at: change.created_at,
        }
    }
}
