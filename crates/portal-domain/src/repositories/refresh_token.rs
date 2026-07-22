//! Refresh token repository trait.

use crate::entities::refresh_token::RefreshToken;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::DomainError;
use uuid::Uuid;

/// Repository trait for refresh token operations.
#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    /// Create a new refresh token.
    async fn create(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<RefreshToken, DomainError>;

    /// Find an active (non-revoked) refresh token by its hash.
    async fn find_active_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, DomainError>;

    /// Find a refresh token by hash **regardless of revoked state**.
    ///
    /// Required for replay detection: if a caller presents a token hash that
    /// matches a revoked row, the most likely cause is token theft — the
    /// legitimate client already rotated and the attacker is trying the old
    /// token. Callers should respond by revoking every token for that user.
    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<RefreshToken>, DomainError>;

    /// Revoke a specific refresh token by ID. Idempotent.
    async fn revoke(&self, id: Uuid) -> Result<(), DomainError>;

    /// Atomically revoke a token **only if it is currently active**.
    ///
    /// Returns `true` if this call performed the revoke, `false` if the token
    /// was already revoked (or didn't exist). Used to make the
    /// find-check-revoke sequence in the refresh handler race-safe: two
    /// concurrent refreshes with the same token will see at most one `true`.
    async fn try_revoke(&self, id: Uuid) -> Result<bool, DomainError>;

    /// Revoke all refresh tokens for a user (e.g., on password change, on
    /// logout-all, or on detected replay).
    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError>;
}
