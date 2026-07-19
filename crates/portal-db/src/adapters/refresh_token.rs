//! Refresh token repository adapter.

use crate::DbPool;
use crate::entities::RefreshTokenRow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::DomainError;
use portal_domain::entities::refresh_token::RefreshToken;
use portal_domain::repositories::refresh_token::RefreshTokenRepository;
use uuid::Uuid;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<RefreshTokenRow> for RefreshToken {
    fn from(row: RefreshTokenRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            token_hash: row.token_hash,
            expires_at: row.expires_at,
            created_at: row.created_at,
            revoked_at: row.revoked_at,
        }
    }
}

// =============================================================================
// Refresh Token Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `RefreshTokenRepository` trait.
#[derive(Clone)]
pub struct PgRefreshTokenRepository {
    pool: DbPool,
}

impl PgRefreshTokenRepository {
    /// Create a new PostgreSQL refresh token repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RefreshTokenRepository for PgRefreshTokenRepository {
    async fn create(
        &self,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<RefreshToken, DomainError> {
        let row = sqlx::query_as::<_, RefreshTokenRow>(
            r"
            INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(RefreshToken::from(row))
    }

    async fn find_active_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, DomainError> {
        let row = sqlx::query_as::<_, RefreshTokenRow>(
            "SELECT * FROM refresh_tokens WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(RefreshToken::from))
    }

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<RefreshToken>, DomainError> {
        let row = sqlx::query_as::<_, RefreshTokenRow>(
            "SELECT * FROM refresh_tokens WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(RefreshToken::from))
    }

    async fn revoke(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn try_revoke(&self, id: Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
