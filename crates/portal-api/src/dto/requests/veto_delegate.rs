//! Veto delegate request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request to create a veto delegation.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreateVetoDelegateRequest {
    /// Player ID to delegate veto authority to.
    /// The player must be a member of the team.
    pub player_id: String,

    /// Optional tournament ID to scope the delegation.
    /// If not provided, the delegation applies to all tournaments.
    #[serde(default)]
    pub tournament_id: Option<String>,
}
