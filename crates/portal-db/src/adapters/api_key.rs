//! API key repository adapter.

use crate::DbPool;
use crate::entities::ApiKeyRow;
use async_trait::async_trait;
use portal_core::{ApiKeyId, DomainError, UserId};
use portal_domain::entities::api_key::ApiKey;
use portal_domain::repositories::api_key::{ApiKeyRepository, CreateApiKey};

// =============================================================================
// Helpers
// =============================================================================

/// Fetch the permission names linked to an API key via the join table.
async fn get_permissions_for_key(
    pool: &DbPool,
    api_key_id: uuid::Uuid,
) -> Result<Vec<String>, DomainError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r"
        SELECT p.name
        FROM api_key_permissions akp
        JOIN permissions p ON p.id = akp.permission_id
        WHERE akp.api_key_id = $1
        ",
    )
    .bind(api_key_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

/// Convert an `ApiKeyRow` + resolved permissions into an `ApiKey` domain entity.
fn row_to_api_key(row: ApiKeyRow, permissions: Vec<String>) -> ApiKey {
    ApiKey::with_permissions(
        ApiKeyId::from(row.id),
        row.service_name,
        row.key_hash,
        row.key_prefix,
        row.is_active,
        row.expires_at,
        row.last_used_at,
        row.created_by.map(UserId::from),
        row.created_at,
        row.updated_at,
        permissions,
    )
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
        let row = sqlx::query_as::<_, ApiKeyRow>("SELECT * FROM api_keys WHERE key_hash = $1")
            .bind(key_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        match row {
            Some(r) => {
                let perms = get_permissions_for_key(&self.pool, r.id).await?;
                Ok(Some(row_to_api_key(r, perms)))
            }
            None => Ok(None),
        }
    }

    async fn find_by_id(&self, id: ApiKeyId) -> Result<Option<ApiKey>, DomainError> {
        let row = sqlx::query_as::<_, ApiKeyRow>("SELECT * FROM api_keys WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        match row {
            Some(r) => {
                let perms = get_permissions_for_key(&self.pool, r.id).await?;
                Ok(Some(row_to_api_key(r, perms)))
            }
            None => Ok(None),
        }
    }

    async fn create(&self, cmd: CreateApiKey) -> Result<ApiKey, DomainError> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            r"
            INSERT INTO api_keys (service_name, key_hash, key_prefix)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(&cmd.service_name)
        .bind(&cmd.key_hash)
        .bind(&cmd.key_prefix)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Link permissions via the join table
        sqlx::query(
            r"
            INSERT INTO api_key_permissions (api_key_id, permission_id)
            SELECT $1, p.id FROM permissions p WHERE p.name = ANY($2)
            ",
        )
        .bind(row.id)
        .bind(&cmd.permissions)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let perms = get_permissions_for_key(&self.pool, row.id).await?;
        Ok(row_to_api_key(row, perms))
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
        sqlx::query("UPDATE api_keys SET is_active = FALSE, updated_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
