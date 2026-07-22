//! Audit repository adapter.

use crate::DbPool;
use crate::entities::EntityChangeRow;
use async_trait::async_trait;
use portal_core::{DomainError, PlayerId};
use portal_domain::entities::audit::{ChangeType, EntityChange, EntityChangeId};
use portal_domain::repositories::{CreateEntityChange, EntityChangeRepository};
use uuid::Uuid;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<EntityChangeRow> for EntityChange {
    fn from(row: EntityChangeRow) -> Self {
        Self {
            id: row.id,
            entity_type: row.entity_type,
            entity_id: row.entity_id,
            change_type: row.change_type.parse().unwrap_or(ChangeType::Update),
            field_name: row.field_name,
            old_value: row.old_value,
            new_value: row.new_value,
            changed_by: PlayerId::from(row.changed_by),
            reverted_at: row.reverted_at,
            reverted_by: row.reverted_by.map(PlayerId::from),
            revert_reason: row.revert_reason,
            request_id: row.request_id,
            ip_address: row.ip_address,
            user_agent: row.user_agent,
            created_at: row.created_at,
        }
    }
}

// =============================================================================
// Entity Change Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the `EntityChangeRepository` trait.
#[derive(Clone)]
pub struct PgEntityChangeRepository {
    pool: DbPool,
}

impl PgEntityChangeRepository {
    /// Create a new `PostgreSQL` entity change repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EntityChangeRepository for PgEntityChangeRepository {
    async fn create(&self, cmd: CreateEntityChange) -> Result<EntityChange, DomainError> {
        let row = sqlx::query_as::<_, EntityChangeRow>(
            r"
            INSERT INTO entity_changes (
                entity_type, entity_id, change_type, field_name,
                old_value, new_value, changed_by,
                request_id, ip_address, user_agent
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            ",
        )
        .bind(&cmd.entity_type)
        .bind(cmd.entity_id)
        .bind(cmd.change_type.to_string())
        .bind(&cmd.field_name)
        .bind(&cmd.old_value)
        .bind(&cmd.new_value)
        .bind(cmd.changed_by.as_uuid())
        .bind(&cmd.request_id)
        .bind(&cmd.ip_address)
        .bind(&cmd.user_agent)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(EntityChange::from(row))
    }

    async fn find_by_id(&self, id: EntityChangeId) -> Result<Option<EntityChange>, DomainError> {
        let row =
            sqlx::query_as::<_, EntityChangeRow>("SELECT * FROM entity_changes WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(EntityChange::from))
    }

    async fn list_by_entity(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityChange>, DomainError> {
        let rows = sqlx::query_as::<_, EntityChangeRow>(
            r"
            SELECT * FROM entity_changes
            WHERE entity_type = $1 AND entity_id = $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            ",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(EntityChange::from).collect())
    }

    async fn count_by_entity(
        &self,
        entity_type: &str,
        entity_id: Uuid,
    ) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM entity_changes WHERE entity_type = $1 AND entity_id = $2",
        )
        .bind(entity_type)
        .bind(entity_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn list_by_field(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        field_name: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityChange>, DomainError> {
        let rows = sqlx::query_as::<_, EntityChangeRow>(
            r"
            SELECT * FROM entity_changes
            WHERE entity_type = $1 AND entity_id = $2 AND field_name = $3
            ORDER BY created_at DESC
            LIMIT $4 OFFSET $5
            ",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(field_name)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(EntityChange::from).collect())
    }

    async fn list_by_player(
        &self,
        player_id: PlayerId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityChange>, DomainError> {
        let rows = sqlx::query_as::<_, EntityChangeRow>(
            r"
            SELECT * FROM entity_changes
            WHERE changed_by = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(player_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(EntityChange::from).collect())
    }

    async fn mark_reverted(
        &self,
        id: EntityChangeId,
        reverted_by: PlayerId,
        reason: Option<String>,
    ) -> Result<EntityChange, DomainError> {
        let row = sqlx::query_as::<_, EntityChangeRow>(
            r"
            UPDATE entity_changes
            SET reverted_at = NOW(), reverted_by = $2, revert_reason = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id)
        .bind(reverted_by.as_uuid())
        .bind(reason)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(EntityChange::from(row))
    }

    async fn get_latest_field_change(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        field_name: &str,
    ) -> Result<Option<EntityChange>, DomainError> {
        let row = sqlx::query_as::<_, EntityChangeRow>(
            r"
            SELECT * FROM entity_changes
            WHERE entity_type = $1 AND entity_id = $2 AND field_name = $3
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(field_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(EntityChange::from))
    }
}
