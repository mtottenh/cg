//! Game-server update repository adapter.

use crate::DbPool;
use crate::entities::{GameServerUpdateJobRow, GameServerUpdateStateRow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{GameServerId, GameServerUpdateJobId, UserId};
use portal_core::types::{UpdateJobStatus, UpdateJobTrigger, UpdateOperation};
use portal_domain::entities::{
    AgentUpdateReport, GameServerUpdateJob, GameServerUpdateState, ResultOutcome, UpdateProgress,
    UpdateResult,
};
use portal_domain::repositories::{CreateUpdateJob, GameServerUpdateRepository};
use serde_json::{Map, Value};

const STATE_COLS: &str = "id, agent_capabilities, update_check, update_checked_at, \
     installed_versions, demo_upload_wired, overrides_applied, reload_pending, \
     agent_update_job, matchzy_overrides, matchzy_overrides_pending";

fn internal(e: sqlx::Error) -> DomainError {
    DomainError::Internal(e.to_string())
}

impl From<GameServerUpdateJobRow> for GameServerUpdateJob {
    fn from(row: GameServerUpdateJobRow) -> Self {
        Self {
            id: GameServerUpdateJobId::from(row.id),
            server_id: GameServerId::from(row.server_id),
            // The CHECK constraints make these parses infallible; a failure
            // means the constraint and the enum drifted apart, and reads as
            // the most conservative value rather than a panic.
            operation: row.operation.parse().unwrap_or(UpdateOperation::All),
            status: row.status.parse().unwrap_or(UpdateJobStatus::Lost),
            trigger: row.trigger.parse().unwrap_or(UpdateJobTrigger::Local),
            requested_by: row.requested_by.map(UserId::from),
            forced: row.forced,
            phase: row.phase,
            failed_phase: row.failed_phase,
            before_versions: row.before_versions,
            after_versions: row.after_versions,
            latest_versions: row.latest_versions,
            load: row.load,
            config_report: row.config_report,
            restarted: row.restarted,
            error: row.error,
            log_tail: row.log_tail,
            agent_synthesized: row.agent_synthesized,
            created_at: row.created_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<GameServerUpdateStateRow> for GameServerUpdateState {
    fn from(row: GameServerUpdateStateRow) -> Self {
        Self {
            server_id: GameServerId::from(row.id),
            capabilities: row.agent_capabilities,
            update_check: row.update_check,
            update_checked_at: row.update_checked_at,
            installed_versions: row.installed_versions,
            demo_upload_wired: row.demo_upload_wired,
            overrides_applied: row.overrides_applied,
            reload_pending: row.reload_pending,
            agent_update_job: row.agent_update_job,
            matchzy_overrides: match row.matchzy_overrides {
                Value::Object(map) => map,
                _ => Map::new(),
            },
            matchzy_overrides_pending: row.matchzy_overrides_pending,
        }
    }
}

/// PostgreSQL implementation of the domain `GameServerUpdateRepository`.
#[derive(Debug, Clone)]
pub struct PgGameServerUpdateRepository {
    pool: DbPool,
}

impl PgGameServerUpdateRepository {
    /// Create a new PostgreSQL game-server update repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GameServerUpdateRepository for PgGameServerUpdateRepository {
    async fn get_state(
        &self,
        server_id: GameServerId,
    ) -> Result<Option<GameServerUpdateState>, DomainError> {
        let sql = format!("SELECT {STATE_COLS} FROM game_servers WHERE id = $1");
        let row = sqlx::query_as::<_, GameServerUpdateStateRow>(&sql)
            .bind(server_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        Ok(row.map(Into::into))
    }

    async fn record_agent_report(
        &self,
        server_id: GameServerId,
        report: &AgentUpdateReport,
    ) -> Result<(), DomainError> {
        // Heartbeats arrive every 30 s and rarely change any of this: the
        // IS DISTINCT FROM guard keeps the common case write-free.
        sqlx::query(
            "UPDATE game_servers SET \
                agent_capabilities = $2, demo_upload_wired = $3, overrides_applied = $4, \
                reload_pending = $5, agent_update_job = $6 \
             WHERE id = $1 \
               AND (agent_capabilities, demo_upload_wired, overrides_applied, \
                    reload_pending, agent_update_job) \
                   IS DISTINCT FROM ($2, $3, $4, $5, $6)",
        )
        .bind(server_id.as_uuid())
        .bind(&report.capabilities)
        .bind(report.demo_upload_wired)
        .bind(report.overrides_applied)
        .bind(report.reload_pending)
        .bind(&report.update_job)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn record_update_state(
        &self,
        server_id: GameServerId,
        check: &Value,
        checked_at: Option<DateTime<Utc>>,
        installed: Option<&Value>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_servers SET update_check = $2, update_checked_at = $3, \
                installed_versions = $4 \
             WHERE id = $1",
        )
        .bind(server_id.as_uuid())
        .bind(check)
        .bind(checked_at)
        .bind(installed)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn overlay_installed_versions(
        &self,
        server_id: GameServerId,
        versions: &Value,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<(), DomainError> {
        if !versions.is_object() {
            return Ok(());
        }
        sqlx::query(
            "UPDATE game_servers SET installed_versions = \
                COALESCE(installed_versions, '{}'::jsonb) || jsonb_strip_nulls($2) \
             WHERE id = $1 \
               AND ($3::timestamptz IS NULL OR update_checked_at IS NULL \
                    OR $3 > update_checked_at)",
        )
        .bind(server_id.as_uuid())
        .bind(versions)
        .bind(as_of)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn set_matchzy_overrides(
        &self,
        server_id: GameServerId,
        overrides: &Map<String, Value>,
    ) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE game_servers SET matchzy_overrides = $2, matchzy_overrides_pending = TRUE, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(server_id.as_uuid())
        .bind(Value::Object(overrides.clone()))
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::GameServerNotFound(server_id));
        }
        Ok(())
    }

    async fn mark_overrides_delivered(
        &self,
        server_id: GameServerId,
        overrides: &Map<String, Value>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_servers SET matchzy_overrides_pending = FALSE \
             WHERE id = $1 AND matchzy_overrides = $2",
        )
        .bind(server_id.as_uuid())
        .bind(Value::Object(overrides.clone()))
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn create_job(&self, job: CreateUpdateJob) -> Result<GameServerUpdateJob, DomainError> {
        let row = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "INSERT INTO game_server_update_jobs \
                (id, server_id, operation, status, trigger, requested_by, forced) \
             VALUES ($1, $2, $3, 'queued', 'portal', $4, $5) \
             RETURNING *",
        )
        .bind(job.id.as_uuid())
        .bind(job.server_id.as_uuid())
        .bind(job.operation.as_str())
        .bind(job.requested_by.as_uuid())
        .bind(job.forced)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            let text = e.to_string();
            if text.contains("uq_game_server_update_jobs_active") {
                DomainError::Conflict("an update job is already running on this server".into())
            } else if text.contains("game_server_update_jobs_server_id_fkey") {
                DomainError::GameServerNotFound(job.server_id)
            } else {
                internal(e)
            }
        })?;
        Ok(row.into())
    }

    async fn find_job(
        &self,
        id: GameServerUpdateJobId,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        let row = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "SELECT * FROM game_server_update_jobs WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(Into::into))
    }

    async fn find_active_job(
        &self,
        server_id: GameServerId,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        let row = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "SELECT * FROM game_server_update_jobs \
             WHERE server_id = $1 AND status IN ('queued', 'running')",
        )
        .bind(server_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(Into::into))
    }

    async fn list_recent_jobs(
        &self,
        server_id: GameServerId,
        limit: i64,
    ) -> Result<Vec<GameServerUpdateJob>, DomainError> {
        let rows = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "SELECT * FROM game_server_update_jobs \
             WHERE server_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2",
        )
        .bind(server_id.as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn apply_progress(
        &self,
        server_id: GameServerId,
        progress: &UpdateProgress,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;

        let updated = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "UPDATE game_server_update_jobs SET status = 'running', \
                phase = COALESCE($3, phase), log_tail = COALESCE($4, log_tail), \
                started_at = COALESCE(started_at, NOW()), updated_at = NOW(), \
                finished_at = NULL, \
                error = CASE WHEN status = 'lost' THEN NULL ELSE error END \
             WHERE id = $1 AND server_id = $2 AND status IN ('queued', 'running', 'lost') \
               AND (status <> 'lost' OR NOT EXISTS ( \
                    SELECT 1 FROM game_server_update_jobs o \
                    WHERE o.server_id = $2 AND o.id <> $1 \
                      AND o.status IN ('queued', 'running'))) \
             RETURNING *",
        )
        .bind(progress.job_id.as_uuid())
        .bind(server_id.as_uuid())
        .bind(&progress.phase)
        .bind(&progress.log_tail)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?;
        if let Some(row) = updated {
            tx.commit().await.map_err(internal)?;
            return Ok(Some(row.into()));
        }

        // Known but not ours to move: its outcome is in, or it is another
        // server's.
        let known: Option<(uuid::Uuid,)> =
            sqlx::query_as("SELECT id FROM game_server_update_jobs WHERE id = $1")
                .bind(progress.job_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(internal)?;
        if known.is_some() {
            tx.rollback().await.map_err(internal)?;
            return Ok(None);
        }

        // A job the portal did not start (cron, SSH, auto-repair,
        // overrides). The helper holds one lock per host, so whatever the
        // portal still believes is RUNNING there is not. A QUEUED row is
        // left alone — its update_start is being answered right now — and
        // makes the insert below a no-op; the next frame tries again.
        sqlx::query(
            "UPDATE game_server_update_jobs SET status = 'lost', \
                error = 'superseded: the host reported a different job running', \
                finished_at = NOW(), updated_at = NOW() \
             WHERE server_id = $1 AND status = 'running'",
        )
        .bind(server_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(internal)?;

        let inserted = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "INSERT INTO game_server_update_jobs \
                (id, server_id, operation, status, trigger, phase, log_tail, started_at) \
             VALUES ($1, $2, $3, 'running', $4, $5, $6, NOW()) \
             ON CONFLICT DO NOTHING \
             RETURNING *",
        )
        .bind(progress.job_id.as_uuid())
        .bind(server_id.as_uuid())
        .bind(progress.operation.as_str())
        .bind(progress.trigger.as_str())
        .bind(&progress.phase)
        .bind(&progress.log_tail)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        Ok(inserted.map(Into::into))
    }

    async fn apply_result(
        &self,
        server_id: GameServerId,
        result: &UpdateResult,
    ) -> Result<ResultOutcome, DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;

        let existing = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "SELECT * FROM game_server_update_jobs WHERE id = $1 FOR UPDATE",
        )
        .bind(result.job_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .map(GameServerUpdateJob::from);

        let Some(existing) = existing else {
            // A job the portal did not start. Two deliveries racing here
            // settle on the primary key: the loser sees a duplicate.
            let inserted = sqlx::query_as::<_, GameServerUpdateJobRow>(
                "INSERT INTO game_server_update_jobs \
                    (id, server_id, operation, status, trigger, phase, failed_phase, \
                     before_versions, after_versions, latest_versions, load, config_report, \
                     restarted, error, log_tail, agent_synthesized, started_at, finished_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
                         $16, $17, COALESCE($18, NOW())) \
                 ON CONFLICT (id) DO NOTHING \
                 RETURNING *",
            )
            .bind(result.job_id.as_uuid())
            .bind(server_id.as_uuid())
            .bind(result.operation.as_str())
            .bind(result.status.as_str())
            .bind(result.trigger.as_str())
            .bind(&result.phase)
            .bind(&result.failed_phase)
            .bind(&result.before_versions)
            .bind(&result.after_versions)
            .bind(&result.latest_versions)
            .bind(&result.load)
            .bind(&result.config_report)
            .bind(result.restarted)
            .bind(&result.error)
            .bind(&result.log_tail)
            .bind(result.agent_synthesized)
            .bind(result.started_at)
            .bind(result.finished_at)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?;
            tx.commit().await.map_err(internal)?;
            return Ok(match inserted {
                Some(row) => ResultOutcome::Stored {
                    job: Box::new(row.into()),
                    first_outcome: true,
                },
                None => ResultOutcome::Duplicate,
            });
        };

        if existing.server_id != server_id {
            tx.rollback().await.map_err(internal)?;
            return Ok(ResultOutcome::Foreign);
        }
        if !existing.accepts_result(result.agent_synthesized) {
            tx.rollback().await.map_err(internal)?;
            return Ok(ResultOutcome::Duplicate);
        }
        let first_outcome =
            !existing.status.is_terminal() || existing.status == UpdateJobStatus::Lost;

        // Operation, trigger, requester and `forced` stay as first stored:
        // for a portal job they are the portal's own record.
        let row = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "UPDATE game_server_update_jobs SET \
                status = $2, phase = $3, failed_phase = $4, \
                before_versions = $5, after_versions = $6, latest_versions = $7, \
                load = $8, config_report = $9, restarted = $10, error = $11, \
                log_tail = COALESCE($12, log_tail), agent_synthesized = $13, \
                started_at = COALESCE($14, started_at), \
                finished_at = COALESCE($15, NOW()), updated_at = NOW() \
             WHERE id = $1 \
             RETURNING *",
        )
        .bind(result.job_id.as_uuid())
        .bind(result.status.as_str())
        .bind(&result.phase)
        .bind(&result.failed_phase)
        .bind(&result.before_versions)
        .bind(&result.after_versions)
        .bind(&result.latest_versions)
        .bind(&result.load)
        .bind(&result.config_report)
        .bind(result.restarted)
        .bind(&result.error)
        .bind(&result.log_tail)
        .bind(result.agent_synthesized)
        .bind(result.started_at)
        .bind(result.finished_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        Ok(ResultOutcome::Stored {
            job: Box::new(row.into()),
            first_outcome,
        })
    }

    async fn touch_job(
        &self,
        server_id: GameServerId,
        id: GameServerUpdateJobId,
        phase: Option<&str>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_server_update_jobs SET status = 'running', \
                phase = COALESCE($3, phase), started_at = COALESCE(started_at, NOW()), \
                updated_at = NOW(), finished_at = NULL, \
                error = CASE WHEN status = 'lost' THEN NULL ELSE error END \
             WHERE id = $1 AND server_id = $2 AND status IN ('queued', 'running', 'lost') \
               AND (status <> 'lost' OR NOT EXISTS ( \
                    SELECT 1 FROM game_server_update_jobs o \
                    WHERE o.server_id = $2 AND o.id <> $1 \
                      AND o.status IN ('queued', 'running')))",
        )
        .bind(id.as_uuid())
        .bind(server_id.as_uuid())
        .bind(phase)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn finish_job(
        &self,
        id: GameServerUpdateJobId,
        status: UpdateJobStatus,
        error: &str,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        if !status.is_terminal() {
            return Err(DomainError::Internal(format!(
                "finish_job needs a terminal status, got {status}"
            )));
        }
        let row = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "UPDATE game_server_update_jobs SET status = $2, error = $3, \
                finished_at = NOW(), updated_at = NOW() \
             WHERE id = $1 AND status IN ('queued', 'running') \
             RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(status.as_str())
        .bind(error)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(Into::into))
    }

    async fn list_stalled_jobs(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<GameServerUpdateJob>, DomainError> {
        let rows = sqlx::query_as::<_, GameServerUpdateJobRow>(
            "SELECT * FROM game_server_update_jobs \
             WHERE status IN ('queued', 'running') AND updated_at < $1 \
             ORDER BY updated_at ASC LIMIT $2",
        )
        .bind(cutoff)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
