//! League database entities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `leagues` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueRow {
    pub id: Uuid,

    // Game association (UUID foreign key to games table)
    pub game_id: Uuid,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,

    // Access control
    pub access_type: String, // open, invite_only, application
    pub status: String,      // active, archived, suspended

    // Settings (JSONB)
    pub settings: serde_json::Value,

    // Audit
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new league.
#[derive(Debug, Clone)]
pub struct NewLeague {
    pub game_id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub access_type: String,
    pub created_by: Uuid,
}

/// Data for updating an existing league.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeague {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub access_type: Option<String>,
    pub status: Option<String>,
    pub settings: Option<serde_json::Value>,
}

/// Database row for the `league_members` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueMemberRow {
    pub id: Uuid,

    // Relationships
    pub league_id: Uuid,
    pub user_id: Uuid,

    // Role
    pub membership_type: String, // admin, moderator, member

    // Timestamps
    pub joined_at: DateTime<Utc>,
}

/// League member with user info (from JOIN).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueMemberWithUserRow {
    // Member info
    pub id: Uuid,
    pub league_id: Uuid,
    pub user_id: Uuid,
    pub membership_type: String,
    pub joined_at: DateTime<Utc>,

    // User info (from JOIN with users table)
    pub username: String,
    pub email: String,
}

/// Data for inserting a new league member.
#[derive(Debug, Clone)]
pub struct NewLeagueMember {
    pub league_id: Uuid,
    pub user_id: Uuid,
    pub membership_type: String,
}

/// Database row for the `league_invitations` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueInvitationRow {
    pub id: Uuid,

    // Relationships
    pub league_id: Uuid,
    pub user_id: Uuid,

    // Invitation details
    pub invitation_type: String, // invite, application
    pub status: String,          // pending, accepted, rejected, expired
    pub message: Option<String>,

    // Who sent/responded
    pub invited_by: Option<Uuid>,
    pub responded_by: Option<Uuid>,
    pub responded_at: Option<DateTime<Utc>>,

    // Expiration
    pub expires_at: Option<DateTime<Utc>>,

    // Timestamps
    pub created_at: DateTime<Utc>,
}

/// Data for inserting a new league invitation.
#[derive(Debug, Clone)]
pub struct NewLeagueInvitation {
    pub league_id: Uuid,
    pub user_id: Uuid,
    pub invitation_type: String,
    pub message: Option<String>,
    pub invited_by: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Data for updating an invitation status.
#[derive(Debug, Clone)]
pub struct UpdateLeagueInvitation {
    pub status: String,
    pub responded_by: Uuid,
}

/// User's league membership with league details.
/// Used for fetching "what leagues is this user in?"
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserLeagueMembershipRow {
    // League info
    pub league_id: Uuid,
    pub league_name: String,
    pub league_slug: String,
    pub league_logo_url: Option<String>,
    pub game_id: Uuid,

    // Membership info
    pub membership_type: String,
    pub joined_at: DateTime<Utc>,
}
