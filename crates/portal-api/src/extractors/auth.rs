//! Authentication extractor.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use portal_core::{PlayerId, UserId};
use portal_domain::validate_token;
use std::sync::Arc;
#[cfg(feature = "test-utils")]
use uuid::Uuid;

/// Bearer token accepted as the well-known dev user under `test-utils` builds.
///
/// This constant — together with [`AuthenticatedUser::dev_user`] and the
/// dev-token branch in [`AuthenticatedUser::from_request_parts`] — only exists
/// when the `test-utils` cargo feature is enabled. Production builds do not
/// see this code at all.
#[cfg(feature = "test-utils")]
const DEV_TOKEN: &str = "dev-token";

/// Well-known dev user IDs (must exist in database via seed).
#[cfg(feature = "test-utils")]
const DEV_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
#[cfg(feature = "test-utils")]
const DEV_PLAYER_ID: &str = "00000000-0000-0000-0000-000000000001";
/// The well-known dev username. Permission checks treat this name as having
/// every permission **only under the `test-utils` cargo feature**, where the
/// `is_dev_user` helper compiles to a real check; in production builds the
/// helper is `const false` and registering a real account named "devuser"
/// grants no special privileges.
#[cfg(feature = "test-utils")]
pub(crate) const DEV_USERNAME: &str = "devuser";

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

    /// Create the well-known dev user for tests. Only compiled under `test-utils`.
    #[cfg(feature = "test-utils")]
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

        // Dev mode: accept dev-token for local development.
        // This branch is removed entirely from production builds.
        #[cfg(feature = "test-utils")]
        if token == DEV_TOKEN {
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
