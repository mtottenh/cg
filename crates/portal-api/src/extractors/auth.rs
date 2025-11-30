//! Authentication extractor.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use portal_core::{PlayerId, UserId};
use portal_domain::validate_token;
use std::sync::Arc;
use uuid::Uuid;

/// Dev token for local development.
/// In production, this would be disabled and real JWT verification used.
const DEV_TOKEN: &str = "dev-token";

/// Well-known dev user IDs (must exist in database via seed).
const DEV_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
const DEV_PLAYER_ID: &str = "00000000-0000-0000-0000-000000000001";
const DEV_USERNAME: &str = "devuser";

/// JWT secret wrapper for `FromRef` extraction.
#[derive(Clone)]
pub struct JwtSecret(pub Arc<str>);

impl axum::extract::FromRef<AppState> for JwtSecret {
    fn from_ref(state: &AppState) -> Self {
        Self(state.jwt_secret.clone())
    }
}

/// Authenticated user extracted from JWT token.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// User ID from the token.
    pub user_id: UserId,
    /// Player ID from the token.
    pub player_id: PlayerId,
    /// Username.
    pub username: String,
}

impl AuthenticatedUser {
    /// Extract the Bearer token from headers.
    fn extract_token(headers: &HeaderMap) -> Option<&str> {
        headers
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
    }

    /// Check if dev auth mode is enabled.
    /// With the `test-utils` feature, dev auth is always enabled.
    #[cfg(feature = "test-utils")]
    pub fn is_dev_auth_enabled() -> bool {
        true
    }

    /// Check if dev auth mode is enabled.
    /// Without `test-utils` feature, requires `DEV_AUTH_ENABLED` environment variable.
    #[cfg(not(feature = "test-utils"))]
    pub fn is_dev_auth_enabled() -> bool {
        std::env::var("DEV_AUTH_ENABLED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }

    /// Create a dev user for testing.
    fn dev_user() -> Self {
        Self {
            user_id: UserId::from(Uuid::parse_str(DEV_USER_ID).expect("valid dev user id")),
            player_id: PlayerId::from(Uuid::parse_str(DEV_PLAYER_ID).expect("valid dev player id")),
            username: DEV_USERNAME.to_string(),
        }
    }

    /// Validate a JWT token and return the authenticated user.
    fn from_jwt(token: &str, jwt_secret: &str) -> Result<Self, ApiError> {
        let claims = validate_token(token, jwt_secret).map_err(|e| {
            tracing::debug!("JWT validation failed: {:?}", e);
            match e {
                portal_core::DomainError::TokenExpired => {
                    ApiError::unauthorized("Token has expired")
                }
                portal_core::DomainError::InvalidToken => {
                    ApiError::unauthorized("Invalid token")
                }
                _ => ApiError::unauthorized("Authentication failed"),
            }
        })?;

        let user_id = claims.user_id().map_err(|_| {
            ApiError::unauthorized("Invalid user ID in token")
        })?;

        Ok(Self {
            user_id: UserId::from(user_id),
            player_id: PlayerId::from(claims.player_id),
            username: claims.username,
        })
    }
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    JwtSecret: axum::extract::FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract token
        let token = Self::extract_token(&parts.headers)
            .ok_or_else(|| ApiError::unauthorized("Missing or invalid authorization header"))?;

        // Dev mode: accept dev-token for local development
        if Self::is_dev_auth_enabled() && token == DEV_TOKEN {
            return Ok(Self::dev_user());
        }

        // Get JWT secret from state
        let JwtSecret(jwt_secret) = JwtSecret::from_ref(state);

        // Validate the JWT token
        Self::from_jwt(token, &jwt_secret)
    }
}

/// Optional authenticated user - doesn't fail if not authenticated.
#[derive(Debug, Clone)]
pub struct OptionalAuthenticatedUser(pub Option<AuthenticatedUser>);

impl<S> FromRequestParts<S> for OptionalAuthenticatedUser
where
    S: Send + Sync,
    JwtSecret: axum::extract::FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Try to extract user, return None if not authenticated
        let user = AuthenticatedUser::from_request_parts(parts, state).await.ok();
        Ok(Self(user))
    }
}
