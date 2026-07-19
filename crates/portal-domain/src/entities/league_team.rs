//! League team domain entities.
//!
//! Teams have persistent identity at the league level, with seasonal participation
//! via `LeagueTeamSeason`. Rosters are per-season, allowing players to change teams
//! between seasons while teams maintain their identity (name, logo, etc.).
//!
//! Key relationships:
//!   League -> `LeagueTeam` (persistent identity)
//!          -> `LeagueTeamSeason` (seasonal participation)
//!               -> `LeagueTeamMember` (seasonal roster)
//!
//! Note on `UserId` vs `PlayerId`:
//! - `PlayerId` is used for player-related operations (team membership, invitations)
//! - `UserId` is used for admin/audit fields (`created_by`, `added_by`, `invited_by`, `locked_by`)

use chrono::{DateTime, Utc};
use portal_core::types::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamSeasonStatus, LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
use portal_core::{
    LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamInvitationId, LeagueTeamMemberId,
    LeagueTeamSeasonId, PlayerId, UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// LEAGUE SEASON
// =============================================================================

/// A season within a league.
///
/// Seasons allow leagues to reset, reform teams, and track historical data.
/// Each season has its own rosters (but teams persist across seasons).
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
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub max_substitutes: Option<i32>,
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
        if let Some(start) = self.registration_start
            && now < start
        {
            return false;
        }

        if let Some(end) = self.registration_end
            && now > end
        {
            return false;
        }

        true
    }

    /// Check if roster changes are allowed based on season status and lock.
    #[must_use]
    pub const fn allows_roster_changes(&self) -> bool {
        self.status.allows_roster_changes() && self.roster_lock_status.allows_any_changes()
    }

    /// Check if primary roster changes are allowed.
    #[must_use]
    pub const fn allows_primary_roster_changes(&self) -> bool {
        self.status.allows_roster_changes() && self.roster_lock_status.allows_primary_changes()
    }

    /// Check if substitute changes are allowed.
    #[must_use]
    pub const fn allows_substitute_changes(&self) -> bool {
        self.status.allows_roster_changes() && self.roster_lock_status.allows_substitute_changes()
    }

    /// Check if the season is currently active (competition ongoing).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// Check if the season is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if a new team can register for this season.
    #[must_use]
    pub fn can_register_team(&self) -> bool {
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
// LEAGUE TEAM (Persistent Identity)
// =============================================================================

/// A team within a league with persistent identity.
///
/// Teams belong to a league (not a season) and maintain their identity
/// (name, logo, colors) across seasons. Seasonal participation is tracked
/// via `LeagueTeamSeason`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeam {
    pub id: LeagueTeamId,
    pub league_id: LeagueId,

    // Identity (persistent)
    pub name: String,
    pub tag: String,

    // Profile (persistent)
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,

    // Ownership (permanent owner, can transfer)
    pub owner_player_id: PlayerId,

    // Status
    pub status: LeagueTeamStatus,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub disbanded_at: Option<DateTime<Utc>>,
}

impl LeagueTeam {
    /// Check if the team is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, LeagueTeamStatus::Active)
    }

    /// Check if the team can participate in seasons.
    #[must_use]
    pub const fn can_participate(&self) -> bool {
        self.is_active()
    }

    /// Check if a player is the owner of this team.
    #[must_use]
    pub fn is_owner(&self, player_id: PlayerId) -> bool {
        self.owner_player_id == player_id
    }

    /// Check if the team is in a terminal state.
    #[must_use]
    pub const fn is_disbanded(&self) -> bool {
        matches!(self.status, LeagueTeamStatus::Disbanded)
    }
}

/// Command to create a new league team.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeamCommand {
    pub league_id: LeagueId,
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
// LEAGUE TEAM SEASON (Seasonal Participation)
// =============================================================================

