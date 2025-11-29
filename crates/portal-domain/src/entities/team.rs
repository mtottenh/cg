//! Team domain entities.

use chrono::{DateTime, Utc};
use portal_core::types::{InvitationStatus, TeamRole, TeamStatus};
use portal_core::{PlayerId, TeamId, TeamInvitationId};

/// Team domain entity.
#[derive(Debug, Clone)]
pub struct Team {
    pub id: TeamId,
    pub name: String,
    pub tag: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub created_by: PlayerId,
    pub game_id: Option<String>,
    pub status: TeamStatus,
    pub disbanded_at: Option<DateTime<Utc>>,
    pub disbanded_reason: Option<String>,
    pub total_matches: i32,
    pub total_wins: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Team {
    /// Check if the team is active and can participate.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == TeamStatus::Active
    }

    /// Check if the team can be modified.
    #[must_use]
    pub fn can_modify(&self) -> bool {
        self.status.can_modify()
    }

    /// Check if the team can compete in matches.
    #[must_use]
    pub fn can_compete(&self) -> bool {
        self.status.can_compete()
    }

    /// Calculate the team's win rate.
    #[must_use]
    pub fn win_rate(&self) -> Option<f64> {
        if self.total_matches == 0 {
            None
        } else {
            Some(f64::from(self.total_wins) / f64::from(self.total_matches))
        }
    }

    /// Check if a player is the founder.
    #[must_use]
    pub fn is_founder(&self, player_id: PlayerId) -> bool {
        self.created_by == player_id
    }
}

/// Team member domain entity.
#[derive(Debug, Clone)]
pub struct TeamMember {
    pub team_id: TeamId,
    pub player_id: PlayerId,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: TeamRole,
    pub role_title: Option<String>,
    pub is_founder: bool,
    pub primary_position: Option<String>,
    pub secondary_position: Option<String>,
    pub status: TeamMemberStatus,
    pub jersey_number: Option<i32>,
    pub invited_by: Option<PlayerId>,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

/// Status of a team member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TeamMemberStatus {
    #[default]
    Active,
    Inactive,
    Benched,
    Trial,
}

impl std::fmt::Display for TeamMemberStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Benched => write!(f, "benched"),
            Self::Trial => write!(f, "trial"),
        }
    }
}

impl std::str::FromStr for TeamMemberStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "benched" => Ok(Self::Benched),
            "trial" => Ok(Self::Trial),
            _ => Err(format!("invalid team member status: {s}")),
        }
    }
}

impl TeamMember {
    /// Check if the member is currently active in the team.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == TeamMemberStatus::Active && self.left_at.is_none()
    }

    /// Check if the member is a captain.
    #[must_use]
    pub fn is_captain(&self) -> bool {
        self.role == TeamRole::Captain
    }
}

/// Command to create a new team.
#[derive(Debug, Clone)]
pub struct CreateTeamCommand {
    pub name: String,
    pub tag: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub game_id: Option<String>,
}

/// Command to update a team.
#[derive(Debug, Clone, Default)]
pub struct UpdateTeamCommand {
    pub name: Option<String>,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub website_url: Option<String>,
}

/// Command to invite a player to a team.
#[derive(Debug, Clone)]
pub struct InvitePlayerCommand {
    pub player_id: PlayerId,
    pub role: TeamRole,
    pub message: Option<String>,
}

/// Command to update a member's role.
#[derive(Debug, Clone)]
pub struct UpdateMemberRoleCommand {
    pub player_id: PlayerId,
    pub new_role: TeamRole,
}

/// A player's membership in a team (with basic team details).
/// Used for fetching "what teams is this player on?"
#[derive(Debug, Clone)]
pub struct PlayerTeamMembership {
    pub team_id: TeamId,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
}

/// Team invitation domain entity.
#[derive(Debug, Clone)]
pub struct TeamInvitation {
    pub id: TeamInvitationId,
    pub team_id: TeamId,
    pub player_id: PlayerId,
    pub invitation_type: InvitationType,
    pub role: TeamRole,
    pub message: Option<String>,
    pub invited_by: Option<PlayerId>,
    pub status: InvitationStatus,
    pub responded_at: Option<DateTime<Utc>>,
    pub response_message: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Type of team invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvitationType {
    /// Invitation sent by team captain to a player.
    #[default]
    Invite,
    /// Request sent by a player to join a team.
    Request,
}

impl std::fmt::Display for InvitationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invite => write!(f, "invite"),
            Self::Request => write!(f, "request"),
        }
    }
}

impl std::str::FromStr for InvitationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "invite" => Ok(Self::Invite),
            "request" => Ok(Self::Request),
            _ => Err(format!("invalid invitation type: {s}")),
        }
    }
}

impl TeamInvitation {
    /// Check if this invitation is pending.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.status == InvitationStatus::Pending
    }

    /// Check if this invitation has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }

    /// Check if this invitation can be acted upon.
    #[must_use]
    pub fn is_actionable(&self) -> bool {
        self.is_pending() && !self.is_expired()
    }
}

