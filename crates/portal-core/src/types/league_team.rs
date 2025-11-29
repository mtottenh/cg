//! League team-specific types.
//!
//! These types are used for league-scoped teams (as opposed to global teams).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Status of a league season.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SeasonStatus {
    /// Season is being configured.
    #[default]
    Draft,
    /// Open for team formation.
    Registration,
    /// Competition in progress.
    Active,
    /// Playoff stage.
    Playoffs,
    /// Season finished.
    Completed,
    /// Season cancelled.
    Cancelled,
}

impl fmt::Display for SeasonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Registration => write!(f, "registration"),
            Self::Active => write!(f, "active"),
            Self::Playoffs => write!(f, "playoffs"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for SeasonStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Self::Draft),
            "registration" => Ok(Self::Registration),
            "active" => Ok(Self::Active),
            "playoffs" => Ok(Self::Playoffs),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid season status: {s}")),
        }
    }
}

impl SeasonStatus {
    /// Check if the season is accepting team registrations.
    #[must_use]
    pub fn is_registration_open(&self) -> bool {
        matches!(self, Self::Registration)
    }

    /// Check if the season is currently active (competition ongoing).
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::Playoffs)
    }

    /// Check if the season allows roster changes.
    #[must_use]
    pub fn allows_roster_changes(&self) -> bool {
        matches!(self, Self::Draft | Self::Registration)
    }

    /// Check if the season is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// Roster lock status for a season.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RosterLockStatus {
    /// Teams can modify rosters freely.
    #[default]
    Open,
    /// Minor changes allowed (substitutes only).
    SoftLock,
    /// No roster changes allowed.
    HardLock,
}

impl fmt::Display for RosterLockStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::SoftLock => write!(f, "soft_lock"),
            Self::HardLock => write!(f, "hard_lock"),
        }
    }
}

impl FromStr for RosterLockStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "soft_lock" => Ok(Self::SoftLock),
            "hard_lock" => Ok(Self::HardLock),
            _ => Err(format!("invalid roster lock status: {s}")),
        }
    }
}

impl RosterLockStatus {
    /// Check if primary roster members can be added/removed.
    #[must_use]
    pub fn allows_primary_changes(&self) -> bool {
        matches!(self, Self::Open)
    }

    /// Check if substitute members can be added/removed.
    #[must_use]
    pub fn allows_substitute_changes(&self) -> bool {
        matches!(self, Self::Open | Self::SoftLock)
    }

    /// Check if any roster changes are allowed.
    #[must_use]
    pub fn allows_any_changes(&self) -> bool {
        !matches!(self, Self::HardLock)
    }
}

/// Status of a league team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LeagueTeamStatus {
    /// Still recruiting, roster incomplete.
    #[default]
    Forming,
    /// Submitted for registration review.
    Pending,
    /// Fully registered and active.
    Active,
    /// Removed from competition.
    Disqualified,
    /// Voluntarily disbanded.
    Disbanded,
    /// Eliminated from tournament/playoffs.
    Eliminated,
}

impl fmt::Display for LeagueTeamStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forming => write!(f, "forming"),
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Disqualified => write!(f, "disqualified"),
            Self::Disbanded => write!(f, "disbanded"),
            Self::Eliminated => write!(f, "eliminated"),
        }
    }
}

impl FromStr for LeagueTeamStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "forming" => Ok(Self::Forming),
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "disqualified" => Ok(Self::Disqualified),
            "disbanded" => Ok(Self::Disbanded),
            "eliminated" => Ok(Self::Eliminated),
            _ => Err(format!("invalid league team status: {s}")),
        }
    }
}

impl LeagueTeamStatus {
    /// Check if the team can compete in matches.
    #[must_use]
    pub fn can_compete(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if the team roster can be modified.
    #[must_use]
    pub fn can_modify_roster(&self) -> bool {
        matches!(self, Self::Forming | Self::Pending | Self::Active)
    }

    /// Check if the team is in a terminal state (cannot be reactivated).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Disbanded | Self::Disqualified)
    }
}

/// Role of a member within a league team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LeagueTeamRole {
    /// Team leader, can manage roster.
    Captain,
    /// Primary roster player.
    #[default]
    Player,
    /// Backup player (can be on multiple teams).
    Substitute,
}

impl fmt::Display for LeagueTeamRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Captain => write!(f, "captain"),
            Self::Player => write!(f, "player"),
            Self::Substitute => write!(f, "substitute"),
        }
    }
}

impl FromStr for LeagueTeamRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "captain" => Ok(Self::Captain),
            "player" => Ok(Self::Player),
            "substitute" => Ok(Self::Substitute),
            _ => Err(format!("invalid league team role: {s}")),
        }
    }
}

