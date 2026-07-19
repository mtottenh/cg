//! Authentication request DTOs.

use regex::Regex;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Regex for validating usernames (alphanumeric with underscores).
static USERNAME_REGEX: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]*$").unwrap());

/// Validate a username.
fn validate_username(username: &str) -> Result<(), validator::ValidationError> {
    if USERNAME_REGEX.is_match(username) {
        Ok(())
    } else {
        Err(validator::ValidationError::new("username_format"))
    }
}

/// Request body for user registration.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterRequest {
    /// Username (3-32 characters, alphanumeric with underscores, must start with letter).
    #[validate(length(min = 3, max = 32, message = "Username must be 3-32 characters"))]
    #[validate(custom(function = "validate_username"))]
    #[schema(example = "john_doe")]
    pub username: String,

    /// Email address.
    #[validate(email(message = "Invalid email address"))]
    #[schema(example = "john@example.com")]
    pub email: String,

    /// Password (8-128 characters).
    #[validate(length(min = 8, max = 128, message = "Password must be 8-128 characters"))]
    pub password: String,

    /// Display name for the player profile.
    #[validate(length(min = 2, max = 32, message = "Display name must be 2-32 characters"))]
    #[schema(example = "John Doe")]
    pub display_name: String,
}

/// Request body for refreshing an access token.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RefreshTokenRequest {
    /// The refresh token issued during login or previous refresh.
    #[validate(length(min = 1, message = "Refresh token is required"))]
    pub refresh_token: String,
}

/// Request body for user login.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LoginRequest {
    /// Username or email address.
    #[validate(length(min = 3, max = 254, message = "Username or email required"))]
    #[schema(example = "john_doe")]
    pub username_or_email: String,

    /// Password.
    #[validate(length(min = 1, max = 128, message = "Password required"))]
    pub password: String,
}
