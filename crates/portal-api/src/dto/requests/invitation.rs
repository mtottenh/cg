//! Team invitation request DTOs.

use portal_core::types::TeamRole;
use portal_core::ValidationError;
use portal_domain::entities::team::InvitePlayerCommand;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request to invite a player to a team.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct InvitePlayerRequest {
    /// The player ID to invite.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub player_id: String,

    /// The role to assign to the player.
    #[schema(example = "player")]
    #[serde(default = "default_role")]
    pub role: String,

    /// Optional message to include with the invitation.
    #[validate(length(max = 500))]
    #[schema(example = "We'd love to have you on our team!")]
    pub message: Option<String>,
}

fn default_role() -> String {
    "player".to_string()
}

impl InvitePlayerRequest {
    /// Convert to domain command.
    ///
    /// # Errors
    /// Returns error if the player ID or role is invalid.
    pub fn into_command(self) -> Result<InvitePlayerCommand, ValidationError> {
        let player_id = self.player_id.parse().map_err(|_| {
            ValidationError::field(portal_core::errors::FieldError::format(
                "player_id",
                "a valid UUID",
            ))
        })?;

        let role: TeamRole = self.role.parse().map_err(|_| {
            ValidationError::field(portal_core::errors::FieldError::format(
                "role",
                "a valid team role (captain, officer, player, substitute, coach, manager)",
            ))
        })?;

        Ok(InvitePlayerCommand {
            player_id,
            role,
            message: self.message,
        })
    }
}

/// Request to respond to an invitation.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RespondToInvitationRequest {
    /// Optional response message.
    #[schema(example = "Thanks for the invite!")]
    pub message: Option<String>,
}
