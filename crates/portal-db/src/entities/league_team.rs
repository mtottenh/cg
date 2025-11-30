//! League team database entities.
//!
//! These entities map to the `league_seasons`, `league_teams`, `league_team_seasons`,
//! `league_team_members`, `league_team_invitations`, and `league_season_participants` tables.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// =============================================================================
// LEAGUE SEASON
// =============================================================================

/// Database row for the `league_seasons` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueSeasonRow {
    pub id: Uuid,
    pub league_id: Uuid,

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
    pub roster_lock_status: String,
    pub roster_locked_at: Option<DateTime<Utc>>,
    pub roster_locked_by: Option<Uuid>,

    // Status
    pub status: String,

    // Metadata
    pub settings: serde_json::Value,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new league season.
#[derive(Debug, Clone)]
pub struct NewLeagueSeason {
    pub league_id: Uuid,
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
    pub created_by: Uuid,
}

/// Data for updating an existing league season.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueSeason {
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
    pub roster_lock_status: Option<String>,
    pub roster_locked_at: Option<DateTime<Utc>>,
    pub roster_locked_by: Option<Uuid>,
    pub status: Option<String>,
    pub settings: Option<serde_json::Value>,
}

// =============================================================================
// LEAGUE TEAM (Persistent Identity - League-Scoped)
// =============================================================================

/// Database row for the `league_teams` table.
///
/// Teams belong to a league (not a season) and have persistent identity.
/// Seasonal participation is tracked via `league_team_seasons`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueTeamRow {
    pub id: Uuid,
    pub league_id: Uuid,

    // Identity (persistent)
    pub name: String,
    pub name_normalized: String,
    pub tag: String,
    pub tag_normalized: String,

    // Profile (persistent)
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,

    // Ownership (permanent owner, can transfer)
    pub owner_player_id: Uuid,

    // Status
    pub status: String,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub disbanded_at: Option<DateTime<Utc>>,
}

/// Data for inserting a new league team.
#[derive(Debug, Clone)]
pub struct NewLeagueTeam {
    pub league_id: Uuid,
    pub name: String,
    pub tag: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub owner_player_id: Uuid,
}

/// Data for updating an existing league team.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueTeam {
    pub name: Option<String>,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub status: Option<String>,
    pub disbanded_at: Option<DateTime<Utc>>,
}

// =============================================================================
// LEAGUE TEAM SEASON (Seasonal Participation)
// =============================================================================

/// Database row for the `league_team_seasons` table.
///
/// Tracks a team's participation in a specific season.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueTeamSeasonRow {
    pub id: Uuid,
    pub team_id: Uuid,
    pub season_id: Uuid,

    // Status
    pub status: String,

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

/// Data for inserting a new league team season registration.
#[derive(Debug, Clone)]
pub struct NewLeagueTeamSeason {
    pub team_id: Uuid,
    pub season_id: Uuid,
    pub status: Option<String>,
    pub registration_notes: Option<String>,
}

/// Data for updating a league team season.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueTeamSeason {
    pub status: Option<String>,
    pub registration_notes: Option<String>,
    pub matches_played: Option<i32>,
    pub matches_won: Option<i32>,
    pub matches_lost: Option<i32>,
    pub matches_drawn: Option<i32>,
    pub seed: Option<i32>,
    pub rating: Option<i32>,
}

/// League team summary row with member counts (from view or aggregated query).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueTeamSummaryRow {
    // Team info (persistent)
    pub team_id: Uuid,
    pub league_id: Uuid,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,
    pub owner_player_id: Uuid,
    pub team_status: String,

    // Season participation info
    pub team_season_id: Option<Uuid>,
    pub season_id: Option<Uuid>,
    pub season_status: Option<String>,

    // Member counts (for current season)
    pub active_member_count: i64,
    pub captain_count: i64,
    pub player_count: i64,
    pub substitute_count: i64,

    // Season settings
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub roster_lock_status: Option<String>,
}

// =============================================================================
// LEAGUE TEAM MEMBER (Seasonal Roster)
// =============================================================================

/// Database row for the `league_team_members` table.
///
/// Members belong to a team's seasonal roster (via `team_season_id`).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueTeamMemberRow {
    pub id: Uuid,
    pub team_season_id: Uuid,
    pub player_id: Uuid,
    /// Denormalized `season_id` (auto-populated by trigger)
    pub season_id: Uuid,

    // Role
    pub role: String,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,

    // Status
    pub status: String,

    // Timestamps
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,

    // Added by (user_id since this is an admin/captain action)
    pub added_by: Option<Uuid>,
}

