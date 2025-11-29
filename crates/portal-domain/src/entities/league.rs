//! League domain entities.

use chrono::{DateTime, Utc};
use portal_core::{GameId, LeagueId, LeagueInvitationId, LeagueMemberId, UserId};
use serde::{Deserialize, Serialize};

/// A league that organizes tournaments for a specific game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct League {
    pub id: LeagueId,
    /// Game UUID identifier.
    pub game_id: GameId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub access_type: LeagueAccessType,
    pub status: LeagueStatus,
    pub settings: serde_json::Value,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// League access type determines how users can join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeagueAccessType {
    /// Anyone can join without approval.
    Open,
    /// Must be invited by a league admin/moderator.
    InviteOnly,
    /// User applies, admin must approve.
    Application,
}

impl LeagueAccessType {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "invite_only" => Some(Self::InviteOnly),
            "application" => Some(Self::Application),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InviteOnly => "invite_only",
            Self::Application => "application",
        }
    }
}

impl std::fmt::Display for LeagueAccessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// League status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeagueStatus {
    /// League is operational.
    Active,
    /// League is read-only/archived.
    Archived,
    /// League is temporarily suspended.
    Suspended,
}

impl LeagueStatus {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            "suspended" => Some(Self::Suspended),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Suspended => "suspended",
        }
    }
}

impl std::fmt::Display for LeagueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A member of a league.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueMember {
    pub id: LeagueMemberId,
    pub league_id: LeagueId,
    pub user_id: UserId,
    pub membership_type: LeagueMembershipType,
    pub joined_at: DateTime<Utc>,
}

/// League member with user info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueMemberWithUser {
    pub id: LeagueMemberId,
    pub league_id: LeagueId,
    pub user_id: UserId,
    pub membership_type: LeagueMembershipType,
    pub joined_at: DateTime<Utc>,
    pub username: String,
    pub email: String,
}

/// League membership type/role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeagueMembershipType {
    /// Full control over the league.
    Admin,
    /// Can manage members but not league settings.
    Moderator,
    /// Regular participant.
    Member,
}

impl LeagueMembershipType {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Self::Admin),
            "moderator" => Some(Self::Moderator),
            "member" => Some(Self::Member),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Moderator => "moderator",
            Self::Member => "member",
        }
    }

    /// Check if this role can manage other members.
    pub fn can_manage_members(&self) -> bool {
        matches!(self, Self::Admin | Self::Moderator)
    }

    /// Check if this role can manage league settings.
    pub fn can_manage_league(&self) -> bool {
        matches!(self, Self::Admin)
    }
}

impl std::fmt::Display for LeagueMembershipType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An invitation to join a league (or application to join).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueInvitation {
    pub id: LeagueInvitationId,
    pub league_id: LeagueId,
    pub user_id: UserId,
    pub invitation_type: LeagueInvitationType,
    pub status: LeagueInvitationStatus,
    pub message: Option<String>,
    pub invited_by: Option<UserId>,
    pub responded_by: Option<UserId>,
    pub responded_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Type of league invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeagueInvitationType {
    /// Admin invites a user.
    Invite,
    /// User applies to join.
    Application,
}

impl LeagueInvitationType {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "invite" => Some(Self::Invite),
            "application" => Some(Self::Application),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invite => "invite",
            Self::Application => "application",
        }
    }
}

impl std::fmt::Display for LeagueInvitationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Status of a league invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeagueInvitationStatus {
    /// Waiting for response.
    Pending,
    /// Accepted.
    Accepted,
    /// Rejected.
    Rejected,
    /// Expired (time limit passed).
    Expired,
}

impl LeagueInvitationStatus {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            "rejected" => Some(Self::Rejected),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }
}

impl std::fmt::Display for LeagueInvitationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// User's league membership with league details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserLeagueMembership {
    pub league_id: LeagueId,
    pub league_name: String,
    pub league_slug: String,
    pub league_logo_url: Option<String>,
    /// Game UUID identifier.
    pub game_id: GameId,
    pub membership_type: LeagueMembershipType,
    pub joined_at: DateTime<Utc>,
}

/// Command to create a new league.
#[derive(Debug, Clone)]
pub struct CreateLeagueCommand {
    pub game_id: GameId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub access_type: LeagueAccessType,
}

/// Command to update a league.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueCommand {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub access_type: Option<LeagueAccessType>,
    pub status: Option<LeagueStatus>,
    pub settings: Option<serde_json::Value>,
}
