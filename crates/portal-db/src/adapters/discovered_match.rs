//! Discovered match repository adapter.

use crate::DbPool;
use crate::entities::DiscoveredMatchRow;
use async_trait::async_trait;
use portal_core::{DemoId, DiscoveredMatchId, DomainError, GameId, SteamTrackingId};
use portal_domain::entities::discovered_match::DiscoveredMatch;
use portal_domain::repositories::discovered_match::{
    BackoffPolicy, CreateDiscoveredMatch, DemoOutcome, DiscoveredMatchRepository,
};

/// Column list with the enum columns cast to `TEXT` for sqlx compatibility.
const COLUMNS: &str = r"
    id, tracking_id, game_id, share_code, match_id, outcome_id, token,
    status::TEXT as status, gc_data, demo_url, demo_id, error,
    retry_count, max_retries, next_attempt_at, claimed_at, last_attempt_at,
    demo_status::TEXT as demo_status, demo_retry_count, demo_max_retries,
    demo_next_attempt_at, demo_last_attempt_at, demo_error,
    discovered_at, enriched_at, created_at, updated_at
";

/// [`COLUMNS`] qualified with the `m` alias, for statements that join another
/// relation (`UPDATE … FROM claimed c … RETURNING`), where a bare `id` would be
/// ambiguous.
const COLUMNS_M: &str = r"
    m.id, m.tracking_id, m.game_id, m.share_code, m.match_id, m.outcome_id, m.token,
    m.status::TEXT as status, m.gc_data, m.demo_url, m.demo_id, m.error,
    m.retry_count, m.max_retries, m.next_attempt_at, m.claimed_at, m.last_attempt_at,
    m.demo_status::TEXT as demo_status, m.demo_retry_count, m.demo_max_retries,
    m.demo_next_attempt_at, m.demo_last_attempt_at, m.demo_error,
    m.discovered_at, m.enriched_at, m.created_at, m.updated_at
";

