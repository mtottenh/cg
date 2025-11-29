//! Team database entities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `teams` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TeamRow {
    pub id: Uuid,

    // Identity
    pub name: String,
    pub name_normalized: String,
    pub tag: String,
    pub tag_normalized: String,

    // Profile
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,

    // Founding Captain
    pub created_by: Uuid,

    // Game Association
    pub game_id: Option<String>,

    // Settings (JSONB)
    pub settings: serde_json::Value,

    // Social
    pub social_links: serde_json::Value,
    pub website_url: Option<String>,

    // Status
    pub status: String,
    pub disbanded_at: Option<DateTime<Utc>>,
    pub disbanded_reason: Option<String>,

    // Statistics
    pub total_matches: i32,
    pub total_wins: i32,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new team.
#[derive(Debug, Clone)]
pub struct NewTeam {
    pub name: String,
    pub tag: String,
    pub created_by: Uuid,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub game_id: Option<String>,
}

/// Data for updating an existing team.
#[derive(Debug, Clone, Default)]
pub struct UpdateTeam {
    pub name: Option<String>,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub social_links: Option<serde_json::Value>,
    pub website_url: Option<String>,
    pub status: Option<String>,
    pub disbanded_at: Option<DateTime<Utc>>,
    pub disbanded_reason: Option<String>,
}

/// Database row for the `team_members` table (with player info from JOIN).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TeamMemberRow {
    pub id: Uuid,

    // Relationships
    pub team_id: Uuid,
    pub player_id: Uuid,

    // Player info (from JOIN with players table)
    pub display_name: String,
    pub avatar_url: Option<String>,

    // Role
    pub role: String,
    pub role_title: Option<String>,

    // Founder Flag
    pub is_founder: bool,

    // Position (game-specific)
    pub primary_position: Option<String>,
    pub secondary_position: Option<String>,

    // Status
    pub status: String,

    // Jersey/Number
    pub jersey_number: Option<i32>,

    // Invited By
    pub invited_by: Option<Uuid>,

    // Timestamps
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

/// Data for inserting a new team member.
#[derive(Debug, Clone)]
pub struct NewTeamMember {
    pub team_id: Uuid,
    pub player_id: Uuid,
    pub role: String,
    pub is_founder: bool,
    pub invited_by: Option<Uuid>,
}

/// Data for updating an existing team member.
#[derive(Debug, Clone, Default)]
pub struct UpdateTeamMember {
    pub role: Option<String>,
    pub role_title: Option<String>,
    pub primary_position: Option<String>,
    pub secondary_position: Option<String>,
    pub status: Option<String>,
    pub jersey_number: Option<i32>,
    pub left_at: Option<DateTime<Utc>>,
}

/// Database row for the `team_invitations` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TeamInvitationRow {
    pub id: Uuid,

    // Relationships
    pub team_id: Uuid,
    pub player_id: Uuid,

    // Invitation Details
    #[sqlx(rename = "type")]
    pub invitation_type: String,
    pub role: String,
    pub message: Option<String>,

    // Sender
    pub invited_by: Option<Uuid>,

    // Status
    pub status: String,

    // Response
    pub responded_at: Option<DateTime<Utc>>,
    pub response_message: Option<String>,

    // Expiration
    pub expires_at: DateTime<Utc>,

    // Timestamps
    pub created_at: DateTime<Utc>,
}

/// Data for inserting a new team invitation.
#[derive(Debug, Clone)]
pub struct NewTeamInvitation {
    pub team_id: Uuid,
    pub player_id: Uuid,
    pub invitation_type: String,
    pub role: String,
    pub message: Option<String>,
    pub invited_by: Option<Uuid>,
}

/// Data for updating an invitation response.
#[derive(Debug, Clone)]
pub struct UpdateTeamInvitation {
    pub status: String,
    pub response_message: Option<String>,
}

/// A player's membership in a team, including team details.
/// Used for fetching "what teams is this player on?"
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerTeamMembershipRow {
    // Team info
    pub team_id: Uuid,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,

    // Membership info
    pub role: String,
    pub joined_at: DateTime<Utc>,
}
