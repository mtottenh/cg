//! Ban repository adapter.

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

/// A `bans` row joined to the banned user's identity.
///
/// P-123: the domain `Ban` carries `username`/`display_name`, so every query
/// that produces one must join. Declared here rather than beside `BanRow` in
/// `entities/` because that row maps the table one-to-one and is still used by
/// the raw `repositories::rbac` layer (and the CLI on top of it), whose
/// `SELECT *` / `RETURNING *` queries have no user columns to bind.
///
/// The `users` join is INNER (`bans.user_id` is a NOT NULL FK and
/// `users.username` is NOT NULL); the `players` join is LEFT, because a user
/// need not have a player profile and an active ban must never disappear from
/// a moderation listing merely because they do not.
#[derive(Debug, sqlx::FromRow)]
struct BanWithUserRow {
    id: uuid::Uuid,
    user_id: uuid::Uuid,
    ban_type: String,
    reason: String,
    scope_type: Option<String>,
    scope_id: Option<uuid::Uuid>,
    issued_by: Option<uuid::Uuid>,
    starts_at: chrono::DateTime<Utc>,
    ends_at: Option<chrono::DateTime<Utc>>,
    lifted_at: Option<chrono::DateTime<Utc>>,
    lifted_by: Option<uuid::Uuid>,
    lift_reason: Option<String>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    username: String,
    display_name: Option<String>,
}

/// The columns every ban query selects, aliased to match [`BanWithUserRow`].
/// Kept in one place so a new query cannot quietly omit the identity columns
/// and reintroduce P-123 on one surface while the others stay fixed.
const BAN_WITH_USER_COLUMNS: &str = "b.id, b.user_id, b.ban_type, b.reason, b.scope_type,
     b.scope_id, b.issued_by, b.starts_at, b.ends_at, b.lifted_at, b.lifted_by,
     b.lift_reason, b.created_at, b.updated_at, u.username, p.display_name";

/// The joins those columns require. `b` must already be in scope.
const BAN_USER_JOINS: &str = "INNER JOIN users u ON u.id = b.user_id
     LEFT JOIN players p ON p.user_id = b.user_id";

