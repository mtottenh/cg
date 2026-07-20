//! League team response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::league_team::{
    LeagueSeason, LeagueSeasonParticipant, LeagueTeam, LeagueTeamInvitation,
    LeagueTeamInvitationWithTeam, LeagueTeamMember, LeagueTeamMemberWithPlayer, LeagueTeamSeason,
    LeagueTeamSummary, PlayerLeagueTeamMembership,
};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// LEAGUE SEASON RESPONSES
// =============================================================================

/// Response DTO for a league season.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueSeasonResponse {
    pub id: String,
    pub league_id: String,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // Timing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_start: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_end: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_start: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_end: Option<DateTime<Utc>>,

    // Team settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_size_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_size_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_substitutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_teams: Option<i32>,

    // Status
    pub roster_lock_status: String,
    pub status: String,

    // Metadata
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<LeagueSeason> for LeagueSeasonResponse {
    fn from(season: LeagueSeason) -> Self {
        Self {
            id: season.id.to_string(),
            league_id: season.league_id.to_string(),
            name: season.name,
            slug: season.slug,
            description: season.description,
            registration_start: season.registration_start,
            registration_end: season.registration_end,
            season_start: season.season_start,
            season_end: season.season_end,
            team_size_min: season.team_size_min,
            team_size_max: season.team_size_max,
            max_substitutes: season.max_substitutes,
            max_teams: season.max_teams,
            roster_lock_status: season.roster_lock_status.to_string(),
            status: season.status.to_string(),
            created_by: season.created_by.to_string(),
            created_at: season.created_at,
            updated_at: season.updated_at,
        }
    }
}

// =============================================================================
// LEAGUE TEAM RESPONSES (Persistent Identity)
// =============================================================================

/// Response DTO for a league team (persistent identity).
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamResponse {
    pub id: String,
    pub league_id: String,

    // Identity (persistent)
    pub name: String,
    pub tag: String,

    // Profile (persistent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_color: Option<String>,

    // Ownership (permanent owner)
    pub owner_player_id: String,

    // Status
    pub status: String,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disbanded_at: Option<DateTime<Utc>>,
}

impl From<LeagueTeam> for LeagueTeamResponse {
    fn from(team: LeagueTeam) -> Self {
        Self {
            id: team.id.to_string(),
            league_id: team.league_id.to_string(),
            name: team.name,
            tag: team.tag,
            description: team.description,
            logo_url: team.logo_url,
            banner_url: team.banner_url,
            primary_color: team.primary_color,
            secondary_color: team.secondary_color,
            owner_player_id: team.owner_player_id.to_string(),
            status: team.status.to_string(),
            created_at: team.created_at,
            updated_at: team.updated_at,
            disbanded_at: team.disbanded_at,
        }
    }
}

// =============================================================================
// LEAGUE TEAM SEASON RESPONSES (Seasonal Participation)
// =============================================================================

/// Response DTO for a team's seasonal participation.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamSeasonResponse {
    pub id: String,
    pub team_id: String,
    pub season_id: String,

    // Status
    pub status: String,

    // Registration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_notes: Option<String>,

    // Statistics (season-specific)
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,

    // Ranking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<i32>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<LeagueTeamSeason> for LeagueTeamSeasonResponse {
    fn from(ts: LeagueTeamSeason) -> Self {
        Self {
            id: ts.id.to_string(),
            team_id: ts.team_id.to_string(),
            season_id: ts.season_id.to_string(),
            status: ts.status.to_string(),
            registered_at: ts.registered_at,
            registration_notes: ts.registration_notes,
            matches_played: ts.matches_played,
            matches_won: ts.matches_won,
            matches_lost: ts.matches_lost,
            matches_drawn: ts.matches_drawn,
            seed: ts.seed,
            rating: ts.rating,
            created_at: ts.created_at,
            updated_at: ts.updated_at,
        }
    }
}

/// Combined response for team with season participation.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamWithSeasonResponse {
    // Team info (persistent)
    pub team: LeagueTeamResponse,
    // Season participation info
    pub team_season: LeagueTeamSeasonResponse,
}

/// Summary response for listing teams in a season.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamSummaryResponse {
    pub team_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_season_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<String>,
    pub league_id: String,

    pub team_name: String,
    pub team_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_logo_url: Option<String>,
    pub team_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_status: Option<String>,
    pub owner_player_id: String,

    pub active_member_count: i64,
    pub captain_count: i64,
    pub player_count: i64,
    pub substitute_count: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_size_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_size_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roster_lock_status: Option<String>,
}

