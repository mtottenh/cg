//! Demo upload attempt repository adapter
//! (docs/demo-ingestion-admin-design.md §4).

use crate::DbPool;
use crate::entities::DemoUploadAttemptRow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{
    DemoId, DemoUploadAttemptId, GameServerId, ServerReservationId, TournamentId, TournamentMatchId,
};
use portal_core::types::{DemoStatus, DemoUploadLinkNote, DemoUploadOutcome};
use portal_domain::entities::demo_upload::{
    DEMO_MISSING_AFTER_MINUTES, DEMO_MISSING_LOOKBACK_DAYS, DEMO_STUCK_AFTER_MINUTES,
};
use portal_domain::entities::{
    DemoUploadAttempt, DemoUploadAttemptDetail, LatestServerUpload, MissingDemo,
    ServerUploadsSummary,
};
use portal_domain::repositories::{
    CreateDemoUploadAttempt, DemoUploadAttemptFilter, DemoUploadAttemptRepository,
};
use uuid::Uuid;

fn internal(e: sqlx::Error) -> DomainError {
    DomainError::Internal(e.to_string())
}

/// The "missing demo" predicate over `server_reservations r`, shared by the
/// overview count and the missing list so the two can never disagree.
///
/// `$1` = now, `$2` = minutes after completion, `$3` = lookback days. A
/// reservation is missing its demo when it completed inside the window and
/// neither a demo link to its match nor a `stored` attempt carrying its
/// MatchZy id exists (the latter covers an upload that arrived but could not
/// be linked — that is a linking problem, not a missing demo).
const MISSING_PREDICATE: &str = "r.status = 'completed' \
     AND r.server_id IS NOT NULL \
     AND r.completed_at < $1 - make_interval(mins => $2) \
     AND r.completed_at > $1 - make_interval(days => $3) \
     AND NOT EXISTS (SELECT 1 FROM demo_match_links l WHERE l.match_id = r.match_id) \
     AND NOT EXISTS ( \
         SELECT 1 FROM demo_upload_attempts a \
         WHERE a.outcome = 'stored' AND a.matchzy_id = r.matchzy_id)";

const ATTEMPT_COLS: &str = "a.id, a.received_at, a.server_id, a.outcome, a.http_status, \
     a.file_name, a.matchzy_id, a.map_number, a.bytes, a.duration_ms, a.demo_id, \
     a.linked_match_id, a.link_note, a.error";

impl TryFrom<DemoUploadAttemptRow> for DemoUploadAttempt {
    type Error = DomainError;