impl From<BanWithUserRow> for Ban {
    fn from(row: BanWithUserRow) -> Self {
        Self {
            id: BanId::from(row.id),
            user_id: UserId::from(row.user_id),
            username: row.username,
            display_name: row.display_name,
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
        let ban = sqlx::query_as::<_, BanWithUserRow>(&format!(
            "SELECT {BAN_WITH_USER_COLUMNS}
             FROM bans b
             {BAN_USER_JOINS}
             WHERE b.id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(ban.map(Ban::from))
    }

    async fn create(&self, cmd: CreateBanCommand) -> Result<Ban, DomainError> {
        let starts_at = cmd.starts_at.unwrap_or_else(Utc::now);
        let ends_at = cmd
            .duration_seconds
            .map(|secs| starts_at + Duration::seconds(secs));

        // The insert is wrapped in a CTE so the returned row can be joined to
        // the user's identity in one round trip — `RETURNING *` alone cannot
        // reach `users`/`players` (P-123).
        let ban = sqlx::query_as::<_, BanWithUserRow>(&format!(
            "WITH inserted AS (
                 INSERT INTO bans
                     (user_id, ban_type, reason, scope_type, scope_id, issued_by, starts_at, ends_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 RETURNING *
             )
             SELECT {BAN_WITH_USER_COLUMNS}
             FROM inserted b
             {BAN_USER_JOINS}"
        ))
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
        .map_err(|e| {
            // A concurrent identical ban that raced past the service read-guard
            // trips the partial unique index (bans_unique_active_unscoped /
            // bans_unique_active_scoped). Surface it as a Conflict so the loser
            // is a clean no-op rather than a spurious 500.
            if let sqlx::Error::Database(db_err) = &e
                && db_err
                    .constraint()
                    .is_some_and(|c| c.starts_with("bans_unique_active"))
            {
                return DomainError::Conflict(format!(
                    "User already has an active {} ban",
                    cmd.ban_type
                ));
            }
            DomainError::Internal(e.to_string())
        })?;

        Ok(Ban::from(ban))
    }

    async fn create_and_enforce(&self, cmd: CreateBanCommand) -> Result<Ban, DomainError> {
        let starts_at = cmd.starts_at.unwrap_or_else(Utc::now);
        let ends_at = cmd
            .duration_seconds
            .map(|secs| starts_at + Duration::seconds(secs));

        // One transaction spans the ban insert AND its platform enforcement
        // (users.status + refresh-token revoke). The old path issued these as
        // three separate autocommit writes, so a crash/DB error after the
        // insert left an active ban row that was never enforced: visible in
        // the admin API while the user kept full access. Committing them
        // together closes that window. See audit residual "ban enforcement
        // not atomic".
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to begin transaction: {e}")))?;

        let row = sqlx::query_as::<_, BanWithUserRow>(&format!(
            "WITH inserted AS (
                 INSERT INTO bans
                     (user_id, ban_type, reason, scope_type, scope_id, issued_by, starts_at, ends_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 RETURNING *
             )
             SELECT {BAN_WITH_USER_COLUMNS}
             FROM inserted b
             {BAN_USER_JOINS}"
        ))
        .bind(cmd.user_id.as_uuid())
        .bind(cmd.ban_type.to_string())
        .bind(&cmd.reason)
        .bind(&cmd.scope_type)
        .bind(cmd.scope_id)
        .bind(cmd.issued_by.map(|id| id.as_uuid()))
        .bind(starts_at)
        .bind(ends_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            // Preserve the wave-4 conflict mapping from within the tx: a
            // concurrent identical ban that raced past the service read-guard
            // trips the partial unique index (bans_unique_active_unscoped /
            // bans_unique_active_scoped) and must surface as a Conflict (409),
            // not a spurious 500. The failed insert aborts the tx (nothing to
            // roll back yet).
            if let sqlx::Error::Database(db_err) = &e
                && db_err
                    .constraint()
                    .is_some_and(|c| c.starts_with("bans_unique_active"))
            {
                return DomainError::Conflict(format!(
                    "User already has an active {} ban",
                    cmd.ban_type
                ));
            }
            DomainError::Internal(e.to_string())
        })?;

        let ban = Ban::from(row);

        // Enforce platform bans in the same tx. Mirrors the pre-refactor
        // service logic (`ban_type == Platform && ban.is_active()`), the
        // users.status update, and the refresh-token revoke — but now any
        // failure here rolls the ban insert back too.
        if ban.ban_type == BanType::Platform && ban.is_active() {
            let result = sqlx::query(
                r"
                UPDATE users
                SET status = 'banned',
                    status_reason = $2,
                    status_changed_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
                ",
            )
            .bind(ban.user_id.as_uuid())
            .bind(&ban.reason)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

            if result.rows_affected() == 0 {
                return Err(DomainError::UserNotFound(ban.user_id));
            }

            sqlx::query(
                r"UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
            )
            .bind(ban.user_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to commit: {e}")))?;

        Ok(ban)
    }

    async fn lift(
        &self,
        id: BanId,
        lifted_by: UserId,
        lift_reason: Option<&str>,
    ) -> Result<Ban, DomainError> {
        let ban = sqlx::query_as::<_, BanWithUserRow>(&format!(
            "WITH updated AS (
                 UPDATE bans SET
                     lifted_at = NOW(),
                     lifted_by = $2,
                     lift_reason = $3,
                     updated_at = NOW()
                 WHERE id = $1
                 RETURNING *
             )
             SELECT {BAN_WITH_USER_COLUMNS}
             FROM updated b
             {BAN_USER_JOINS}"
        ))
        .bind(id.as_uuid())
        .bind(lifted_by.as_uuid())
        .bind(lift_reason)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::BanNotFound(id))?;

        Ok(Ban::from(ban))
    }

    async fn get_active_for_user(&self, user_id: UserId) -> Result<Vec<Ban>, DomainError> {
        let bans = sqlx::query_as::<_, BanWithUserRow>(&format!(
            "SELECT {BAN_WITH_USER_COLUMNS}
             FROM bans b
             {BAN_USER_JOINS}
             WHERE b.user_id = $1
               AND b.lifted_at IS NULL
               AND b.starts_at <= NOW()
               AND (b.ends_at IS NULL OR b.ends_at > NOW())
             ORDER BY b.starts_at DESC"
        ))
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

        // Build WHERE clauses dynamically.
        //
        // Every column is qualified with the `b` alias. That is not style: the
        // identity join (P-123) brings in `players`, which also has a
        // `user_id` column, so an unqualified `user_id = $1` is now an
        // ambiguous-reference error from Postgres rather than a filter.
        let mut conditions = vec!["1=1".to_string()];
        let mut param_count = 0;

        if filters.user_id.is_some() {
            param_count += 1;
            conditions.push(format!("b.user_id = ${param_count}"));
        }

        if filters.ban_type.is_some() {
            param_count += 1;
            conditions.push(format!("b.ban_type = ${param_count}"));
        }

        if filters.active_only {
            conditions.push(
                "b.lifted_at IS NULL AND b.starts_at <= NOW() AND (b.ends_at IS NULL OR b.ends_at > NOW())"
                    .to_string(),
            );
        }

        if filters.scope_type.is_some() {
            param_count += 1;
            conditions.push(format!("b.scope_type = ${param_count}"));
        }

        if filters.scope_id.is_some() {
            param_count += 1;
            conditions.push(format!("b.scope_id = ${param_count}"));
        }

        let where_clause = conditions.join(" AND ");

        // Count query. It needs no identity join (nothing filters on the
        // joined columns), but it must keep the `b` alias so it can share the
        // WHERE clause with the items query.
        let count_query = format!("SELECT COUNT(*) as count FROM bans b WHERE {where_clause}");
        let items_query = format!(
            "SELECT {BAN_WITH_USER_COLUMNS}
             FROM bans b
             {BAN_USER_JOINS}
             WHERE {where_clause}
             ORDER BY b.created_at DESC
             LIMIT ${} OFFSET ${}",
            param_count + 1,
            param_count + 2
        );

        // Build count query
        let mut count_builder = sqlx::query(&count_query);
        let mut items_builder = sqlx::query_as::<_, BanWithUserRow>(&items_query);

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
        let bans = sqlx::query_as::<_, BanWithUserRow>(&format!(
            "SELECT {BAN_WITH_USER_COLUMNS}
             FROM bans b
             {BAN_USER_JOINS}
             WHERE b.user_id = $1
             ORDER BY b.created_at DESC"
        ))
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(bans.into_iter().map(Ban::from).collect())
    }
}