impl From<LeagueTeamSummary> for LeagueTeamSummaryResponse {
    fn from(summary: LeagueTeamSummary) -> Self {
        Self {
            team_id: summary.team_id.to_string(),
            team_season_id: summary.team_season_id.map(|id| id.to_string()),
            season_id: summary.season_id.map(|id| id.to_string()),
            league_id: summary.league_id.to_string(),
            team_name: summary.team_name,
            team_tag: summary.team_tag,
            team_logo_url: summary.team_logo_url,
            team_status: summary.team_status.to_string(),
            season_status: summary.season_status.map(|s| s.to_string()),
            owner_player_id: summary.owner_player_id.to_string(),
            active_member_count: summary.active_member_count,
            captain_count: summary.captain_count,
            player_count: summary.player_count,
            substitute_count: summary.substitute_count,
            team_size_min: summary.team_size_min,
            team_size_max: summary.team_size_max,
            roster_lock_status: summary.roster_lock_status.map(|s| s.to_string()),
        }
    }
}

// =============================================================================
// LEAGUE TEAM MEMBER RESPONSES (Seasonal Roster)
// =============================================================================

/// Response DTO for a league team member.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamMemberResponse {
    pub id: String,
    pub team_season_id: String,
    pub player_id: String,
    pub season_id: String,

    // Role
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jersey_number: Option<i32>,

    // Status
    pub status: String,

    // Timestamps
    pub joined_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_at: Option<DateTime<Utc>>,

    // Who added
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
}

impl From<LeagueTeamMember> for LeagueTeamMemberResponse {
    fn from(member: LeagueTeamMember) -> Self {
        Self {
            id: member.id.to_string(),
            team_season_id: member.team_season_id.to_string(),
            player_id: member.player_id.to_string(),
            season_id: member.season_id.to_string(),
            role: member.role.to_string(),
            position: member.position,
            jersey_number: member.jersey_number,
            status: member.status.to_string(),
            joined_at: member.joined_at,
            left_at: member.left_at,
            added_by: member.added_by.map(|u| u.to_string()),
        }
    }
}

/// Response DTO for a league team member with player details.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamMemberWithPlayerResponse {
    pub id: String,
    pub team_season_id: String,
    pub player_id: String,

    // Role
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jersey_number: Option<i32>,

    // Status
    pub status: String,

    // Timestamps
    pub joined_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_at: Option<DateTime<Utc>>,

    // Player info
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

impl From<LeagueTeamMemberWithPlayer> for LeagueTeamMemberWithPlayerResponse {
    fn from(member: LeagueTeamMemberWithPlayer) -> Self {
        Self {
            id: member.id.to_string(),
            team_season_id: member.team_season_id.to_string(),
            player_id: member.player_id.to_string(),
            role: member.role.to_string(),
            position: member.position,
            jersey_number: member.jersey_number,
            status: member.status.to_string(),
            joined_at: member.joined_at,
            left_at: member.left_at,
            display_name: member.display_name,
            avatar_url: member.avatar_url,
        }
    }
}

// =============================================================================
// PLAYER MEMBERSHIP RESPONSES
// =============================================================================

/// Response DTO for a player's league team membership.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlayerLeagueTeamMembershipResponse {
    // Player info
    pub player_id: String,

    // Team info
    pub team_id: String,
    pub team_season_id: String,
    pub team_name: String,
    pub team_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_logo_url: Option<String>,

    // Membership info
    pub role: String,
    pub status: String,
    pub joined_at: DateTime<Utc>,

    // Season info
    pub season_id: String,
    pub season_name: String,
    pub season_status: String,

    // League info
    pub league_id: String,
    pub league_name: String,
}

impl From<PlayerLeagueTeamMembership> for PlayerLeagueTeamMembershipResponse {
    fn from(membership: PlayerLeagueTeamMembership) -> Self {
        Self {
            player_id: membership.player_id.to_string(),
            team_id: membership.team_id.to_string(),
            team_season_id: membership.team_season_id.to_string(),
            team_name: membership.team_name,
            team_tag: membership.team_tag,
            team_logo_url: membership.team_logo_url,
            role: membership.role.to_string(),
            status: membership.status.to_string(),
            joined_at: membership.joined_at,
            season_id: membership.season_id.to_string(),
            season_name: membership.season_name,
            season_status: membership.season_status.to_string(),
            league_id: membership.league_id.to_string(),
            league_name: membership.league_name,
        }
    }
}