/// Exponential backoff with equal jitter, as a SQL interval expression.
///
/// `attempts_col` is the row's own attempt counter, so the delay grows with the
/// row rather than with anything the caller has read: `min(base * 2^(n-1), cap)`
/// scaled into `[50%, 100%]`. `GREATEST(n - 1, 0)` keeps the first retry at
/// exactly `base` and guards against a zero counter.
///
/// `base_param` and `cap_param` are bound-parameter placeholders, passed in so
/// each call site can slot the expression into its own statement at whatever
/// index it has spare. Both are cast explicitly: these adapters use the runtime
/// query form, so an uncast placeholder in a bare arithmetic expression leaves
/// Postgres to guess the type.
fn backoff_interval(attempts_col: &str, base_param: &str, cap_param: &str) -> String {
    format!(
        "(LEAST({base_param}::double precision * POWER(2, GREATEST({attempts_col} - 1, 0)), \
         {cap_param}::double precision) * (0.5 + random() * 0.5)) * INTERVAL '1 second'"
    )
}

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
            next_attempt_at: row.next_attempt_at,
            claimed_at: row.claimed_at,
            last_attempt_at: row.last_attempt_at,
            demo_status: row.demo_status,
            demo_retry_count: row.demo_retry_count,
            demo_max_retries: row.demo_max_retries,
            demo_next_attempt_at: row.demo_next_attempt_at,
            demo_last_attempt_at: row.demo_last_attempt_at,
            demo_error: row.demo_error,
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
        // `next_attempt_at <= NOW()` is the backoff gate. Ordering by it rather
        // than by created_at means the match that has waited longest past its
        // schedule goes first, instead of an old match that is still cooling
        // down blocking the head of the queue on every cycle.
        let sql = format!(
            r"
            SELECT {COLUMNS} FROM discovered_matches
            WHERE game_id = $1
              AND status IN ('pending', 'failed')
              AND retry_count < max_retries
              AND next_attempt_at <= NOW()
            ORDER BY next_attempt_at ASC, created_at ASC
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

    async fn reclaim_stale(
        &self,
        lease_secs: i64,
        backoff: BackoffPolicy,
    ) -> Result<u64, DomainError> {
        // The retry is charged HERE, not on the next claim: the point of a
        // lease is that a worker which dies mid-enrichment has still consumed
        // an attempt. Without that, a match that reliably kills its enricher
        // is handed out forever and takes the whole queue down with it.
        let backoff_sql = backoff_interval("retry_count + 1", "$2", "$3");
        let sql = format!(
            r"
            UPDATE discovered_matches
            SET status          = 'failed',
                retry_count     = retry_count + 1,
                last_attempt_at = NOW(),
                claimed_at      = NULL,
                error           = COALESCE(error, 'enrichment claim expired (worker died mid-attempt)'),
                next_attempt_at = NOW() + {backoff_sql},
                updated_at      = NOW()
            WHERE status = 'enriching'
              AND claimed_at IS NOT NULL
              AND claimed_at < NOW() - ($1::double precision * INTERVAL '1 second')
            "
        );
        let result = sqlx::query(&sql)
            .bind(lease_secs as f64)
            .bind(backoff.base_secs)
            .bind(backoff.cap_secs)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn claim(&self, id: DiscoveredMatchId) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r"
            UPDATE discovered_matches
            SET status          = 'enriching',
                claimed_at      = NOW(),
                last_attempt_at = NOW(),
                updated_at      = NOW()
            WHERE id = $1
              AND status IN ('pending', 'failed')
              AND next_attempt_at <= NOW()
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
        // Enrichment succeeding is what OPENS the demo stage: until the GC
        // answers there is no URL to fetch. `not_applicable` when GC returned
        // no demo (nothing to do, and it must not sit in the queue as pending
        // forever); otherwise `pending` so the demo worker picks it up.
        //
        // A demo stage that already reached a terminal state is left alone — a
        // re-delivered enrichment must not re-download a demo that parsed fine,
        // nor revive one already settled as unavailable.
        let sql = format!(
            r"
            UPDATE discovered_matches
            SET status = 'enriched',
                gc_data = $2,
                demo_url = $3,
                enriched_at = NOW(),
                error = NULL,
                claimed_at = NULL,
                demo_status = CASE
                    WHEN $3::text IS NULL THEN 'not_applicable'::demo_extraction_status
                    WHEN demo_status IN ('pending', 'not_applicable')
                        THEN 'pending'::demo_extraction_status
                    ELSE demo_status
                END,
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
        backoff: BackoffPolicy,
    ) -> Result<DiscoveredMatch, DomainError> {
        // The schedule is computed from the POST-increment attempt count, so
        // the first failure waits `base`, the second `2 * base`, and so on.
        let backoff_sql = backoff_interval("retry_count + 1", "$3", "$4");
        let sql = format!(
            r"
            UPDATE discovered_matches
            SET status          = 'failed',
                error           = $2,
                retry_count     = retry_count + 1,
                last_attempt_at = NOW(),
                claimed_at      = NULL,
                next_attempt_at = NOW() + {backoff_sql},
                updated_at      = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(id.as_uuid())
            .bind(error)
            .bind(backoff.base_secs)
            .bind(backoff.cap_secs)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Discovered match not found".into()))?;

        Ok(DiscoveredMatch::from(row))
    }

    async fn lease_demo_jobs(
        &self,
        game_id: GameId,
        limit: i64,
        lease_secs: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        // Select, increment and lease in ONE statement. Splitting them would
        // reintroduce both bugs this stage exists to fix: two enrichers could
        // take the same job, and a worker that dies before reporting would
        // never have recorded its attempt.
        //
        // `demo_next_attempt_at` doubles as the lease expiry — pushing it out
        // by `lease_secs` makes the row ineligible for exactly as long as the
        // attempt should take, with no second column to keep consistent.
        let sql = format!(
            r"
            WITH claimed AS (
                SELECT id FROM discovered_matches
                WHERE game_id = $1
                  AND status = 'enriched'
                  AND demo_url IS NOT NULL
                  AND demo_status = 'pending'
                  AND demo_retry_count < demo_max_retries
                  AND demo_next_attempt_at <= NOW()
                ORDER BY demo_next_attempt_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE discovered_matches m
            SET demo_retry_count     = m.demo_retry_count + 1,
                demo_last_attempt_at = NOW(),
                demo_next_attempt_at = NOW() + ($3::double precision * INTERVAL '1 second'),
                updated_at           = NOW()
            FROM claimed c
            WHERE m.id = c.id
            RETURNING {COLUMNS_M}
            "
        );
        let rows = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(game_id.as_uuid())
            .bind(limit)
            .bind(lease_secs as f64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(DiscoveredMatch::from).collect())
    }

    async fn record_demo_result(
        &self,
        id: DiscoveredMatchId,
        outcome: DemoOutcome,
        error: Option<&str>,
        backoff: BackoffPolicy,
    ) -> Result<DiscoveredMatch, DomainError> {
        // `demo_retry_count` was already incremented by the lease, so it is the
        // number of attempts MADE. Budget is spent when it reaches the ceiling.
        let backoff_sql = backoff_interval("demo_retry_count", "$4", "$5");
        let sql = format!(
            r"
            UPDATE discovered_matches
            SET demo_status = CASE
                    WHEN $2::text = 'succeeded' THEN 'succeeded'::demo_extraction_status
                    WHEN $2::text = 'empty'     THEN 'empty'::demo_extraction_status
                    -- 410 and friends: no budget spent proving it again.
                    WHEN $2::text = 'gone'      THEN 'unavailable'::demo_extraction_status
                    -- Retryable, but the budget is gone: settle terminally as
                    -- whatever the last attempt saw.
                    WHEN demo_retry_count >= demo_max_retries THEN
                        CASE WHEN $2::text = 'unavailable'
                             THEN 'unavailable'::demo_extraction_status
                             ELSE 'failed'::demo_extraction_status
                        END
                    ELSE 'pending'::demo_extraction_status
                END,
                demo_error = $3::text,
                demo_next_attempt_at = CASE
                    WHEN $2::text IN ('succeeded', 'empty', 'gone')
                      OR demo_retry_count >= demo_max_retries
                        THEN demo_next_attempt_at
                    ELSE NOW() + {backoff_sql}
                END,
                updated_at = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(id.as_uuid())
            .bind(outcome.as_str())
            .bind(error)
            .bind(backoff.base_secs)
            .bind(backoff.cap_secs)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Discovered match not found".into()))?;

        Ok(DiscoveredMatch::from(row))
    }

    async fn count_by_demo_status(
        &self,
        game_id: Option<GameId>,
    ) -> Result<Vec<(String, i64)>, DomainError> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            r"
            SELECT demo_status::TEXT AS demo_status, COUNT(*)
            FROM discovered_matches
            WHERE ($1::uuid IS NULL OR game_id = $1)
            GROUP BY demo_status
            ",
        )
        .bind(game_id.map(|g| g.as_uuid()))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows)
    }

    async fn count_by_status(
        &self,
        game_id: Option<GameId>,
    ) -> Result<Vec<(String, i64)>, DomainError> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            r"
            SELECT status::TEXT AS status, COUNT(*)
            FROM discovered_matches
            WHERE ($1::uuid IS NULL OR game_id = $1)
            GROUP BY status
            ",
        )
        .bind(game_id.map(|g| g.as_uuid()))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows)
    }

    async fn count_retry_exhausted(&self, game_id: Option<GameId>) -> Result<i64, DomainError> {
        let (count,): (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*)
            FROM discovered_matches
            WHERE status = 'failed'
              AND retry_count >= max_retries
              AND ($1::uuid IS NULL OR game_id = $1)
            ",
        )
        .bind(game_id.map(|g| g.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count)
    }

    async fn requeue_failed(
        &self,
        game_id: Option<GameId>,
        only_exhausted: bool,
    ) -> Result<u64, DomainError> {
        // `next_attempt_at` is staggered rather than set to NOW() for every
        // row: a requeue of a large backlog that all became due in the same
        // instant would arrive at the rate-limited GC as one burst.
        let result = sqlx::query(
            r"
            UPDATE discovered_matches
            SET status          = 'pending',
                error           = NULL,
                retry_count     = 0,
                claimed_at      = NULL,
                next_attempt_at = NOW() + (random() * 300) * INTERVAL '1 second',
                updated_at      = NOW()
            WHERE status = 'failed'
              AND ($1::uuid IS NULL OR game_id = $1)
              AND ($2::bool IS FALSE OR retry_count >= max_retries)
            ",
        )
        .bind(game_id.map(|g| g.as_uuid()))
        .bind(only_exhausted)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn requeue_one(&self, id: DiscoveredMatchId) -> Result<DiscoveredMatch, DomainError> {
        let sql = format!(
            r"
            UPDATE discovered_matches
            SET status          = 'pending',
                error           = NULL,
                retry_count     = 0,
                claimed_at      = NULL,
                next_attempt_at = NOW(),
                updated_at      = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Discovered match not found".into()))?;

        Ok(DiscoveredMatch::from(row))
    }

    async fn list_by_status(
        &self,
        game_id: Option<GameId>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        let sql = format!(
            r"
            SELECT {COLUMNS} FROM discovered_matches
            WHERE ($1::uuid IS NULL OR game_id = $1)
              AND ($2::text IS NULL OR status::TEXT = $2)
            ORDER BY discovered_at DESC
            LIMIT $3
            "
        );
        let rows = sqlx::query_as::<_, DiscoveredMatchRow>(&sql)
            .bind(game_id.map(|g| g.as_uuid()))
            .bind(status)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(DiscoveredMatch::from).collect())
    }
}
