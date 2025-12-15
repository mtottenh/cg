//! Veto delegate response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::veto_delegate::VetoDelegate;
use serde::Serialize;
use utoipa::ToSchema;

/// Response for a veto delegate.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VetoDelegateResponse {
    /// Unique identifier for the delegation.
    pub id: String,
    /// Team season ID the delegation is for.
    pub team_season_id: String,
    /// Player ID who is delegated veto authority.
    pub player_id: String,
    /// User ID who created the delegation.
    pub delegated_by_user_id: String,
    /// Role of the user who created the delegation.
    pub delegated_by_role: String,
    /// Tournament ID the delegation is scoped to (if any).
    pub tournament_id: Option<String>,
    /// When the delegation was revoked (if revoked).
    pub revoked_at: Option<DateTime<Utc>>,
    /// User ID who revoked the delegation (if revoked).
    pub revoked_by_user_id: Option<String>,
    /// When the delegation was created.
    pub created_at: DateTime<Utc>,
    /// Whether the delegation is currently active.
    pub is_active: bool,
}

impl From<VetoDelegate> for VetoDelegateResponse {
    fn from(delegate: VetoDelegate) -> Self {
        Self {
            id: delegate.id.to_string(),
            team_season_id: delegate.team_season_id.to_string(),
            player_id: delegate.player_id.to_string(),
            delegated_by_user_id: delegate.delegated_by_user_id.to_string(),
            delegated_by_role: delegate.delegated_by_role.to_string(),
            tournament_id: delegate.tournament_id.map(|id| id.to_string()),
            revoked_at: delegate.revoked_at,
            revoked_by_user_id: delegate.revoked_by_user_id.map(|id| id.to_string()),
            created_at: delegate.created_at,
            is_active: delegate.is_active(),
        }
    }
}

/// Response containing a list of veto delegates.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VetoDelegateListResponse {
    /// List of active delegates.
    pub delegates: Vec<VetoDelegateResponse>,
}