/// A team's participation in a specific season.
///
/// This tracks the team's status, roster, and statistics for a single season.
/// A team can participate in multiple seasons, with different rosters each time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamSeason {
    pub id: LeagueTeamSeasonId,
    pub team_id: LeagueTeamId,
    pub season_id: LeagueSeasonId,

    // Status
    pub status: LeagueTeamSeasonStatus,

    // Registration
    pub registered_at: Option<DateTime<Utc>>,
    pub registration_notes: Option<String>,

    // Statistics (season-specific)
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
}

impl LeagueTeamSeason {
    /// Check if the team can compete in matches this season.
    #[must_use]
    pub const fn can_compete(&self) -> bool {
        self.status.can_compete()
    }

    /// Check if the team roster can be modified this season.
    #[must_use]
    pub const fn can_modify_roster(&self) -> bool {
        self.status.can_modify_roster()
    }

    /// Check if the team is in a terminal state for this season.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Calculate the team's win rate for this season.
    #[must_use]
    pub fn win_rate(&self) -> Option<f64> {
        if self.matches_played == 0 {
            None
        } else {
            Some(f64::from(self.matches_won) / f64::from(self.matches_played))
        }
    }
}

/// Command to register a team for a season.
#[derive(Debug, Clone)]
pub struct RegisterTeamForSeasonCommand {
    pub team_id: LeagueTeamId,
    pub season_id: LeagueSeasonId,
}

// =============================================================================
// LEAGUE TEAM MEMBER (Seasonal Roster)
// =============================================================================

/// A member of a league team's seasonal roster.
///
/// Members have roles (captain, player, substitute) and can only be a primary
/// member (captain/player) of one team per season. Multiple captains are allowed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamMember {
    pub id: LeagueTeamMemberId,
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,
    pub season_id: LeagueSeasonId,

    // Role
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,

    // Status
    pub status: LeagueTeamMemberStatus,

    // Timestamps
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,

    // Who added this member (user_id since this is an admin/captain action)
    pub added_by: Option<UserId>,
}

impl LeagueTeamMember {
    /// Check if the member is currently active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.status.is_active() && self.left_at.is_none()
    }

    /// Check if this is a primary member (captain or player).
    #[must_use]
    pub const fn is_primary(&self) -> bool {
        self.role.is_primary()
    }

    /// Check if this member is a captain.
    #[must_use]
    pub const fn is_captain(&self) -> bool {
        matches!(self.role, LeagueTeamRole::Captain)
    }

    /// Check if this member is a substitute.
    #[must_use]
    pub const fn is_substitute(&self) -> bool {
        matches!(self.role, LeagueTeamRole::Substitute)
    }

    /// Check if this member can manage the team roster.
    #[must_use]
    pub const fn can_manage_roster(&self) -> bool {
        self.is_active() && self.role.can_manage_roster()
    }
}

/// League team member with additional player info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamMemberWithPlayer {
    pub id: LeagueTeamMemberId,
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
    pub status: LeagueTeamMemberStatus,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,

    // Player info
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Command to add a member to a league team's seasonal roster.
#[derive(Debug, Clone)]
pub struct AddLeagueTeamMemberCommand {
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
}

// =============================================================================
// LEAGUE TEAM INVITATION
// =============================================================================

/// An invitation to join a league team's roster (or request to join).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamInvitation {
    pub id: LeagueTeamInvitationId,
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,

    // Invitation details
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,

    // Who sent it (user_id since this is an admin/captain action)
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
    pub const fn is_pending(&self) -> bool {
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
    pub const fn is_invite(&self) -> bool {
        matches!(self.invitation_type, LeagueTeamInvitationType::Invite)
    }

    /// Check if this is a request (sent by player to team).
    #[must_use]
    pub const fn is_request(&self) -> bool {
        matches!(self.invitation_type, LeagueTeamInvitationType::Request)
    }
}

/// League team invitation with additional team info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamInvitationWithTeam {
    pub id: LeagueTeamInvitationId,
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,
    pub invited_by: Option<UserId>,
    pub status: LeagueTeamInvitationStatus,
    pub responded_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,

    // Team info
    pub team_id: LeagueTeamId,
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
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,
}

// =============================================================================
// LEAGUE SEASON PARTICIPANT (For Individual Format)
// =============================================================================

