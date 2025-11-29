//! User response DTOs.

use portal_domain::entities::User;
use serde::Serialize;
use utoipa::ToSchema;

/// User response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserResponse {
    /// Unique user identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Username (unique).
    #[schema(example = "johndoe")]
    pub username: String,

    /// Email address.
    #[schema(example = "john@example.com")]
    pub email: String,

    /// Whether the email has been verified.
    pub email_verified: bool,

    /// Account status.
    #[schema(example = "active")]
    pub status: String,

    /// User's preferred locale.
    #[schema(example = "en-US")]
    pub locale: String,

    /// User's timezone.
    #[schema(example = "America/New_York")]
    pub timezone: String,

    /// Whether two-factor authentication is enabled.
    pub two_factor_enabled: bool,

    /// When the account was created.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,

    /// When the account was last updated.
    pub updated_at: String,

    /// Last login timestamp.
    pub last_login_at: Option<String>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username,
            email: user.email,
            email_verified: user.email_verified,
            status: user.status.to_string(),
            locale: user.locale,
            timezone: user.timezone,
            two_factor_enabled: user.two_factor_enabled,
            created_at: user.created_at.to_rfc3339(),
            updated_at: user.updated_at.to_rfc3339(),
            last_login_at: user.last_login_at.map(|t| t.to_rfc3339()),
        }
    }
}
