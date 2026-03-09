//! Discovered match repository adapter.

use crate::entities::DiscoveredMatchRow;
use crate::DbPool;
use async_trait::async_trait;
use portal_core::{DemoId, DiscoveredMatchId, DomainError, GameId, SteamTrackingId};
use portal_domain::entities::discovered_match::DiscoveredMatch;
use portal_domain::repositories::discovered_match::{
    CreateDiscoveredMatch, DiscoveredMatchRepository,
};

/// Column list with `status::TEXT` cast for sqlx compatibility.
const COLUMNS: &str = r"
    id, tracking_id, game_id, share_code, match_id, outcome_id, token,
    status::TEXT as status, gc_data, demo_url, demo_id, error,
    retry_count, max_retries, discovered_at, enriched_at, created_at, updated_at
";

// =============================================================================
// Type Conversions
// =============================================================================

impl From<DiscoveredMatchRow> for DiscoveredMatch {
    fn from(row: DiscoveredMatchRow) -> Self {
        Self {
            id: DiscoveredMatchId::from(row.id),
            tracking_id: SteamTrackingId::from(row.tracking_id),
            game_id: GameId::from(row.game_id),
            share_code: row.share_code,
            match_id: row.match_id,
            outcome_id: row.outcome_id,
            token: row.token,
            status: row.status,
            gc_data: row.gc_data,
            demo_url: row.demo_url,
            demo_id: row.demo_id.map(DemoId::from),
            error: row.error,
            retry_count: row.retry_count,
            max_retries: row.max_retries,
            discovered_at: row.discovered_at,
            enriched_at: row.enriched_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// Discovered Match Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `DiscoveredMatchRepository` trait.
#[derive(Clone)]
pub struct PgDiscoveredMatchRepository {
    pool: DbPool,
}

impl PgDiscoveredMatchRepository {
    /// Create a new PostgreSQL discovered match repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DiscoveredMatchRepository for PgDiscoveredMatchRepository {
    async fn find_by_id(
        &self,
        id: DiscoveredMatchId,
    ) -> Result<Option<DiscoveredMatch>, DomainError> {
        let sql = format!("SELECT {COLUMNS} FROM discovered_matches WHERE id = $1");
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(DiscoveredMatch::from))
    }

    async fn find_by_share_code(
        &self,
        share_code: &str,
    ) -> Result<Option<DiscoveredMatch>, DomainError> {
        let sql = format!("SELECT {COLUMNS} FROM discovered_matches WHERE share_code = $1");
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(share_code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(DiscoveredMatch::from))
    }

    async fn upsert(&self, cmd: CreateDiscoveredMatch) -> Result<DiscoveredMatch, DomainError> {
        let sql = format!(
            r"
            INSERT INTO discovered_matches (tracking_id, game_id, share_code, match_id, outcome_id, token)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (share_code) DO UPDATE SET updated_at = NOW()
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(cmd.tracking_id.as_uuid())
            .bind(cmd.game_id.as_uuid())
            .bind(&cmd.share_code)
            .bind(cmd.match_id)
            .bind(cmd.outcome_id)
            .bind(cmd.token)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(DiscoveredMatch::from(row))
    }

    async fn find_pending(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        let sql = format!(
            r"
            SELECT {COLUMNS} FROM discovered_matches
            WHERE game_id = $1
              AND status IN ('pending', 'failed')
              AND retry_count < max_retries
            ORDER BY created_at ASC
            LIMIT $2
            "
        );
        let rows = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(game_id.as_uuid())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(DiscoveredMatch::from).collect())
    }

    async fn claim(&self, id: DiscoveredMatchId) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r"
            UPDATE discovered_matches
            SET status = 'enriching', updated_at = NOW()
            WHERE id = $1 AND status IN ('pending', 'failed')
            ",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn mark_enriched(
        &self,
        id: DiscoveredMatchId,
        gc_data: serde_json::Value,
        demo_url: Option<String>,
    ) -> Result<DiscoveredMatch, DomainError> {
        let sql = format!(
            r"
            UPDATE discovered_matches
            SET status = 'enriched',
                gc_data = $2,
                demo_url = $3,
                enriched_at = NOW(),
                error = NULL,
                updated_at = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(id.as_uuid())
            .bind(&gc_data)
            .bind(&demo_url)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Discovered match not found".into()))?;

        Ok(DiscoveredMatch::from(row))
    }

    async fn find_recent_with_demo_url(
        &self,
        game_id: GameId,
        tracking_id: Option<SteamTrackingId>,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        let (sql, has_tracking) = if tracking_id.is_some() {
            (
                format!(
                    r"
                    SELECT {COLUMNS} FROM discovered_matches
                    WHERE game_id = $1
                      AND tracking_id = $2
                      AND status = 'enriched'
                      AND demo_url IS NOT NULL
                    ORDER BY enriched_at DESC
                    LIMIT $3
                    "
                ),
                true,
            )
        } else {
            (
                format!(
                    r"
                    SELECT {COLUMNS} FROM discovered_matches
                    WHERE game_id = $1
                      AND status = 'enriched'
                      AND demo_url IS NOT NULL
                    ORDER BY enriched_at DESC
                    LIMIT $2
                    "
                ),
                false,
            )
        };

        let rows = if has_tracking {
            sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
                .bind(game_id.as_uuid())
                .bind(tracking_id.unwrap().as_uuid())
                .bind(limit)
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
                .bind(game_id.as_uuid())
                .bind(limit)
                .fetch_all(&self.pool)
                .await
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(DiscoveredMatch::from).collect())
    }

    async fn mark_failed(
        &self,
        id: DiscoveredMatchId,
        error: &str,
    ) -> Result<DiscoveredMatch, DomainError> {
        let sql = format!(
            r"
            UPDATE discovered_matches
            SET status = 'failed',
                error = $2,
                retry_count = retry_count + 1,
                updated_at = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(id.as_uuid())
            .bind(error)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Discovered match not found".into()))?;

        Ok(DiscoveredMatch::from(row))
    }
}