    fn try_from(row: DemoUploadAttemptRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: DemoUploadAttemptId::from(row.id),
            received_at: row.received_at,
            server_id: row.server_id.map(GameServerId::from),
            // The CHECK constraint pins the vocabulary; a parse failure is a
            // schema/code mismatch, not bad input.
            outcome: row.outcome.parse().map_err(DomainError::Internal)?,
            http_status: i32::from(row.http_status),
            file_name: row.file_name,
            matchzy_id: row.matchzy_id,
            map_number: row.map_number,
            bytes: row.bytes,
            duration_ms: row.duration_ms,
            demo_id: row.demo_id.map(DemoId::from),
            linked_match_id: row.linked_match_id.map(TournamentMatchId::from),
            link_note: row
                .link_note
                .as_deref()
                .map(str::parse::<DemoUploadLinkNote>)
                .transpose()
                .map_err(DomainError::Internal)?,
            error: row.error,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct DemoUploadAttemptDetailRow {
    #[sqlx(flatten)]
    attempt: DemoUploadAttemptRow,
    server_name: Option<String>,
    demo_status: Option<String>,
    linked_tournament_id: Option<Uuid>,
    linked_tournament_slug: Option<String>,
}

impl TryFrom<DemoUploadAttemptDetailRow> for DemoUploadAttemptDetail {
    type Error = DomainError;

    fn try_from(row: DemoUploadAttemptDetailRow) -> Result<Self, Self::Error> {
        Ok(Self {
            attempt: row.attempt.try_into()?,
            server_name: row.server_name,
            // Lenient: an unknown catalog status reads as absent rather than
            // failing the whole page.
            demo_status: row
                .demo_status
                .as_deref()
                .and_then(|s| s.parse::<DemoStatus>().ok()),
            linked_tournament_id: row.linked_tournament_id.map(TournamentId::from),
            linked_tournament_slug: row.linked_tournament_slug,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SummaryRow {
    uploaded_24h: i64,
    refused_24h: i64,
    failed_24h: i64,
    missing: i64,
    stuck_pending: i64,
    last_upload_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct MissingDemoRow {
    reservation_id: Uuid,
    match_id: Uuid,
    tournament_id: Uuid,
    tournament_slug: Option<String>,
    tournament_name: Option<String>,
    matchzy_id: i64,
    server_id: Uuid,
    server_name: Option<String>,
    completed_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct LatestRow {
    server_id: Uuid,
    received_at: DateTime<Utc>,
    outcome: String,
}

/// Threshold constants as SQL `int` binds (all small; the casts cannot fail).
fn mins(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// `PostgreSQL` implementation of `DemoUploadAttemptRepository`.
#[derive(Debug, Clone)]
pub struct PgDemoUploadAttemptRepository {
    pool: DbPool,
}

impl PgDemoUploadAttemptRepository {
    /// Create a new PostgreSQL demo upload attempt repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DemoUploadAttemptRepository for PgDemoUploadAttemptRepository {
    async fn record(
        &self,
        attempt: CreateDemoUploadAttempt,
    ) -> Result<DemoUploadAttempt, DomainError> {
        let row = sqlx::query_as::<_, DemoUploadAttemptRow>(
            "INSERT INTO demo_upload_attempts \
                (id, server_id, outcome, http_status, file_name, matchzy_id, map_number, \
                 bytes, duration_ms, demo_id, linked_match_id, link_note, error) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             RETURNING *",
        )
        .bind(DemoUploadAttemptId::new().as_uuid())
        .bind(attempt.server_id.map(|s| s.as_uuid()))
        .bind(attempt.outcome.as_str())
        .bind(i16::try_from(attempt.http_status).unwrap_or(i16::MAX))
        .bind(attempt.file_name)
        .bind(attempt.matchzy_id)
        .bind(attempt.map_number)
        .bind(attempt.bytes)
        .bind(attempt.duration_ms)
        .bind(attempt.demo_id.map(|d| d.as_uuid()))
        .bind(attempt.linked_match_id.map(|m| m.as_uuid()))
        .bind(attempt.link_note.map(|n| n.as_str()))
        .bind(attempt.error)
        .fetch_one(&self.pool)
        .await
        .map_err(internal)?;
        row.try_into()
    }

    async fn list(
        &self,
        filter: DemoUploadAttemptFilter,
    ) -> Result<Vec<DemoUploadAttemptDetail>, DomainError> {
        // Keyset on received_at alone (exclusive): attempts are separate
        // statements, so equal microsecond timestamps do not occur in
        // practice.
        let sql = format!(
            "SELECT {ATTEMPT_COLS}, gs.name AS server_name, d.status AS demo_status, \
                    tm.tournament_id AS linked_tournament_id, \
                    t.slug AS linked_tournament_slug \
             FROM demo_upload_attempts a \
             LEFT JOIN game_servers gs ON gs.id = a.server_id \
             LEFT JOIN demos d ON d.id = a.demo_id \
             LEFT JOIN tournament_matches tm ON tm.id = a.linked_match_id \
             LEFT JOIN tournaments t ON t.id = tm.tournament_id \
             WHERE ($1::uuid IS NULL OR a.server_id = $1) \
               AND ($2::text IS NULL OR a.outcome = $2) \
               AND ($3::uuid IS NULL OR a.demo_id = $3) \
               AND ($4::timestamptz IS NULL OR a.received_at < $4) \
             ORDER BY a.received_at DESC, a.id DESC \
             LIMIT $5"
        );
        let rows = sqlx::query_as::<_, DemoUploadAttemptDetailRow>(&sql)
            .bind(filter.server_id.map(|s| s.as_uuid()))
            .bind(filter.outcome.map(|o| o.as_str()))
            .bind(filter.demo_id.map(|d| d.as_uuid()))
            .bind(filter.before)
            .bind(filter.limit)
            .fetch_all(&self.pool)
            .await
            .map_err(internal)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn summary(&self, now: DateTime<Utc>) -> Result<ServerUploadsSummary, DomainError> {
        let sql = format!(
            "SELECT \
                (SELECT COUNT(*) FROM demo_upload_attempts \
                  WHERE received_at > $1 - INTERVAL '24 hours' \
                    AND outcome = 'stored') AS uploaded_24h, \
                (SELECT COUNT(*) FROM demo_upload_attempts \
                  WHERE received_at > $1 - INTERVAL '24 hours' \
                    AND outcome IN ('rejected_auth', 'rejected_request')) AS refused_24h, \
                (SELECT COUNT(*) FROM demo_upload_attempts \
                  WHERE received_at > $1 - INTERVAL '24 hours' \
                    AND outcome IN ('store_failed', 'catalog_failed')) AS failed_24h, \
                (SELECT COUNT(*) FROM server_reservations r WHERE {MISSING_PREDICATE}) AS missing, \
                (SELECT COUNT(*) FROM demos d \
                  WHERE d.s3_key LIKE 'matchzy/%' \
                    AND d.status IN ('pending', 'processing') \
                    AND d.discovered_at < $1 - make_interval(mins => $4)) AS stuck_pending, \
                (SELECT MAX(received_at) FROM demo_upload_attempts \
                  WHERE outcome = 'stored') AS last_upload_at"
        );
        let row = sqlx::query_as::<_, SummaryRow>(&sql)
            .bind(now)
            .bind(mins(DEMO_MISSING_AFTER_MINUTES))
            .bind(mins(DEMO_MISSING_LOOKBACK_DAYS))
            .bind(mins(DEMO_STUCK_AFTER_MINUTES))
            .fetch_one(&self.pool)
            .await
            .map_err(internal)?;
        Ok(ServerUploadsSummary {
            uploaded_24h: row.uploaded_24h,
            refused_24h: row.refused_24h,
            failed_24h: row.failed_24h,
            missing: row.missing,
            stuck_pending: row.stuck_pending,
            last_upload_at: row.last_upload_at,
        })
    }

    async fn list_missing(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MissingDemo>, DomainError> {
        let sql = format!(
            "SELECT r.id AS reservation_id, r.match_id, tm.tournament_id, \
                    t.slug AS tournament_slug, t.name AS tournament_name, r.matchzy_id, \
                    r.server_id, gs.name AS server_name, r.completed_at \
             FROM server_reservations r \
             JOIN tournament_matches tm ON tm.id = r.match_id \
             LEFT JOIN tournaments t ON t.id = tm.tournament_id \
             LEFT JOIN game_servers gs ON gs.id = r.server_id \
             WHERE {MISSING_PREDICATE} \
             ORDER BY r.completed_at DESC \
             LIMIT $4"
        );
        let rows = sqlx::query_as::<_, MissingDemoRow>(&sql)
            .bind(now)
            .bind(mins(DEMO_MISSING_AFTER_MINUTES))
            .bind(mins(DEMO_MISSING_LOOKBACK_DAYS))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(internal)?;
        Ok(rows
            .into_iter()
            .map(|r| MissingDemo {
                reservation_id: ServerReservationId::from(r.reservation_id),
                match_id: TournamentMatchId::from(r.match_id),
                tournament_id: TournamentId::from(r.tournament_id),
                tournament_slug: r.tournament_slug,
                tournament_name: r.tournament_name,
                matchzy_id: r.matchzy_id,
                server_id: GameServerId::from(r.server_id),
                server_name: r.server_name,
                completed_at: r.completed_at,
            })
            .collect())
    }

    async fn latest_per_server(
        &self,
        server_ids: &[GameServerId],
    ) -> Result<Vec<LatestServerUpload>, DomainError> {
        if server_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = server_ids.iter().map(GameServerId::as_uuid).collect();
        // One index probe per server on (server_id, received_at DESC).
        let rows = sqlx::query_as::<_, LatestRow>(
            "SELECT s.id AS server_id, a.received_at, a.outcome \
             FROM unnest($1::uuid[]) AS s(id) \
             CROSS JOIN LATERAL ( \
                 SELECT received_at, outcome FROM demo_upload_attempts \
                 WHERE server_id = s.id \
                 ORDER BY received_at DESC LIMIT 1 \
             ) a",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        rows.into_iter()
            .map(|r| {
                Ok(LatestServerUpload {
                    server_id: GameServerId::from(r.server_id),
                    received_at: r.received_at,
                    outcome: r
                        .outcome
                        .parse::<DemoUploadOutcome>()
                        .map_err(DomainError::Internal)?,
                })
            })
            .collect()
    }

    async fn prune(&self, before: DateTime<Utc>) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM demo_upload_attempts WHERE received_at < $1")
            .bind(before)
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        Ok(result.rows_affected())
    }
}
