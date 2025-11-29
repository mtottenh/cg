//! Team-specific types.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A player's role within a team.
///
/// Role hierarchy (highest to lowest):
/// - Captain: Full team administration rights
/// - Officer: Can invite players, manage roster
/// - Player: Active competitive roster member
/// - Substitute: Backup player for matches
/// - Coach: Non-playing strategic role
/// - Manager: Non-playing administrative role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    /// Full team administration (invite, kick, promote, settings).
    Captain,
    /// Can invite players and manage roster.
    Officer,
    /// Active competitive roster member.
    #[default]
    Player,
    /// Backup player for matches.
    Substitute,
    /// Non-playing strategic advisor.
    Coach,
    /// Non-playing administrative role.
    Manager,
}

impl fmt::Display for TeamRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Captain => write!(f, "captain"),
            Self::Officer => write!(f, "officer"),
            Self::Player => write!(f, "player"),
            Self::Substitute => write!(f, "substitute"),
            Self::Coach => write!(f, "coach"),
            Self::Manager => write!(f, "manager"),
        }
    }
}

impl FromStr for TeamRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "captain" => Ok(Self::Captain),
            "officer" => Ok(Self::Officer),
            "player" => Ok(Self::Player),
            "substitute" => Ok(Self::Substitute),
            "coach" => Ok(Self::Coach),
            "manager" => Ok(Self::Manager),
            _ => Err(format!("invalid team role: {s}")),
        }
    }
}

impl TeamRole {
    /// Get the numeric hierarchy level (higher = more authority).
    ///
    /// This is primarily used for display ordering and informational purposes.
    /// Permission checks should use RBAC scoped permissions instead.
    #[must_use]
    pub fn hierarchy_level(&self) -> u8 {
        match self {
            Self::Captain => 100,
            Self::Officer => 80,
            Self::Player => 50,
            Self::Substitute => 40,
            Self::Coach => 30,
            Self::Manager => 30,
        }
    }

    /// Check if this role is a playing role (vs staff).
    ///
    /// This is an informational method - it indicates whether the role
    /// is for a player who competes vs a non-playing staff member.
    /// Actual permission to play in matches should use RBAC.
    #[must_use]
    pub fn is_playing_role(&self) -> bool {
        !matches!(self, Self::Coach | Self::Manager)
    }
}

/// Status of a team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    /// Team is active and can participate.
    #[default]
    Active,
    /// Team is inactive (e.g., on break).
    Inactive,
    /// Team has been disbanded.
    Disbanded,
    /// Team has been suspended by admins.
    Suspended,
}

impl fmt::Display for TeamStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Disbanded => write!(f, "disbanded"),
            Self::Suspended => write!(f, "suspended"),
        }
    }
}

impl FromStr for TeamStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "disbanded" => Ok(Self::Disbanded),
            "suspended" => Ok(Self::Suspended),
            _ => Err(format!("invalid team status: {s}")),
        }
    }
}

impl TeamStatus {
    /// Check if the team can participate in competitions.
    #[must_use]
    pub fn can_compete(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if the team can be modified (roster changes, settings).
    #[must_use]
    pub fn can_modify(&self) -> bool {
        matches!(self, Self::Active | Self::Inactive)
    }

    /// Check if the team is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Disbanded)
    }
}

/// Status of a team invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InvitationStatus {
    /// Invitation is pending response.
    #[default]
    Pending,
    /// Invitation was accepted.
    Accepted,
    /// Invitation was declined by invitee.
    Declined,
    /// Invitation was cancelled by team.
    Cancelled,
    /// Invitation expired without response.
    Expired,
}

impl fmt::Display for InvitationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Accepted => write!(f, "accepted"),
            Self::Declined => write!(f, "declined"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

impl FromStr for InvitationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "declined" => Ok(Self::Declined),
            "cancelled" => Ok(Self::Cancelled),
            "expired" => Ok(Self::Expired),
            _ => Err(format!("invalid invitation status: {s}")),
        }
    }
}

impl InvitationStatus {
    /// Check if the invitation can still be responded to.
    #[must_use]
    pub fn is_actionable(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Check if the invitation is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_role_hierarchy_level() {
        // Hierarchy levels are informational (for display ordering)
        assert!(TeamRole::Captain.hierarchy_level() > TeamRole::Officer.hierarchy_level());
        assert!(TeamRole::Officer.hierarchy_level() > TeamRole::Player.hierarchy_level());
        assert!(TeamRole::Player.hierarchy_level() > TeamRole::Substitute.hierarchy_level());
    }

    #[test]
    fn test_team_role_is_playing_role() {
        // is_playing_role is informational, not a permission check
        assert!(TeamRole::Captain.is_playing_role());
        assert!(TeamRole::Player.is_playing_role());
        assert!(TeamRole::Substitute.is_playing_role());
        assert!(!TeamRole::Coach.is_playing_role());
        assert!(!TeamRole::Manager.is_playing_role());
    }

    #[test]
    fn test_team_status_compete() {
        assert!(TeamStatus::Active.can_compete());
        assert!(!TeamStatus::Inactive.can_compete());
        assert!(!TeamStatus::Suspended.can_compete());
        assert!(!TeamStatus::Disbanded.can_compete());
    }

    #[test]
    fn test_invitation_status() {
        assert!(InvitationStatus::Pending.is_actionable());
        assert!(!InvitationStatus::Accepted.is_actionable());
        assert!(!InvitationStatus::Expired.is_actionable());
    }
}