impl LeagueTeamRole {
    /// Check if this is a primary role (counts toward roster minimum).
    #[must_use]
    pub fn is_primary(&self) -> bool {
        matches!(self, Self::Captain | Self::Player)
    }

    /// Check if this role can manage the team roster.
    #[must_use]
    pub fn can_manage_roster(&self) -> bool {
        matches!(self, Self::Captain)
    }
}

/// Status of a league team member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LeagueTeamMemberStatus {
    /// Currently on roster.
    #[default]
    Active,
    /// Temporarily unavailable.
    Inactive,
    /// Left the team.
    Left,
    /// Removed by captain/admin.
    Removed,
}

impl fmt::Display for LeagueTeamMemberStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Left => write!(f, "left"),
            Self::Removed => write!(f, "removed"),
        }
    }
}

impl FromStr for LeagueTeamMemberStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "left" => Ok(Self::Left),
            "removed" => Ok(Self::Removed),
            _ => Err(format!("invalid league team member status: {s}")),
        }
    }
}

impl LeagueTeamMemberStatus {
    /// Check if the member is currently on the active roster.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if the member can be re-added to the team.
    #[must_use]
    pub fn can_rejoin(&self) -> bool {
        matches!(self, Self::Inactive | Self::Left)
    }
}

/// Type of league team invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LeagueTeamInvitationType {
    /// Captain invites a player.
    #[default]
    Invite,
    /// Player requests to join.
    Request,
}

impl fmt::Display for LeagueTeamInvitationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invite => write!(f, "invite"),
            Self::Request => write!(f, "request"),
        }
    }
}

impl FromStr for LeagueTeamInvitationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "invite" => Ok(Self::Invite),
            "request" => Ok(Self::Request),
            _ => Err(format!("invalid league team invitation type: {s}")),
        }
    }
}

/// Status of a league team invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LeagueTeamInvitationStatus {
    /// Waiting for response.
    #[default]
    Pending,
    /// Invitation was accepted.
    Accepted,
    /// Invitation was declined.
    Declined,
    /// Invitation expired.
    Expired,
    /// Invitation was cancelled.
    Cancelled,
}

impl fmt::Display for LeagueTeamInvitationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Accepted => write!(f, "accepted"),
            Self::Declined => write!(f, "declined"),
            Self::Expired => write!(f, "expired"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for LeagueTeamInvitationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "declined" => Ok(Self::Declined),
            "expired" => Ok(Self::Expired),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid league team invitation status: {s}")),
        }
    }
}

impl LeagueTeamInvitationStatus {
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
    fn test_season_status_roundtrip() {
        for status in [
            SeasonStatus::Draft,
            SeasonStatus::Registration,
            SeasonStatus::Active,
            SeasonStatus::Playoffs,
            SeasonStatus::Completed,
            SeasonStatus::Cancelled,
        ] {
            let s = status.to_string();
            let parsed: SeasonStatus = s.parse().unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_roster_lock_status() {
        assert!(RosterLockStatus::Open.allows_primary_changes());
        assert!(RosterLockStatus::Open.allows_substitute_changes());

        assert!(!RosterLockStatus::SoftLock.allows_primary_changes());
        assert!(RosterLockStatus::SoftLock.allows_substitute_changes());

        assert!(!RosterLockStatus::HardLock.allows_primary_changes());
        assert!(!RosterLockStatus::HardLock.allows_substitute_changes());
    }

    #[test]
    fn test_league_team_status() {
        assert!(!LeagueTeamStatus::Forming.can_compete());
        assert!(LeagueTeamStatus::Active.can_compete());
        assert!(!LeagueTeamStatus::Disbanded.can_compete());

        assert!(LeagueTeamStatus::Forming.can_modify_roster());
        assert!(LeagueTeamStatus::Active.can_modify_roster());
        assert!(!LeagueTeamStatus::Disbanded.can_modify_roster());
    }

    #[test]
    fn test_league_team_role() {
        assert!(LeagueTeamRole::Captain.is_primary());
        assert!(LeagueTeamRole::Player.is_primary());
        assert!(!LeagueTeamRole::Substitute.is_primary());

        assert!(LeagueTeamRole::Captain.can_manage_roster());
        assert!(!LeagueTeamRole::Player.can_manage_roster());
    }

    #[test]
    fn test_league_team_invitation_status() {
        assert!(LeagueTeamInvitationStatus::Pending.is_actionable());
        assert!(!LeagueTeamInvitationStatus::Accepted.is_actionable());
        assert!(!LeagueTeamInvitationStatus::Expired.is_actionable());
    }
}
