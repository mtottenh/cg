//! Authentication response DTOs.

use crate::dto::responses::{PlayerResponse, UserResponse};
use portal_domain::entities::{Player, User};
use serde::Serialize;
use utoipa::ToSchema;

/// Response for successful user registration.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RegisterResponse {
    /// JWT access token for immediate use.
    pub access_token: String,
    /// Refresh token for silent renewal.
    pub refresh_token: String,
    /// The created user.
    pub user: UserResponse,
    /// The created player profile.
    pub player: PlayerResponse,
}

impl RegisterResponse {
    /// Create a new registration response.
    pub fn new(user: User, player: Player, access_token: String, refresh_token: String) -> Self {
        Self {
            access_token,
            refresh_token,
            user: UserResponse::from(user),
            player: PlayerResponse::from(player),
        }
    }
}

/// Response for successful login.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LoginResponse {
    /// JWT access token.
    pub access_token: String,
    /// Refresh token for silent renewal.
    pub refresh_token: String,
    /// User ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub user_id: String,
    /// Player ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub player_id: String,
    /// Username.
    #[schema(example = "john_doe")]
    pub username: String,
}

/// Response for logout / logout-all.
///
/// Deliberately carries no detail about *which* tokens existed — logout is
/// idempotent and must not become an oracle for guessing valid tokens.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LogoutResponse {
    /// Whether the session(s) are now revoked. Always `true` on success.
    #[schema(example = true)]
    pub logged_out: bool,
}
