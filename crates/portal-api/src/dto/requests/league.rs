//! League request DTOs.

use portal_core::GameId;
use portal_domain::entities::league::{CreateLeagueCommand, LeagueAccessType, UpdateLeagueCommand};
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request to create a new league.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateLeagueRequest {
    /// Game ID (UUID) this league is for.
    pub game_id: String,

    /// League name.
    #[validate(length(min = 3, max = 100))]
    pub name: String,

    /// URL-friendly slug.
    #[validate(length(min = 3, max = 100), custom(function = "validate_slug"))]
    pub slug: String,

    /// Optional description.
    #[validate(length(max = 2000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Optional logo URL.
    #[validate(url)]
    #[serde(default)]
    pub logo_url: Option<String>,

    /// Access type: open, invite_only, or application.
    #[serde(default = "default_access_type")]
    pub access_type: String,
}

fn default_access_type() -> String {
    "open".to_string()
}

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

impl TryFrom<CreateLeagueRequest> for CreateLeagueCommand {
    type Error = crate::error::ApiError;

    fn try_from(req: CreateLeagueRequest) -> Result<Self, Self::Error> {
        let access_type = LeagueAccessType::from_str(&req.access_type)
            .ok_or_else(|| crate::error::ApiError::bad_request("Invalid access type"))?;

        // Parse game_id as UUID
        let game_id: GameId = req
            .game_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid game ID format"))?;

        Ok(CreateLeagueCommand {
            game_id,
            name: req.name,
            slug: req.slug,
            description: req.description,
            logo_url: req.logo_url,
            access_type,
        })
    }
}

/// Request to update a league.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateLeagueRequest {
    /// Updated league name.
    #[validate(length(min = 3, max = 100))]
    #[serde(default)]
    pub name: Option<String>,

    /// Updated slug.
    #[validate(length(min = 3, max = 100))]
    #[serde(default)]
    pub slug: Option<String>,

    /// Updated description.
    #[validate(length(max = 2000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Updated logo URL.
    #[validate(url)]
    #[serde(default)]
    pub logo_url: Option<String>,

    /// Updated access type.
    #[serde(default)]
    pub access_type: Option<String>,

    /// Updated settings.
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
}

impl TryFrom<UpdateLeagueRequest> for UpdateLeagueCommand {
    type Error = crate::error::ApiError;

    fn try_from(req: UpdateLeagueRequest) -> Result<Self, Self::Error> {
        let access_type = req
            .access_type
            .map(|s| {
                LeagueAccessType::from_str(&s)
                    .ok_or_else(|| crate::error::ApiError::bad_request("Invalid access type"))
            })
            .transpose()?;

        Ok(UpdateLeagueCommand {
            name: req.name,
            slug: req.slug,
            description: req.description,
            logo_url: req.logo_url,
            access_type,
            status: None,
            settings: req.settings,
        })
    }
}

/// Request to invite a user to a league.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct InviteToLeagueRequest {
    /// User ID to invite.
    pub user_id: String,

    /// Optional message with the invitation.
    #[validate(length(max = 500))]
    #[serde(default)]
    pub message: Option<String>,
}

/// Request to apply to join a league.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ApplyToLeagueRequest {
    /// Optional application message.
    #[validate(length(max = 500))]
    #[serde(default)]
    pub message: Option<String>,
}

/// Request to update a member's role.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateLeagueMemberRoleRequest {
    /// New membership type: admin, moderator, or member.
    pub membership_type: String,
}
