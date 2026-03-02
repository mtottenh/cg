//! API key repository adapter.

use crate::entities::ApiKeyRow;
use crate::DbPool;
use async_trait::async_trait;
use portal_core::{ApiKeyId, DomainError, UserId};
use portal_domain::entities::api_key::ApiKey;
use portal_domain::repositories::api_key::{ApiKeyRepository, CreateApiKey};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<ApiKeyRow> for ApiKey {
    fn from(row: ApiKeyRow) -> Self {
        Self {
            id: ApiKeyId::from(row.id),
            service_name: row.service_name,
            key_hash: row.key_hash,
            key_prefix: row.key_prefix,
            permissions: row.permissions,
            is_active: row.is_active,
            expires_at: row.expires_at,
            last_used_at: row.last_used_at,
            created_by: row.created_by.map(UserId::from),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// API Key Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `ApiKeyRepository` trait.
#[derive(Clone)]
pub struct PgApiKeyRepository {
    pool: DbPool,
}

impl PgApiKeyRepository {
    /// Create a new PostgreSQL API key repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyRepository for PgApiKeyRepository {
    async fn find_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DomainError> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT * FROM api_keys WHERE key_hash = $1",
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(ApiKey::from))
    }

    async fn find_by_id(&self, id: ApiKeyId) -> Result<Option<ApiKey>, DomainError> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT * FROM api_keys WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(ApiKey::from))
    }

    async fn create(&self, cmd: CreateApiKey) -> Result<ApiKey, DomainError> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            r"
            INSERT INTO api_keys (service_name, key_hash, key_prefix, permissions)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            ",
        )
        .bind(&cmd.service_name)
        .bind(&cmd.key_hash)
        .bind(&cmd.key_prefix)
        .bind(&cmd.permissions)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(ApiKey::from(row))
    }

    async fn touch(&self, id: ApiKeyId) -> Result<(), DomainError> {
        sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn deactivate(&self, id: ApiKeyId) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE api_keys SET is_active = FALSE, updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
