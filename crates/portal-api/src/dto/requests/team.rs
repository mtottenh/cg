//! Team request DTOs.

use once_cell::sync::Lazy;
use portal_core::types::TeamRole;
use portal_core::validation::{TeamName, TeamTag};
use portal_core::ValidationError;
use portal_domain::entities::team::{CreateTeamCommand, UpdateMemberRoleCommand, UpdateTeamCommand};
use regex::Regex;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Regex for validating hex color codes.
static HEX_COLOR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^#[0-9A-Fa-f]{6}$").unwrap());

/// Validate a hex color string.
fn validate_hex_color(color: &str) -> Result<(), validator::ValidationError> {
    if HEX_COLOR_REGEX.is_match(color) {
        Ok(())
    } else {
        Err(validator::ValidationError::new("hex_color"))
    }
}

/// Request to create a new team.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct CreateTeamRequest {
    /// Team display name.
    #[validate(length(min = 3, max = 64))]
    #[schema(example = "Cloud9", min_length = 3, max_length = 64)]
    pub name: String,

    /// Short team tag.
    #[validate(length(min = 2, max = 5))]
    #[schema(example = "C9", min_length = 2, max_length = 5)]
    pub tag: String,

    /// Optional team description.
    #[validate(length(max = 2000))]
    #[schema(example = "Professional esports organization")]
    pub description: Option<String>,

    /// Optional logo URL.
    #[validate(url)]
    #[schema(example = "https://example.com/logo.png")]
    pub logo_url: Option<String>,

    /// Optional game ID (for game-specific teams).
    #[schema(example = "cs2")]
    pub game_id: Option<String>,
}

impl TryFrom<CreateTeamRequest> for CreateTeamCommand {
    type Error = ValidationError;

    fn try_from(req: CreateTeamRequest) -> Result<Self, Self::Error> {
        // Validate name and tag format
        let name = TeamName::new(&req.name)?;
        let tag = TeamTag::new(&req.tag)?;

        Ok(CreateTeamCommand {
            name: name.into_inner(),
            tag: tag.into_inner(),
            description: req.description,
            logo_url: req.logo_url,
            game_id: req.game_id,
        })
    }
}

/// Request to update a team.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UpdateTeamRequest {
    /// New team name.
    #[validate(length(min = 3, max = 64))]
    #[schema(example = "Cloud9 Blue")]
    pub name: Option<String>,

    /// New team tag.
    #[validate(length(min = 2, max = 5))]
    #[schema(example = "C9B")]
    pub tag: Option<String>,

    /// New description.
    #[validate(length(max = 2000))]
    pub description: Option<String>,

    /// New logo URL.
    #[validate(url)]
    pub logo_url: Option<String>,

    /// New banner URL.
    #[validate(url)]
    pub banner_url: Option<String>,

    /// Primary team color (hex).
    #[validate(custom(function = "validate_hex_color"))]
    #[schema(example = "#1E90FF")]
    pub primary_color: Option<String>,

    /// Secondary team color (hex).
    #[validate(custom(function = "validate_hex_color"))]
    #[schema(example = "#FFFFFF")]
    pub secondary_color: Option<String>,

    /// Team website URL.
    #[validate(url)]
    pub website_url: Option<String>,
}

impl TryFrom<UpdateTeamRequest> for UpdateTeamCommand {
    type Error = ValidationError;

    fn try_from(req: UpdateTeamRequest) -> Result<Self, Self::Error> {
        // Validate name if provided
        if let Some(ref name) = req.name {
            TeamName::new(name)?;
        }

        // Validate tag if provided
        if let Some(ref tag) = req.tag {
            TeamTag::new(tag)?;
        }

        Ok(UpdateTeamCommand {
            name: req.name,
            tag: req.tag,
            description: req.description,
            logo_url: req.logo_url,
            banner_url: req.banner_url,
            primary_color: req.primary_color,
            secondary_color: req.secondary_color,
            website_url: req.website_url,
        })
    }
}

/// Request to update a member's role.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateMemberRoleRequest {
    /// The new role for the member.
    #[schema(example = "officer")]
    pub role: String,
}

impl UpdateMemberRoleRequest {
    /// Convert to domain command.
    ///
    /// # Errors
    /// Returns error if the role is invalid.
    pub fn into_command(
        self,
        player_id: portal_core::PlayerId,
    ) -> Result<UpdateMemberRoleCommand, ValidationError> {
        let role: TeamRole = self.role.parse().map_err(|_| {
            ValidationError::field(portal_core::errors::FieldError::format(
                "role",
                "a valid team role (captain, officer, player, substitute, coach, manager)",
            ))
        })?;

        Ok(UpdateMemberRoleCommand {
            player_id,
            new_role: role,
        })
    }
}
