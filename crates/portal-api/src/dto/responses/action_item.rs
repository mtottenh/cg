//! Action item response DTOs.

use portal_db::ActionItem;
use serde::Serialize;
use utoipa::ToSchema;

/// A pending action item requiring captain attention.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActionItemResponse {
    /// Type of action required.
    #[schema(example = "confirm_result")]
    pub action_type: String,

    /// Match ID this action relates to.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub match_id: String,

    /// Tournament ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub tournament_id: String,

    /// Tournament slug for URL construction.
    #[schema(example = "summer-cup-2026")]
    pub tournament_slug: String,

    /// Tournament name for display.
    #[schema(example = "Summer Cup 2026")]
    pub tournament_name: String,

    /// Human-readable match label.
    #[schema(example = "Team Alpha vs Team Bravo")]
    pub match_label: String,

    /// Optional deadline for this action (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,

    /// When the action became available (ISO 8601).
    pub created_at: String,
}

impl From<ActionItem> for ActionItemResponse {
    fn from(item: ActionItem) -> Self {
        Self {
            action_type: item.action_type,
            match_id: item.match_id.to_string(),
            tournament_id: item.tournament_id.to_string(),
            tournament_slug: item.tournament_slug,
            tournament_name: item.tournament_name,
            match_label: item.match_label,
            deadline: item.deadline.map(|t| t.to_rfc3339()),
            created_at: item.created_at.to_rfc3339(),
        }
    }
}
