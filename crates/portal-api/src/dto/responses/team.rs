//! Team response DTOs.

use portal_domain::entities::team::{PlayerTeamMembership, Team, TeamMember};
use serde::Serialize;
use utoipa::ToSchema;

/// Team response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamResponse {
    /// Unique team identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Team display name.
    #[schema(example = "Cloud9")]
    pub name: String,

    /// Short team tag.
    #[schema(example = "C9")]
    pub tag: String,

    /// Team description.
    #[schema(example = "Professional esports organization")]
    pub description: Option<String>,

    /// Logo URL.
    #[schema(example = "https://example.com/logo.png")]
    pub logo_url: Option<String>,

    /// Banner URL.
    pub banner_url: Option<String>,

    /// Primary team color.
    #[schema(example = "#1E90FF")]
    pub primary_color: Option<String>,

    /// Secondary team color.
    #[schema(example = "#FFFFFF")]
    pub secondary_color: Option<String>,

    /// ID of the player who created the team.
    pub created_by: String,

    /// Game ID if team is game-specific.
    #[schema(example = "cs2")]
    pub game_id: Option<String>,

    /// Team status.
    #[schema(example = "active")]
    pub status: String,

    /// Total matches played.
    #[schema(example = 150)]
    pub total_matches: i32,

    /// Total wins.
    #[schema(example = 95)]
    pub total_wins: i32,

    /// Win rate (0.0 to 1.0).
    #[schema(example = 0.633)]
    pub win_rate: Option<f64>,

    /// When the team was created.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,

    /// When the team was last updated.
    pub updated_at: String,
}

impl From<Team> for TeamResponse {
    fn from(team: Team) -> Self {
        let win_rate = team.win_rate();
        Self {
            id: team.id.to_string(),
            name: team.name,
            tag: team.tag,
            description: team.description,
            logo_url: team.logo_url,
            banner_url: team.banner_url,
            primary_color: team.primary_color,
            secondary_color: team.secondary_color,
            created_by: team.created_by.to_string(),
            game_id: team.game_id,
            status: team.status.to_string(),
            total_matches: team.total_matches,
            total_wins: team.total_wins,
            win_rate,
            created_at: team.created_at.to_rfc3339(),
            updated_at: team.updated_at.to_rfc3339(),
        }
    }
}

/// Team member response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamMemberResponse {
    /// Player ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub player_id: String,

    /// Player's display name.
    #[schema(example = "ProGamer123")]
    pub display_name: String,

    /// Player's avatar URL.
    #[schema(example = "https://example.com/avatar.png")]
    pub avatar_url: Option<String>,

    /// Member's role in the team.
    #[schema(example = "captain")]
    pub role: String,

    /// Custom role title.
    #[schema(example = "IGL")]
    pub role_title: Option<String>,

    /// Whether this member is the team founder.
    pub is_founder: bool,

    /// Primary position (game-specific).
    #[schema(example = "entry")]
    pub primary_position: Option<String>,

    /// Secondary position (game-specific).
    pub secondary_position: Option<String>,

    /// Member status.
    #[schema(example = "active")]
    pub status: String,

    /// Jersey number.
    #[schema(example = 7)]
    pub jersey_number: Option<i32>,

    /// When the member joined.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub joined_at: String,
}

impl From<TeamMember> for TeamMemberResponse {
    fn from(member: TeamMember) -> Self {
        Self {
            player_id: member.player_id.to_string(),
            display_name: member.display_name,
            avatar_url: member.avatar_url,
            role: member.role.to_string(),
            role_title: member.role_title,
            is_founder: member.is_founder,
            primary_position: member.primary_position,
            secondary_position: member.secondary_position,
            status: member.status.to_string(),
            jersey_number: member.jersey_number,
            joined_at: member.joined_at.to_rfc3339(),
        }
    }
}

/// Team with members response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamWithMembersResponse {
    /// Team information.
    #[serde(flatten)]
    pub team: TeamResponse,

    /// Team members.
    pub members: Vec<TeamMemberResponse>,
}

/// A player's team membership (with team details).
/// Used for GET /players/{id}/teams to return the player's role in each team.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlayerTeamMembershipResponse {
    /// Team ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub team_id: String,

    /// Team display name.
    #[schema(example = "Cloud9")]
    pub team_name: String,

    /// Short team tag.
    #[schema(example = "C9")]
    pub team_tag: String,

    /// Team logo URL.
    #[schema(example = "https://example.com/logo.png")]
    pub team_logo_url: Option<String>,

    /// Player's role in the team.
    #[schema(example = "captain")]
    pub role: String,

    /// When the player joined this team.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub joined_at: String,
}

impl From<PlayerTeamMembership> for PlayerTeamMembershipResponse {
    fn from(m: PlayerTeamMembership) -> Self {
        Self {
            team_id: m.team_id.to_string(),
            team_name: m.team_name,
            team_tag: m.team_tag,
            team_logo_url: m.team_logo_url,
            role: m.role.to_string(),
            joined_at: m.joined_at.to_rfc3339(),
        }
    }
}