// =============================================================================
// LEAGUE TEAM INVITATION RESPONSES
// =============================================================================

/// Response DTO for a league team invitation.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamInvitationResponse {
    pub id: String,
    pub team_season_id: String,
    pub player_id: String,

    // Invited player info (enriched by list handlers; absent when the
    // player could not be resolved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_avatar_url: Option<String>,

    // Invitation details
    pub invitation_type: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    // Who invited
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited_by: Option<String>,

    // Status
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_message: Option<String>,

    // Expiration
    pub expires_at: DateTime<Utc>,

    // Timestamp
    pub created_at: DateTime<Utc>,
}

impl From<LeagueTeamInvitation> for LeagueTeamInvitationResponse {
    fn from(inv: LeagueTeamInvitation) -> Self {
        Self {
            id: inv.id.to_string(),
            team_season_id: inv.team_season_id.to_string(),
            player_id: inv.player_id.to_string(),
            player_display_name: None,
            player_avatar_url: None,
            invitation_type: inv.invitation_type.to_string(),
            role: inv.role.to_string(),
            message: inv.message,
            invited_by: inv.invited_by.map(|u| u.to_string()),
            status: inv.status.to_string(),
            responded_at: inv.responded_at,
            response_message: inv.response_message,
            expires_at: inv.expires_at,
            created_at: inv.created_at,
        }
    }
}

impl LeagueTeamInvitationResponse {
    /// Attach the invited player's display info.
    #[must_use]
    pub fn with_player(mut self, player: &portal_domain::entities::Player) -> Self {
        self.player_display_name = Some(player.display_name.clone());
        self.player_avatar_url.clone_from(&player.avatar_url);
        self
    }
}

/// Response DTO for a league team invitation with team context.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueTeamInvitationWithTeamResponse {
    pub id: String,
    pub team_season_id: String,
    pub player_id: String,

    // Invitation details
    pub invitation_type: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    // Who invited
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited_by: Option<String>,

    // Status
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_at: Option<DateTime<Utc>>,

    // Expiration
    pub expires_at: DateTime<Utc>,

    // Timestamp
    pub created_at: DateTime<Utc>,

    // Team info
    pub team_id: String,
    pub team_name: String,
    pub team_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_logo_url: Option<String>,

    // Season info
    pub season_id: String,
    pub season_name: String,

    // League info
    pub league_id: String,
    pub league_name: String,
}

impl From<LeagueTeamInvitationWithTeam> for LeagueTeamInvitationWithTeamResponse {
    fn from(inv: LeagueTeamInvitationWithTeam) -> Self {
        Self {
            id: inv.id.to_string(),
            team_season_id: inv.team_season_id.to_string(),
            player_id: inv.player_id.to_string(),
            invitation_type: inv.invitation_type.to_string(),
            role: inv.role.to_string(),
            message: inv.message,
            invited_by: inv.invited_by.map(|u| u.to_string()),
            status: inv.status.to_string(),
            responded_at: inv.responded_at,
            expires_at: inv.expires_at,
            created_at: inv.created_at,
            team_id: inv.team_id.to_string(),
            team_name: inv.team_name,
            team_tag: inv.team_tag,
            team_logo_url: inv.team_logo_url,
            season_id: inv.season_id.to_string(),
            season_name: inv.season_name,
            league_id: inv.league_id.to_string(),
            league_name: inv.league_name,
        }
    }
}

// =============================================================================
// LEAGUE SEASON PARTICIPANT RESPONSES (Individual Format)
// =============================================================================

/// Response DTO for an individual format season participant.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueSeasonParticipantResponse {
    pub id: String,
    pub season_id: String,
    pub player_id: String,

    // Status
    pub status: String,

    // Ranking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<i32>,

    // Statistics
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,

    // Timestamps
    pub registered_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdrawn_at: Option<DateTime<Utc>>,
}

impl From<LeagueSeasonParticipant> for LeagueSeasonParticipantResponse {
    fn from(p: LeagueSeasonParticipant) -> Self {
        Self {
            id: p.id.to_string(),
            season_id: p.season_id.to_string(),
            player_id: p.player_id.to_string(),
            status: p.status.to_string(),
            seed: p.seed,
            rating: p.rating,
            matches_played: p.matches_played,
            matches_won: p.matches_won,
            matches_lost: p.matches_lost,
            matches_drawn: p.matches_drawn,
            registered_at: p.registered_at,
            withdrawn_at: p.withdrawn_at,
        }
    }
}
