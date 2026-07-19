//! JWT token generation and validation.
//!
//! # Claims philosophy
//!
//! The access token carries the minimum needed to identify the caller
//! (`sub`, `player_id`, `username`) plus the standard timestamps. We
//! deliberately do **not** embed role information or an `is_admin` flag in
//! the claims.
//!
//! Previously there was an `is_admin: bool` claim populated at token-issue
//! time from a DB lookup. Nothing in the request-handling path read it —
//! every admin check already flows through `PermissionChecker` which
//! consults the live RBAC tables — so the claim was both dead weight *and*
//! a soft-staleness window: a user demoted mid-session would still carry
//! `is_admin: true` until their 15-min access token expired. Removing the
//! claim makes the DB the single source of truth for authz on every
//! request.

use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use portal_core::DomainError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The signing algorithm we accept and emit. Pinned explicitly so a future
/// `jsonwebtoken` default change, or a crafted header specifying a different
/// algorithm, can never cause validation to silently fall through.
const JWT_ALGORITHM: Algorithm = Algorithm::HS256;

/// Default access token expiry in minutes.
pub const ACCESS_TOKEN_EXPIRY_MINUTES: i64 = 15;

/// JWT claims structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject - the user ID.
    pub sub: String,

    /// Player ID for game operations.
    pub player_id: Uuid,

    /// Username for display/logging.
    pub username: String,

    /// Expiration time (Unix timestamp).
    pub exp: i64,

    /// Issued at (Unix timestamp).
    pub iat: i64,
}

impl Claims {
    /// Create new claims for a user.
    #[must_use]
    pub fn new(user_id: Uuid, player_id: Uuid, username: String, expiry_minutes: i64) -> Self {
        let now = Utc::now();
        let exp = now + Duration::minutes(expiry_minutes);

        Self {
            sub: user_id.to_string(),
            player_id,
            username,
            exp: exp.timestamp(),
            iat: now.timestamp(),
        }
    }

    /// Get the user ID from claims.
    pub fn user_id(&self) -> Result<Uuid, DomainError> {
        Uuid::parse_str(&self.sub)
            .map_err(|e| DomainError::Internal(format!("Invalid user ID in claims: {e}")))
    }
}

/// Generate an access token for a user with the default expiry.
pub fn generate_access_token(
    user_id: Uuid,
    player_id: Uuid,
    username: &str,
    secret: &str,
) -> Result<String, DomainError> {
    generate_access_token_with_expiry(
        user_id,
        player_id,
        username,
        secret,
        ACCESS_TOKEN_EXPIRY_MINUTES,
    )
}

/// Generate an access token with a custom expiry.
pub fn generate_access_token_with_expiry(
    user_id: Uuid,
    player_id: Uuid,
    username: &str,
    secret: &str,
    expiry_minutes: i64,
) -> Result<String, DomainError> {
    let claims = Claims::new(user_id, player_id, username.to_string(), expiry_minutes);

    encode(
        &Header::new(JWT_ALGORITHM),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| DomainError::Internal(format!("Failed to generate token: {e}")))
}

/// Validate a JWT token and extract claims.
///
/// # Arguments
/// * `token` - The JWT token string
/// * `secret` - The JWT secret key
///
/// # Returns
/// The decoded claims if valid
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, DomainError> {
    // Pin the accepted algorithm explicitly. `Validation::new` constrains
    // `algorithms` to a single entry, so a token with `alg: none` or a
    // different symmetric/asymmetric algorithm will be rejected outright.
    let validation = Validation::new(JWT_ALGORITHM);

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => DomainError::TokenExpired,
        jsonwebtoken::errors::ErrorKind::InvalidToken
        | jsonwebtoken::errors::ErrorKind::InvalidSignature => DomainError::InvalidToken,
        _ => DomainError::InvalidToken,
    })?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-key-for-testing-only";

    #[test]
    fn test_generate_and_validate_token() {
        let user_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let username = "testuser";

        let token = generate_access_token(user_id, player_id, username, TEST_SECRET).unwrap();

        assert!(!token.is_empty());

        let claims = validate_token(&token, TEST_SECRET).unwrap();

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.player_id, player_id);
        assert_eq!(claims.username, username);
    }

    #[test]
    fn test_invalid_token() {
        let result = validate_token("invalid-token", TEST_SECRET);
        assert!(matches!(result, Err(DomainError::InvalidToken)));
    }

    #[test]
    fn test_wrong_secret() {
        let user_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();

        let token = generate_access_token(user_id, player_id, "user", TEST_SECRET).unwrap();

        let result = validate_token(&token, "wrong-secret");
        assert!(matches!(result, Err(DomainError::InvalidToken)));
    }

    #[test]
    fn test_expired_token() {
        let user_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();

        // Generate token with -5 minute expiry (well past jsonwebtoken's 60-second leeway)
        let token =
            generate_access_token_with_expiry(user_id, player_id, "user", TEST_SECRET, -5).unwrap();

        let result = validate_token(&token, TEST_SECRET);
        assert!(matches!(result, Err(DomainError::TokenExpired)));
    }

    #[test]
    fn test_claims_user_id() {
        let user_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();

        let claims = Claims::new(user_id, player_id, "test".to_string(), 15);

        assert_eq!(claims.user_id().unwrap(), user_id);
    }
}
