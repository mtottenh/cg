//! League team domain entities.
//!
//! These entities represent teams that are scoped to a specific league season,
//! as opposed to global teams. Each league team exists within a season and
//! players can only be a primary member of one team per season.

use chrono::{DateTime, Utc};
use portal_core::types::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
use portal_core::{
    LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamInvitationId, LeagueTeamMemberId, UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// LEAGUE SEASON
// =============================================================================

/// A season within a league.
///
/// Seasons allow leagues to reset, reform teams, and track historical data.
/// Each season has its own teams and rosters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueSeason {
    pub id: LeagueSeasonId,
    pub league_id: LeagueId,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,

    // Timing
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub season_start: Option<DateTime<Utc>>,
    pub season_end: Option<DateTime<Utc>>,

    // Team settings
    pub team_size_min: i32,
    pub team_size_max: i32,
    pub max_substitutes: i32,
    pub max_teams: Option<i32>,

    // Roster lock
    pub roster_lock_status: RosterLockStatus,
    pub roster_locked_at: Option<DateTime<Utc>>,
    pub roster_locked_by: Option<UserId>,

    // Status
    pub status: SeasonStatus,

    // Metadata
    pub settings: serde_json::Value,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LeagueSeason {
    /// Check if the season is accepting team registrations.
    #[must_use]
    pub fn is_registration_open(&self) -> bool {
        if !self.status.is_registration_open() {
            return false;
        }

        let now = Utc::now();

        // Check registration window if defined
        if let Some(start) = self.registration_start {
            if now < start {
                return false;
            }
        }

        if let Some(end) = self.registration_end {
            if now > end {
                return false;
            }
        }

        true
    }

    /// Check if roster changes are allowed based on season status and lock.
    #[must_use]
    pub fn allows_roster_changes(&self) -> bool {
        self.status.allows_roster_changes() && self.roster_lock_status.allows_any_changes()
    }

    /// Check if primary roster changes are allowed.
    #[must_use]
    pub fn allows_primary_roster_changes(&self) -> bool {
        self.status.allows_roster_changes() && self.roster_lock_status.allows_primary_changes()
    }

    /// Check if substitute changes are allowed.
    #[must_use]
    pub fn allows_substitute_changes(&self) -> bool {
        self.status.allows_roster_changes() && self.roster_lock_status.allows_substitute_changes()
    }

    /// Check if the season is currently active (competition ongoing).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// Check if the season is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if a new team can be created in this season.
    #[must_use]
    pub fn can_create_team(&self) -> bool {
        self.is_registration_open()
    }
}

/// Command to create a new league season.
#[derive(Debug, Clone)]
pub struct CreateLeagueSeasonCommand {
    pub league_id: LeagueId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub season_start: Option<DateTime<Utc>>,
    pub season_end: Option<DateTime<Utc>>,
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub max_substitutes: Option<i32>,
    pub max_teams: Option<i32>,
}

/// Command to update a league season.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueSeasonCommand {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub season_start: Option<DateTime<Utc>>,
    pub season_end: Option<DateTime<Utc>>,
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub max_substitutes: Option<i32>,
    pub max_teams: Option<i32>,
    pub status: Option<SeasonStatus>,
    pub roster_lock_status: Option<RosterLockStatus>,
    pub settings: Option<serde_json::Value>,
}

// =============================================================================
// LEAGUE TEAM
// =============================================================================

/// A team within a league season.
///
/// Teams are scoped to a specific season and must meet roster requirements
/// to participate in competitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeam {
    pub id: LeagueTeamId,
    pub season_id: LeagueSeasonId,

    // Identity
    pub name: String,
    pub tag: String,

    // Profile
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,

    // Captain
    pub captain_user_id: UserId,

    // Status
    pub status: LeagueTeamStatus,

    // Registration
    pub registered_at: Option<DateTime<Utc>>,
    pub registration_notes: Option<String>,

    // Statistics
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,

    // Ranking
    pub seed: Option<i32>,
    pub rating: Option<i32>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub disbanded_at: Option<DateTime<Utc>>,
}

impl LeagueTeam {
    /// Check if the team can compete in matches.
    #[must_use]
    pub fn can_compete(&self) -> bool {
        self.status.can_compete()
    }

    /// Check if the team roster can be modified.
    #[must_use]
    pub fn can_modify_roster(&self) -> bool {
        self.status.can_modify_roster()
    }

    /// Check if the team is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if a user is the captain of this team.
    #[must_use]
    pub fn is_captain(&self, user_id: UserId) -> bool {
        self.captain_user_id == user_id
    }

    /// Calculate the team's win rate.
    #[must_use]
    pub fn win_rate(&self) -> Option<f64> {
        if self.matches_played == 0 {
            None
        } else {
            Some(f64::from(self.matches_won) / f64::from(self.matches_played))
        }
    }

    /// Get total matches (should equal played, but calculated for verification).
    #[must_use]
    pub fn total_matches(&self) -> i32 {
        self.matches_won + self.matches_lost + self.matches_drawn
    }
}

/// Command to create a new league team.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeamCommand {
    pub season_id: LeagueSeasonId,
    pub name: String,
    pub tag: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
}

/// Command to update a league team.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueTeamCommand {
    pub name: Option<String>,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
}

// =============================================================================
// LEAGUE TEAM MEMBER
// =============================================================================

