//! Ban repository adapter.

use crate::entities::BanRow;
use crate::DbPool;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use portal_core::{BanId, DomainError, UserId};
use portal_domain::entities::{Ban, BanFilters, BanType, CreateBanCommand};
use portal_domain::repositories::{BanRepository, PaginatedBans, PaginationMeta};
use sqlx::Row;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<BanRow> for Ban {
    fn from(row: BanRow) -> Self {
        Self {
            id: BanId::from(row.id),
            user_id: UserId::from(row.user_id),
            ban_type: row.ban_type.parse().unwrap_or(BanType::Platform),
            reason: row.reason,
            scope_type: row.scope_type,
            scope_id: row.scope_id,
            issued_by: row.issued_by.map(UserId::from),
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            lifted_at: row.lifted_at,
            lifted_by: row.lifted_by.map(UserId::from),
            lift_reason: row.lift_reason,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// Ban Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `BanRepository` trait.
#[derive(Clone)]
pub struct PgBanRepository {
    pool: DbPool,
}

impl PgBanRepository {
    /// Create a new `PostgreSQL` ban repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BanRepository for PgBanRepository {
    async fn find_by_id(&self, id: BanId) -> Result<Option<Ban>, DomainError> {
        let ban = sqlx::query_as::<_, BanRow>("SELECT * FROM bans WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(ban.map(Ban::from))
    }

    async fn create(&self, cmd: CreateBanCommand) -> Result<Ban, DomainError> {
        let starts_at = cmd.starts_at.unwrap_or_else(Utc::now);
        let ends_at = cmd.duration_seconds.map(|secs| starts_at + Duration::seconds(secs));

        let ban = sqlx::query_as::<_, BanRow>(
            r"
            INSERT INTO bans (user_id, ban_type, reason, scope_type, scope_id, issued_by, starts_at, ends_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
        )
        .bind(cmd.user_id.as_uuid())
        .bind(cmd.ban_type.to_string())
        .bind(&cmd.reason)
        .bind(&cmd.scope_type)
        .bind(cmd.scope_id)
        .bind(cmd.issued_by.map(|id| id.as_uuid()))
        .bind(starts_at)
        .bind(ends_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Ban::from(ban))
    }

    async fn lift(
        &self,
        id: BanId,
        lifted_by: UserId,
        lift_reason: Option<&str>,
    ) -> Result<Ban, DomainError> {
        let ban = sqlx::query_as::<_, BanRow>(
            r"
            UPDATE bans SET
                lifted_at = NOW(),
                lifted_by = $2,
                lift_reason = $3,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(lifted_by.as_uuid())
        .bind(lift_reason)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::BanNotFound(id.to_string()))?;

        Ok(Ban::from(ban))
    }

    async fn get_active_for_user(&self, user_id: UserId) -> Result<Vec<Ban>, DomainError> {
        let bans = sqlx::query_as::<_, BanRow>(
            r"
            SELECT * FROM bans
            WHERE user_id = $1
              AND lifted_at IS NULL
              AND starts_at <= NOW()
              AND (ends_at IS NULL OR ends_at > NOW())
            ORDER BY starts_at DESC
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(bans.into_iter().map(Ban::from).collect())
    }

    async fn is_platform_banned(&self, user_id: UserId) -> Result<bool, DomainError> {
        let row = sqlx::query(
            r"
            SELECT 1 FROM bans
            WHERE user_id = $1
              AND ban_type = 'platform'
              AND lifted_at IS NULL
              AND starts_at <= NOW()
              AND (ends_at IS NULL OR ends_at > NOW())
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn list(
        &self,
        filters: BanFilters,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedBans, DomainError> {
        let offset = (page - 1) * per_page;

        // Build WHERE clauses dynamically
        let mut conditions = vec!["1=1".to_string()];
        let mut param_count = 0;

        if filters.user_id.is_some() {
            param_count += 1;
            conditions.push(format!("user_id = ${param_count}"));
        }

        if filters.ban_type.is_some() {
            param_count += 1;
            conditions.push(format!("ban_type = ${param_count}"));
        }

        if filters.active_only {
            conditions.push("lifted_at IS NULL AND starts_at <= NOW() AND (ends_at IS NULL OR ends_at > NOW())".to_string());
        }

        if filters.scope_type.is_some() {
            param_count += 1;
            conditions.push(format!("scope_type = ${param_count}"));
        }

        if filters.scope_id.is_some() {
            param_count += 1;
            conditions.push(format!("scope_id = ${param_count}"));
        }

        let where_clause = conditions.join(" AND ");

        // Count query
        let count_query = format!("SELECT COUNT(*) as count FROM bans WHERE {where_clause}");
        let items_query = format!(
            r"
            SELECT * FROM bans
            WHERE {where_clause}
            ORDER BY created_at DESC
            LIMIT ${} OFFSET ${}
            ",
            param_count + 1,
            param_count + 2
        );

        // Build count query
        let mut count_builder = sqlx::query(&count_query);
        let mut items_builder = sqlx::query_as::<_, BanRow>(&items_query);

        // Bind parameters in order
        if let Some(user_id) = &filters.user_id {
            count_builder = count_builder.bind(user_id.as_uuid());
            items_builder = items_builder.bind(user_id.as_uuid());
        }

        if let Some(ban_type) = &filters.ban_type {
            count_builder = count_builder.bind(ban_type.to_string());
            items_builder = items_builder.bind(ban_type.to_string());
        }

        if let Some(scope_type) = &filters.scope_type {
            count_builder = count_builder.bind(scope_type);
            items_builder = items_builder.bind(scope_type);
        }

        if let Some(scope_id) = &filters.scope_id {
            count_builder = count_builder.bind(scope_id);
            items_builder = items_builder.bind(scope_id);
        }

        // Bind pagination parameters
        items_builder = items_builder.bind(per_page).bind(offset);

        // Execute queries
        let count_row = count_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let total_items: i64 = count_row.get("count");
        let total_pages = (total_items + per_page - 1) / per_page;

        let bans = items_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(PaginatedBans {
            items: bans.into_iter().map(Ban::from).collect(),
            pagination: PaginationMeta {
                page,
                per_page,
                total_items,
                total_pages,
            },
        })
    }

    async fn get_user_ban_history(&self, user_id: UserId) -> Result<Vec<Ban>, DomainError> {
        let bans = sqlx::query_as::<_, BanRow>(
            r"
            SELECT * FROM bans
            WHERE user_id = $1
            ORDER BY created_at DESC
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(bans.into_iter().map(Ban::from).collect())
    }
}
