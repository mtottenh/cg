//! JWT token generation and validation.

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use portal_core::DomainError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    /// Whether the user has admin privileges.
    #[serde(default)]
    pub is_admin: bool,

    /// Expiration time (Unix timestamp).
    pub exp: i64,

    /// Issued at (Unix timestamp).
    pub iat: i64,
}

impl Claims {
    /// Create new claims for a user.
    #[must_use]
    pub fn new(user_id: Uuid, player_id: Uuid, username: String, expiry_minutes: i64) -> Self {
        Self::with_admin(user_id, player_id, username, expiry_minutes, false)
    }

    /// Create new claims for a user with admin flag.
    #[must_use]
    pub fn with_admin(
        user_id: Uuid,
        player_id: Uuid,
        username: String,
        expiry_minutes: i64,
        is_admin: bool,
    ) -> Self {
        let now = Utc::now();
        let exp = now + Duration::minutes(expiry_minutes);

        Self {
            sub: user_id.to_string(),
            player_id,
            username,
            is_admin,
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

/// Generate an access token for a user.
///
/// # Arguments
/// * `user_id` - The user's unique identifier
/// * `player_id` - The player's unique identifier (for game operations)
/// * `username` - The user's username for display
/// * `secret` - The JWT secret key
///
/// # Returns
/// The encoded JWT token string
pub fn generate_access_token(
    user_id: Uuid,
    player_id: Uuid,
    username: &str,
    secret: &str,
) -> Result<String, DomainError> {
    generate_access_token_with_admin(user_id, player_id, username, secret, false)
}

/// Generate an access token for a user with admin flag.
///
/// # Arguments
/// * `user_id` - The user's unique identifier
/// * `player_id` - The player's unique identifier (for game operations)
/// * `username` - The user's username for display
/// * `secret` - The JWT secret key
/// * `is_admin` - Whether the user has admin privileges
///
/// # Returns
/// The encoded JWT token string
pub fn generate_access_token_with_admin(
    user_id: Uuid,
    player_id: Uuid,
    username: &str,
    secret: &str,
    is_admin: bool,
) -> Result<String, DomainError> {
    let claims = Claims::with_admin(
        user_id,
        player_id,
        username.to_string(),
        ACCESS_TOKEN_EXPIRY_MINUTES,
        is_admin,
    );

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| DomainError::Internal(format!("Failed to generate token: {e}")))
}

/// Generate an access token with custom expiry.
///
/// # Arguments
/// * `user_id` - The user's unique identifier
/// * `player_id` - The player's unique identifier
/// * `username` - The user's username
/// * `secret` - The JWT secret key
/// * `expiry_minutes` - Custom expiry time in minutes
///
/// # Returns
/// The encoded JWT token string
pub fn generate_access_token_with_expiry(
    user_id: Uuid,
    player_id: Uuid,
    username: &str,
    secret: &str,
    expiry_minutes: i64,
) -> Result<String, DomainError> {
    let claims = Claims::new(user_id, player_id, username.to_string(), expiry_minutes);

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| DomainError::Internal(format!("Failed to generate token: {e}")))
}

/// Generate an access token with admin flag and custom expiry.
pub fn generate_access_token_with_admin_and_expiry(
    user_id: Uuid,
    player_id: Uuid,
    username: &str,
    secret: &str,
    is_admin: bool,
    expiry_minutes: i64,
) -> Result<String, DomainError> {
    let claims = Claims::with_admin(
        user_id,
        player_id,
        username.to_string(),
        expiry_minutes,
        is_admin,
    );

    encode(
        &Header::default(),
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
    let validation = Validation::default();

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
