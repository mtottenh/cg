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
    async fn find_active_by_hash(&self, token_hash: &str) -> Result<Option<RefreshToken>, DomainError>;

    /// Revoke a specific refresh token by ID.
    async fn revoke(&self, id: Uuid) -> Result<(), DomainError>;

    /// Revoke all refresh tokens for a user (e.g., on password change or logout-all).
    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError>;
}