/// A member of a league team.
///
/// Members have roles (captain, player, substitute) and can only be a primary
/// member (captain/player) of one team per season.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamMember {
    pub id: LeagueTeamMemberId,
    pub team_id: LeagueTeamId,
    pub user_id: UserId,

    // Role
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,

    // Status
    pub status: LeagueTeamMemberStatus,

    // Timestamps
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,

    // Who added this member
    pub added_by: Option<UserId>,
}

impl LeagueTeamMember {
    /// Check if the member is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status.is_active() && self.left_at.is_none()
    }

    /// Check if this is a primary member (captain or player).
    #[must_use]
    pub fn is_primary(&self) -> bool {
        self.role.is_primary()
    }

    /// Check if this member is a captain.
    #[must_use]
    pub fn is_captain(&self) -> bool {
        matches!(self.role, LeagueTeamRole::Captain)
    }

    /// Check if this member is a substitute.
    #[must_use]
    pub fn is_substitute(&self) -> bool {
        matches!(self.role, LeagueTeamRole::Substitute)
    }

    /// Check if this member can manage the team roster.
    #[must_use]
    pub fn can_manage_roster(&self) -> bool {
        self.is_active() && self.role.can_manage_roster()
    }
}

/// League team member with additional user info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamMemberWithUser {
    pub id: LeagueTeamMemberId,
    pub team_id: LeagueTeamId,
    pub user_id: UserId,
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
    pub status: LeagueTeamMemberStatus,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,

    // User info
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Command to add a member to a league team.
#[derive(Debug, Clone)]
pub struct AddLeagueTeamMemberCommand {
    pub team_id: LeagueTeamId,
    pub user_id: UserId,
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
}

// =============================================================================
// LEAGUE TEAM INVITATION
// =============================================================================

/// An invitation to join a league team (or request to join).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamInvitation {
    pub id: LeagueTeamInvitationId,
    pub team_id: LeagueTeamId,
    pub user_id: UserId,

    // Invitation details
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,

    // Who sent it
    pub invited_by: Option<UserId>,

    // Status
    pub status: LeagueTeamInvitationStatus,
    pub responded_at: Option<DateTime<Utc>>,
    pub response_message: Option<String>,

    // Expiration
    pub expires_at: DateTime<Utc>,

    // Timestamp
    pub created_at: DateTime<Utc>,
}

impl LeagueTeamInvitation {
    /// Check if this invitation is pending.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.status.is_actionable()
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

    /// Check if this is an invite (sent by team to player).
    #[must_use]
    pub fn is_invite(&self) -> bool {
        matches!(self.invitation_type, LeagueTeamInvitationType::Invite)
    }

    /// Check if this is a request (sent by player to team).
    #[must_use]
    pub fn is_request(&self) -> bool {
        matches!(self.invitation_type, LeagueTeamInvitationType::Request)
    }
}

/// League team invitation with additional team info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamInvitationWithTeam {
    pub id: LeagueTeamInvitationId,
    pub team_id: LeagueTeamId,
    pub user_id: UserId,
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,
    pub invited_by: Option<UserId>,
    pub status: LeagueTeamInvitationStatus,
    pub responded_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,

    // Team info
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,

    // Season info
    pub season_id: LeagueSeasonId,
    pub season_name: String,

    // League info
    pub league_id: LeagueId,
    pub league_name: String,
}

/// Command to create a league team invitation.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeamInvitationCommand {
    pub team_id: LeagueTeamId,
    pub user_id: UserId,
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,
}

// =============================================================================
// USER LEAGUE TEAM MEMBERSHIP
// =============================================================================

/// A user's membership in a league team (with context about season and league).
///
/// Used for fetching "what league teams is this user on?"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserLeagueTeamMembership {
    // Team info
    pub team_id: LeagueTeamId,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,

    // Membership info
    pub role: LeagueTeamRole,
    pub status: LeagueTeamMemberStatus,
    pub joined_at: DateTime<Utc>,

    // Season info
    pub season_id: LeagueSeasonId,
    pub season_name: String,
    pub season_status: SeasonStatus,

    // League info
    pub league_id: LeagueId,
    pub league_name: String,
}

// =============================================================================
// LEAGUE TEAM SUMMARY
// =============================================================================

/// Summary of a league team with member counts.
///
/// Used for listing teams with aggregated data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamSummary {
    pub team_id: LeagueTeamId,
    pub season_id: LeagueSeasonId,
    pub league_id: LeagueId,

    pub team_name: String,
    pub team_tag: String,
    pub team_status: LeagueTeamStatus,
    pub captain_user_id: UserId,

    pub active_member_count: i64,
    pub primary_member_count: i64,
    pub substitute_count: i64,

    pub team_size_min: i32,
    pub team_size_max: i32,
    pub roster_lock_status: RosterLockStatus,
}

impl LeagueTeamSummary {
    /// Check if the team has met the minimum roster size.
    #[must_use]
    pub fn has_minimum_roster(&self) -> bool {
        self.primary_member_count >= i64::from(self.team_size_min)
    }

    /// Check if the team is at maximum roster size.
    #[must_use]
    pub fn is_roster_full(&self) -> bool {
        self.primary_member_count >= i64::from(self.team_size_max)
    }

    /// Get remaining roster slots for primary members.
    #[must_use]
    pub fn remaining_roster_slots(&self) -> i64 {
        i64::from(self.team_size_max) - self.primary_member_count
    }
}
