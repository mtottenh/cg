//! League team request DTOs.

use chrono::{DateTime, Utc};
use portal_core::types::{LeagueTeamInvitationType, LeagueTeamRole, RosterLockStatus, SeasonStatus};
use portal_core::{LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamSeasonId, PlayerId};
use portal_domain::entities::league_team::{
    AddLeagueTeamMemberCommand, CreateLeagueSeasonCommand, CreateLeagueTeamCommand,
    CreateLeagueTeamInvitationCommand, RegisterTeamForSeasonCommand, UpdateLeagueSeasonCommand,
    UpdateLeagueTeamCommand,
};
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Validate URL-friendly slug format.
fn validate_slug(slug: &str) -> Result<(), validator::ValidationError> {
    // Must contain only lowercase letters, numbers, and hyphens
    // Must start and end with alphanumeric
    let bytes = slug.as_bytes();
    if bytes.is_empty() {
        return Err(validator::ValidationError::new("slug_empty"));
    }

    // Check first and last characters
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(validator::ValidationError::new("slug_invalid_start"));
    }
    if !(last.is_ascii_lowercase() || last.is_ascii_digit()) {
        return Err(validator::ValidationError::new("slug_invalid_end"));
    }

    // Check all characters
    for &b in bytes {
        if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return Err(validator::ValidationError::new("slug_invalid_chars"));
        }
    }

    Ok(())
}

// =============================================================================
// LEAGUE SEASON REQUESTS
// =============================================================================

/// Request to create a new league season.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateLeagueSeasonRequest {
    /// League ID (UUID).
    pub league_id: String,

    /// Season name.
    #[validate(length(min = 2, max = 100))]
    pub name: String,

    /// URL-friendly slug.
    #[validate(length(min = 2, max = 100), custom(function = "validate_slug"))]
    pub slug: String,

    /// Optional description.
    #[validate(length(max = 2000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Registration start time.
    #[serde(default)]
    pub registration_start: Option<DateTime<Utc>>,

    /// Registration end time.
    #[serde(default)]
    pub registration_end: Option<DateTime<Utc>>,

    /// Season start time.
    #[serde(default)]
    pub season_start: Option<DateTime<Utc>>,

    /// Season end time.
    #[serde(default)]
    pub season_end: Option<DateTime<Utc>>,

    /// Minimum team size.
    #[validate(range(min = 1, max = 50))]
    #[serde(default)]
    pub team_size_min: Option<i32>,

    /// Maximum team size.
    #[validate(range(min = 1, max = 50))]
    #[serde(default)]
    pub team_size_max: Option<i32>,

    /// Maximum substitutes per team.
    #[validate(range(min = 0, max = 20))]
    #[serde(default)]
    pub max_substitutes: Option<i32>,

    /// Maximum teams in the season.
    #[validate(range(min = 2, max = 1000))]
    #[serde(default)]
    pub max_teams: Option<i32>,
}

impl TryFrom<CreateLeagueSeasonRequest> for CreateLeagueSeasonCommand {
    type Error = crate::error::ApiError;

    fn try_from(req: CreateLeagueSeasonRequest) -> Result<Self, Self::Error> {
        let league_id: LeagueId = req
            .league_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid league ID format"))?;

        Ok(Self {
            league_id,
            name: req.name,
            slug: req.slug,
            description: req.description,
            registration_start: req.registration_start,
            registration_end: req.registration_end,
            season_start: req.season_start,
            season_end: req.season_end,
            team_size_min: req.team_size_min,
            team_size_max: req.team_size_max,
            max_substitutes: req.max_substitutes,
            max_teams: req.max_teams,
        })
    }
}

/// Request to update a league season.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateLeagueSeasonRequest {
    /// Updated name.
    #[validate(length(min = 2, max = 100))]
    #[serde(default)]
    pub name: Option<String>,

    /// Updated slug.
    #[validate(length(min = 2, max = 100))]
    #[serde(default)]
    pub slug: Option<String>,

    /// Updated description.
    #[validate(length(max = 2000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Updated registration start.
    #[serde(default)]
    pub registration_start: Option<DateTime<Utc>>,

    /// Updated registration end.
    #[serde(default)]
    pub registration_end: Option<DateTime<Utc>>,

    /// Updated season start.
    #[serde(default)]
    pub season_start: Option<DateTime<Utc>>,

    /// Updated season end.
    #[serde(default)]
    pub season_end: Option<DateTime<Utc>>,

    /// Updated minimum team size.
    #[validate(range(min = 1, max = 50))]
    #[serde(default)]
    pub team_size_min: Option<i32>,

    /// Updated maximum team size.
    #[validate(range(min = 1, max = 50))]
    #[serde(default)]
    pub team_size_max: Option<i32>,

    /// Updated maximum substitutes.
    #[validate(range(min = 0, max = 20))]
    #[serde(default)]
    pub max_substitutes: Option<i32>,

    /// Updated maximum teams.
    #[validate(range(min = 2, max = 1000))]
    #[serde(default)]
    pub max_teams: Option<i32>,

    /// Updated status: draft, registration, active, paused, completed, cancelled.
    #[serde(default)]
    pub status: Option<String>,

    /// Updated roster lock: open, `soft_lock`, `hard_lock`.
    #[serde(default)]
    pub roster_lock_status: Option<String>,

    /// Updated settings (JSON).
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
}

