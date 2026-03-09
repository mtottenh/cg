//! League domain entities.

use chrono::{DateTime, Utc};
use portal_core::{GameId, LeagueId, LeagueInvitationId, LeagueMemberId, LeagueSeasonId, UserId};
use serde::{Deserialize, Serialize};

/// A league that organizes tournaments for a specific game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct League {
    /// Unique identifier for the league.
    pub id: LeagueId,
    /// The game this league is for.
    pub game_id: GameId,
    /// Display name of the league.
    pub name: String,
    /// URL-friendly slug for the league.
    pub slug: String,
    /// Optional description of the league.
    pub description: Option<String>,
    /// URL to the league's logo image.
    pub logo_url: Option<String>,
    /// How users can join this league.
    pub access_type: LeagueAccessType,
    /// Current status of the league.
    pub status: LeagueStatus,
    /// Current active season for this league.
    pub current_season_id: Option<LeagueSeasonId>,
    /// League-specific settings as JSON.
    pub settings: serde_json::Value,
    /// User who created the league.
    pub created_by: UserId,
    /// When the league was created.
    pub created_at: DateTime<Utc>,
    /// When the league was last updated.
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
    pub const fn as_str(&self) -> &'static str {
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
    pub const fn as_str(&self) -> &'static str {
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
    /// Unique identifier for this membership.
    pub id: LeagueMemberId,
    /// The league this membership belongs to.
    pub league_id: LeagueId,
    /// The user who is a member.
    pub user_id: UserId,
    /// The member's role in the league.
    pub membership_type: LeagueMembershipType,
    /// When the user joined the league.
    pub joined_at: DateTime<Utc>,
}

/// League member with user info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueMemberWithUser {
    /// Unique identifier for this membership.
    pub id: LeagueMemberId,
    /// The league this membership belongs to.
    pub league_id: LeagueId,
    /// The user who is a member.
    pub user_id: UserId,
    /// The member's role in the league.
    pub membership_type: LeagueMembershipType,
    /// When the user joined the league.
    pub joined_at: DateTime<Utc>,
    /// The member's username.
    pub username: String,
    /// The member's email address.
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
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Moderator => "moderator",
            Self::Member => "member",
        }
    }

    /// Check if this role can manage other members.
    pub const fn can_manage_members(&self) -> bool {
        matches!(self, Self::Admin | Self::Moderator)
    }

    /// Check if this role can manage league settings.
    pub const fn can_manage_league(&self) -> bool {
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
    /// Unique identifier for this invitation.
    pub id: LeagueInvitationId,
    /// The league the invitation is for.
    pub league_id: LeagueId,
    /// The user being invited or applying.
    pub user_id: UserId,
    /// Whether this is an invite or application.
    pub invitation_type: LeagueInvitationType,
    /// Current status of the invitation.
    pub status: LeagueInvitationStatus,
    /// Optional message with the invitation.
    pub message: Option<String>,
    /// User who sent the invitation (for invites).
    pub invited_by: Option<UserId>,
    /// User who responded to the invitation.
    pub responded_by: Option<UserId>,
    /// When the invitation was responded to.
    pub responded_at: Option<DateTime<Utc>>,
    /// When the invitation expires.
    pub expires_at: Option<DateTime<Utc>>,
    /// When the invitation was created.
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
    pub const fn as_str(&self) -> &'static str {
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
    pub const fn as_str(&self) -> &'static str {
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
    /// The league ID.
    pub league_id: LeagueId,
    /// The league name.
    pub league_name: String,
    /// The league URL slug.
    pub league_slug: String,
    /// URL to the league's logo.
    pub league_logo_url: Option<String>,
    /// The game this league is for.
    pub game_id: GameId,
    /// The user's role in the league.
    pub membership_type: LeagueMembershipType,
    /// When the user joined.
    pub joined_at: DateTime<Utc>,
}

/// Command to create a new league.
#[derive(Debug, Clone)]
pub struct CreateLeagueCommand {
    /// The game this league is for.
    pub game_id: GameId,
    /// Display name of the league.
    pub name: String,
    /// URL-friendly slug.
    pub slug: String,
    /// Optional description.
    pub description: Option<String>,
    /// URL to the league's logo.
    pub logo_url: Option<String>,
    /// How users can join.
    pub access_type: LeagueAccessType,
    /// Optional league settings (entry requirements, etc.).
    pub settings: Option<serde_json::Value>,
}

/// Command to update a league.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueCommand {
    /// New name for the league.
    pub name: Option<String>,
    /// New URL slug.
    pub slug: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New logo URL.
    pub logo_url: Option<String>,
    /// New access type.
    pub access_type: Option<LeagueAccessType>,
    /// New status.
    pub status: Option<LeagueStatus>,
    /// New settings.
    pub settings: Option<serde_json::Value>,
}