/// A player's participation in an individual format league season.
///
/// Used for 1v1 tournaments and other formats where teams are not required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueSeasonParticipant {
    pub id: uuid::Uuid,
    pub season_id: LeagueSeasonId,
    pub player_id: PlayerId,

    // Status
    pub status: LeagueSeasonParticipantStatus,

    // Seed/Rating
    pub seed: Option<i32>,
    pub rating: Option<i32>,

    // Statistics
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,

    // Timestamps
    pub registered_at: DateTime<Utc>,
    pub withdrawn_at: Option<DateTime<Utc>>,
}

/// Status of a participant in an individual format league.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeagueSeasonParticipantStatus {
    Registered,
    Active,
    Eliminated,
    Disqualified,
    Withdrawn,
}

impl std::fmt::Display for LeagueSeasonParticipantStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registered => write!(f, "registered"),
            Self::Active => write!(f, "active"),
            Self::Eliminated => write!(f, "eliminated"),
            Self::Disqualified => write!(f, "disqualified"),
            Self::Withdrawn => write!(f, "withdrawn"),
        }
    }
}

impl std::str::FromStr for LeagueSeasonParticipantStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "registered" => Ok(Self::Registered),
            "active" => Ok(Self::Active),
            "eliminated" => Ok(Self::Eliminated),
            "disqualified" => Ok(Self::Disqualified),
            "withdrawn" => Ok(Self::Withdrawn),
            _ => Err(format!("invalid participant status: {s}")),
        }
    }
}

impl LeagueSeasonParticipant {
    /// Check if the participant is actively competing.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(
            self.status,
            LeagueSeasonParticipantStatus::Registered | LeagueSeasonParticipantStatus::Active
        )
    }

    /// Calculate the participant's win rate.
    #[must_use]
    pub fn win_rate(&self) -> Option<f64> {
        if self.matches_played == 0 {
            None
        } else {
            Some(f64::from(self.matches_won) / f64::from(self.matches_played))
        }
    }
}

// =============================================================================
// PLAYER LEAGUE TEAM MEMBERSHIP
// =============================================================================

/// A player's membership in a league team (with context about season and league).
///
/// Used for fetching "what league teams is this player on?"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerLeagueTeamMembership {
    // Player info
    pub player_id: PlayerId,

    // Team info
    pub team_id: LeagueTeamId,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,

    // Team season info
    pub team_season_id: LeagueTeamSeasonId,
    pub team_season_status: LeagueTeamSeasonStatus,

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

/// Summary of a league team's seasonal participation with member counts.
///
/// Used for listing teams with aggregated data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueTeamSummary {
    // Team info (persistent)
    pub team_id: LeagueTeamId,
    pub league_id: LeagueId,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,
    pub owner_player_id: PlayerId,
    pub team_status: LeagueTeamStatus,

    // Season participation info
    pub team_season_id: Option<LeagueTeamSeasonId>,
    pub season_id: Option<LeagueSeasonId>,
    pub season_status: Option<LeagueTeamSeasonStatus>,

    // Member counts (for current season)
    pub active_member_count: i64,
    pub captain_count: i64,
    pub player_count: i64,
    pub substitute_count: i64,

    // Season settings
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub roster_lock_status: Option<RosterLockStatus>,
}

impl LeagueTeamSummary {
    /// Check if the team has met the minimum roster size.
    #[must_use]
    pub fn has_minimum_roster(&self) -> bool {
        if let Some(min) = self.team_size_min {
            (self.captain_count + self.player_count) >= i64::from(min)
        } else {
            true
        }
    }

    /// Check if the team is at maximum roster size.
    #[must_use]
    pub fn is_roster_full(&self) -> bool {
        if let Some(max) = self.team_size_max {
            (self.captain_count + self.player_count) >= i64::from(max)
        } else {
            false
        }
    }

    /// Get remaining roster slots for primary members.
    #[must_use]
    pub fn remaining_roster_slots(&self) -> Option<i64> {
        self.team_size_max
            .map(|max| i64::from(max) - self.captain_count - self.player_count)
    }
}