impl TryFrom<UpdateLeagueSeasonRequest> for UpdateLeagueSeasonCommand {
    type Error = crate::error::ApiError;

    fn try_from(req: UpdateLeagueSeasonRequest) -> Result<Self, Self::Error> {
        let status = req
            .status
            .map(|s| {
                s.parse::<SeasonStatus>()
                    .map_err(|_| crate::error::ApiError::bad_request("Invalid season status"))
            })
            .transpose()?;

        let roster_lock_status = req
            .roster_lock_status
            .map(|s| {
                s.parse::<RosterLockStatus>()
                    .map_err(|_| crate::error::ApiError::bad_request("Invalid roster lock status"))
            })
            .transpose()?;

        Ok(Self {
            name: req.name,
            slug: req.slug,
            description: req.description,
            registration_start: req.registration_start,
            registration_end: req.registration_end,
            season_start: req.season_start,
            season_end: req.season_end,
            team_size_min: req.team_size_min,
            team_size_max: req.team_size_max,
            max_substitutes: req.max_substitutes,
            max_teams: req.max_teams,
            status,
            roster_lock_status,
            settings: req.settings,
        })
    }
}

// =============================================================================
// LEAGUE TEAM REQUESTS (Persistent Team Identity)
// =============================================================================

/// Request to create a new league team.
///
/// Teams have persistent identity at the league level. When created,
/// they are automatically registered for the specified season.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateLeagueTeamRequest {
    /// Team name.
    #[validate(length(min = 2, max = 50))]
    pub name: String,

    /// Team tag (short identifier, 2-5 chars).
    #[validate(length(min = 2, max = 5))]
    pub tag: String,

    /// Optional description.
    #[validate(length(max = 1000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Optional logo URL.
    #[validate(url)]
    #[serde(default)]
    pub logo_url: Option<String>,

    /// Primary team color (hex format).
    #[validate(length(min = 4, max = 7))]
    #[serde(default)]
    pub primary_color: Option<String>,

    /// Secondary team color (hex format).
    #[validate(length(min = 4, max = 7))]
    #[serde(default)]
    pub secondary_color: Option<String>,
}

impl CreateLeagueTeamRequest {
    /// Convert to command with the `league_id` and `season_id` from path parameters.
    pub fn into_command(
        self,
        league_id: LeagueId,
        season_id: LeagueSeasonId,
    ) -> CreateLeagueTeamCommand {
        CreateLeagueTeamCommand {
            league_id,
            season_id,
            name: self.name,
            tag: self.tag,
            description: self.description,
            logo_url: self.logo_url,
            primary_color: self.primary_color,
            secondary_color: self.secondary_color,
        }
    }
}

/// Request to update a league team's persistent identity.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateLeagueTeamRequest {
    /// Updated team name.
    #[validate(length(min = 2, max = 50))]
    #[serde(default)]
    pub name: Option<String>,

    /// Updated team tag.
    #[validate(length(min = 2, max = 5))]
    #[serde(default)]
    pub tag: Option<String>,

    /// Updated description.
    #[validate(length(max = 1000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Updated logo URL.
    #[validate(url)]
    #[serde(default)]
    pub logo_url: Option<String>,

    /// Updated banner URL.
    #[validate(url)]
    #[serde(default)]
    pub banner_url: Option<String>,

    /// Updated primary color.
    #[validate(length(min = 4, max = 7))]
    #[serde(default)]
    pub primary_color: Option<String>,

    /// Updated secondary color.
    #[validate(length(min = 4, max = 7))]
    #[serde(default)]
    pub secondary_color: Option<String>,
}

impl From<UpdateLeagueTeamRequest> for UpdateLeagueTeamCommand {
    fn from(req: UpdateLeagueTeamRequest) -> Self {
        Self {
            name: req.name,
            tag: req.tag,
            description: req.description,
            logo_url: req.logo_url,
            banner_url: req.banner_url,
            primary_color: req.primary_color,
            secondary_color: req.secondary_color,
        }
    }
}

/// Request to register an existing team for a new season.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterTeamForSeasonRequest {
    /// Team ID to register.
    pub team_id: String,
}

impl RegisterTeamForSeasonRequest {
    /// Convert to command with the `season_id` from path parameter.
    pub fn into_command(
        self,
        season_id: LeagueSeasonId,
    ) -> Result<RegisterTeamForSeasonCommand, crate::error::ApiError> {
        let team_id: LeagueTeamId = self
            .team_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid team ID format"))?;

        Ok(RegisterTeamForSeasonCommand { team_id, season_id })
    }
}

