//! Ban request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

/// Request body for creating a new ban.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateBanRequest {
    /// The user ID to ban.
    #[schema(example = "01234567-89ab-cdef-0123-456789abcdef")]
    pub user_id: Uuid,

    /// Type of ban (platform, matchmaking, chat, league, tournament).
    #[validate(custom(function = "validate_ban_type"))]
    #[schema(example = "platform")]
    pub ban_type: String,

    /// Reason for the ban.
    #[validate(length(min = 5, max = 2000, message = "Reason must be 5-2000 characters"))]
    #[schema(example = "Cheating violation detected")]
    pub reason: String,

    /// Optional scope type for context-specific bans (e.g., "league", "tournament").
    #[schema(example = "league")]
    pub scope_type: Option<String>,

    /// Optional scope ID for context-specific bans.
    #[schema(example = "01234567-89ab-cdef-0123-456789abcdef")]
    pub scope_id: Option<Uuid>,

    /// Duration in seconds (null for permanent ban).
    /// Common values: 3600 (1 hour), 86400 (1 day), 604800 (1 week), 2592000 (30 days).
    #[schema(example = 604800)]
    pub duration_seconds: Option<i64>,
}

/// Request body for lifting a ban.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LiftBanRequest {
    /// Reason for lifting the ban.
    #[validate(length(max = 1000, message = "Reason must be at most 1000 characters"))]
    #[schema(example = "Ban appealed and approved")]
    pub reason: Option<String>,
}

/// Query parameters for listing bans.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListBansQuery {
    /// Filter by user ID.
    pub user_id: Option<Uuid>,

    /// Filter by ban type.
    pub ban_type: Option<String>,

    /// Filter by scope type.
    pub scope_type: Option<String>,

    /// Filter by scope ID.
    pub scope_id: Option<Uuid>,

    /// Only show active bans.
    #[serde(default)]
    pub active_only: bool,

    /// Page number (1-indexed).
    #[serde(default = "default_page")]
    pub page: i64,

    /// Items per page.
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

const fn default_page() -> i64 {
    1
}

const fn default_per_page() -> i64 {
    20
}

/// Validate ban type.
fn validate_ban_type(ban_type: &str) -> Result<(), validator::ValidationError> {
    match ban_type {
        "platform" | "matchmaking" | "chat" | "league" | "tournament" => Ok(()),
        _ => Err(validator::ValidationError::new("invalid_ban_type")),
    }
}
