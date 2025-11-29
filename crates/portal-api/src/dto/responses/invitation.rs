//! Team invitation response DTOs.

use portal_domain::entities::team::TeamInvitation;
use serde::Serialize;
use utoipa::ToSchema;

/// Team invitation response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamInvitationResponse {
    /// Unique invitation identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Team ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440001")]
    pub team_id: String,

    /// Invited player ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440002")]
    pub player_id: String,

    /// Type of invitation (invite or request).
    #[schema(example = "invite")]
    pub invitation_type: String,

    /// Role being offered.
    #[schema(example = "player")]
    pub role: String,

    /// Optional message with the invitation.
    #[schema(example = "We'd love to have you on our team!")]
    pub message: Option<String>,

    /// ID of player who sent the invitation.
    pub invited_by: Option<String>,

    /// Invitation status.
    #[schema(example = "pending")]
    pub status: String,

    /// When the invitation was responded to.
    pub responded_at: Option<String>,

    /// Response message.
    pub response_message: Option<String>,

    /// When the invitation expires.
    #[schema(example = "2024-01-22T10:30:00Z")]
    pub expires_at: String,

    /// When the invitation was created.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,
}

impl From<TeamInvitation> for TeamInvitationResponse {
    fn from(invitation: TeamInvitation) -> Self {
        Self {
            id: invitation.id.to_string(),
            team_id: invitation.team_id.to_string(),
            player_id: invitation.player_id.to_string(),
            invitation_type: invitation.invitation_type.to_string(),
            role: invitation.role.to_string(),
            message: invitation.message,
            invited_by: invitation.invited_by.map(|id| id.to_string()),
            status: invitation.status.to_string(),
            responded_at: invitation.responded_at.map(|dt| dt.to_rfc3339()),
            response_message: invitation.response_message,
            expires_at: invitation.expires_at.to_rfc3339(),
            created_at: invitation.created_at.to_rfc3339(),
        }
    }
}

/// Team invitation with team info for the player's pending invitations list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamInvitationWithTeamResponse {
    /// The invitation details.
    #[serde(flatten)]
    pub invitation: TeamInvitationResponse,

    /// Team name.
    #[schema(example = "Cloud9")]
    pub team_name: String,

    /// Team tag.
    #[schema(example = "C9")]
    pub team_tag: String,

    /// Team logo URL.
    pub team_logo_url: Option<String>,
}

/// Count of pending invitations.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InvitationCountResponse {
    /// Number of pending invitations.
    #[schema(example = 3)]
    pub count: i64,
}