/// Request to transfer team ownership to another player.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct TransferOwnershipRequest {
    /// Player ID to transfer ownership to.
    pub new_owner_player_id: String,
}

impl TransferOwnershipRequest {
    /// Parse the new owner player ID.
    pub fn parse_new_owner(&self) -> Result<PlayerId, crate::error::ApiError> {
        self.new_owner_player_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid player ID format"))
    }
}

// =============================================================================
// LEAGUE TEAM MEMBER REQUESTS (Seasonal Roster)
// =============================================================================

/// Request to add a member to a team's seasonal roster.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AddLeagueTeamMemberRequest {
    /// Player ID to add.
    pub player_id: String,

    /// Role: captain, player, or substitute.
    #[serde(default = "default_member_role")]
    pub role: String,

    /// Optional position (e.g., "AWP", "Entry", "Support").
    #[validate(length(max = 50))]
    #[serde(default)]
    pub position: Option<String>,

    /// Optional jersey number.
    #[validate(range(min = 0, max = 99))]
    #[serde(default)]
    pub jersey_number: Option<i32>,
}

fn default_member_role() -> String {
    "player".to_string()
}

impl AddLeagueTeamMemberRequest {
    /// Convert to command with the `team_season_id` from path parameter.
    pub fn into_command(
        self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<AddLeagueTeamMemberCommand, crate::error::ApiError> {
        let player_id: PlayerId = self
            .player_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid player ID format"))?;

        let role: LeagueTeamRole = self
            .role
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid role"))?;

        Ok(AddLeagueTeamMemberCommand {
            team_season_id,
            player_id,
            role,
            position: self.position,
            jersey_number: self.jersey_number,
        })
    }
}

/// Request to update a team member's role.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateLeagueTeamMemberRequest {
    /// New role: captain, player, or substitute.
    pub role: String,

    /// Optional updated position.
    #[validate(length(max = 50))]
    #[serde(default)]
    pub position: Option<String>,

    /// Optional updated jersey number.
    #[validate(range(min = 0, max = 99))]
    #[serde(default)]
    pub jersey_number: Option<i32>,
}

// =============================================================================
// LEAGUE TEAM INVITATION REQUESTS
// =============================================================================

/// Request to invite a player to a team's seasonal roster.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct InviteToLeagueTeamRequest {
    /// Player ID to invite.
    pub player_id: String,

    /// Role for the player: player or substitute.
    #[serde(default = "default_invitation_role")]
    pub role: String,

    /// Optional message with the invitation.
    #[validate(length(max = 500))]
    #[serde(default)]
    pub message: Option<String>,
}

fn default_invitation_role() -> String {
    "player".to_string()
}

impl InviteToLeagueTeamRequest {
    /// Convert to command with the `team_season_id` from path parameter.
    pub fn into_command(
        self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<CreateLeagueTeamInvitationCommand, crate::error::ApiError> {
        let player_id: PlayerId = self
            .player_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid player ID format"))?;

        let role: LeagueTeamRole = self
            .role
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid role"))?;

        Ok(CreateLeagueTeamInvitationCommand {
            team_season_id,
            player_id,
            invitation_type: LeagueTeamInvitationType::Invite,
            role,
            message: self.message,
        })
    }
}

/// Request to apply to join a team's seasonal roster.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ApplyToLeagueTeamRequest {
    /// Desired role: player or substitute.
    #[serde(default = "default_invitation_role")]
    pub role: String,

    /// Optional message with the application.
    #[validate(length(max = 500))]
    #[serde(default)]
    pub message: Option<String>,
}

impl ApplyToLeagueTeamRequest {
    /// Convert to command with the `team_season_id` and `player_id`.
    pub fn into_command(
        self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<CreateLeagueTeamInvitationCommand, crate::error::ApiError> {
        let role: LeagueTeamRole = self
            .role
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid role"))?;

        Ok(CreateLeagueTeamInvitationCommand {
            team_season_id,
            player_id,
            invitation_type: LeagueTeamInvitationType::Request,
            role,
            message: self.message,
        })
    }
}

/// Request to respond to an invitation.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RespondToInvitationRequest {
    /// Response message (optional).
    #[validate(length(max = 500))]
    #[serde(default)]
    pub message: Option<String>,
}

// =============================================================================
// LEAGUE SEASON PARTICIPANT REQUESTS (Individual Format)
// =============================================================================

/// Request to register as a participant in an individual format season.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterParticipantRequest {
    // No fields needed - player ID comes from auth context
}

/// Request to withdraw from an individual format season.
#[derive(Debug, Deserialize, ToSchema)]
pub struct WithdrawParticipantRequest {
    // No fields needed - participant ID from path
}