/// League team member with player details (from JOIN).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueTeamMemberWithPlayerRow {
    pub id: Uuid,
    pub team_season_id: Uuid,
    pub player_id: Uuid,
    pub role: String,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
    pub status: String,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,

    // Player info
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Data for inserting a new league team member.
#[derive(Debug, Clone)]
pub struct NewLeagueTeamMember {
    pub team_season_id: Uuid,
    pub player_id: Uuid,
    pub role: String,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
    pub added_by: Option<Uuid>,
}

/// Data for updating an existing league team member.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueTeamMember {
    pub role: Option<String>,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
    pub status: Option<String>,
    pub left_at: Option<DateTime<Utc>>,
}

// =============================================================================
// LEAGUE TEAM INVITATION
// =============================================================================

/// Database row for the `league_team_invitations` table.
///
/// Invitations target a team's seasonal roster (via `team_season_id`).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueTeamInvitationRow {
    pub id: Uuid,
    pub team_season_id: Uuid,
    pub player_id: Uuid,

    // Invitation details
    pub invitation_type: String,
    pub role: String,
    pub message: Option<String>,

    // Sender (user_id since this is an admin/captain action)
    pub invited_by: Option<Uuid>,

    // Status
    pub status: String,
    pub responded_at: Option<DateTime<Utc>>,
    pub response_message: Option<String>,

    // Expiration
    pub expires_at: DateTime<Utc>,

    // Timestamps
    pub created_at: DateTime<Utc>,
}

/// League team invitation with team and season details (from JOINs).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueTeamInvitationWithTeamRow {
    pub id: Uuid,
    pub team_season_id: Uuid,
    pub player_id: Uuid,
    pub invitation_type: String,
    pub role: String,
    pub message: Option<String>,
    pub invited_by: Option<Uuid>,
    pub status: String,
    pub responded_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,

    // Team info
    pub team_id: Uuid,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,

    // Season info
    pub season_id: Uuid,
    pub season_name: String,

    // League info
    pub league_id: Uuid,
    pub league_name: String,
}

/// Data for inserting a new league team invitation.
#[derive(Debug, Clone)]
pub struct NewLeagueTeamInvitation {
    pub team_season_id: Uuid,
    pub player_id: Uuid,
    pub invitation_type: String,
    pub role: String,
    pub message: Option<String>,
    pub invited_by: Option<Uuid>,
}

/// Data for updating an invitation response.
#[derive(Debug, Clone)]
pub struct UpdateLeagueTeamInvitation {
    pub status: String,
    pub response_message: Option<String>,
}

// =============================================================================
// LEAGUE SEASON PARTICIPANT (For Individual Format)
// =============================================================================

/// Database row for the `league_season_participants` table.
///
/// Used for individual format leagues (1v1 tournaments).
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LeagueSeasonParticipantRow {
    pub id: Uuid,
    pub season_id: Uuid,
    pub player_id: Uuid,

    // Status
    pub status: String,

    // Ranking
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

/// Data for inserting a new league season participant.
#[derive(Debug, Clone)]
pub struct NewLeagueSeasonParticipant {
    pub season_id: Uuid,
    pub player_id: Uuid,
    pub seed: Option<i32>,
}

/// Data for updating a league season participant.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueSeasonParticipant {
    pub status: Option<String>,
    pub seed: Option<i32>,
    pub rating: Option<i32>,
    pub matches_played: Option<i32>,
    pub matches_won: Option<i32>,
    pub matches_lost: Option<i32>,
    pub matches_drawn: Option<i32>,
    pub withdrawn_at: Option<DateTime<Utc>>,
}

// =============================================================================
// PLAYER LEAGUE TEAM MEMBERSHIP
// =============================================================================

/// A player's membership in a league team, including team/season/league details.
/// Used for fetching "what league teams is this player on?"
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerLeagueTeamMembershipRow {
    // Player info
    pub player_id: Uuid,

    // Team info
    pub team_id: Uuid,
    pub team_name: String,
    pub team_tag: String,
    pub team_logo_url: Option<String>,

    // Team season info
    pub team_season_id: Uuid,
    pub team_season_status: String,

    // Membership info
    pub role: String,
    pub membership_status: String,
    pub joined_at: DateTime<Utc>,

    // Season info
    pub season_id: Uuid,
    pub season_name: String,
    pub season_status: String,

    // League info
    pub league_id: Uuid,
    pub league_name: String,
}
