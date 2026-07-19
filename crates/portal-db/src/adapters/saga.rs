//! PostgreSQL implementations of SagaExecutionRepository and ProgressionLogRepository.

use crate::DbPool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{DomainError, SagaId, TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::saga::{SagaExecution, SagaStatus, StepRecord};
use portal_domain::repositories::evidence::{
    CreateProgressionLog, CreateSagaExecution, ProgressionLog, ProgressionLogRepository,
    ProgressionType, SagaExecutionRepository,
};
use sqlx::FromRow;

// =============================================================================
// SAGA EXECUTION REPOSITORY
// =============================================================================

/// PostgreSQL implementation of SagaExecutionRepository.
#[derive(Debug, Clone)]
pub struct PgSagaExecutionRepository {
    pool: DbPool,
}

impl PgSagaExecutionRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// Database row for saga_executions table.
#[derive(Debug, FromRow)]
struct SagaExecutionRow {
    id: uuid::Uuid,
    saga_type: String,
    saga_version: i32,
    tournament_id: Option<uuid::Uuid>,
    match_id: Option<uuid::Uuid>,
    correlation_id: Option<String>,
    input_data: serde_json::Value,
    current_step: i32,
    status: String,
    step_history: serde_json::Value,
    last_error: Option<String>,
    retry_count: i32,
    max_retries: i32,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn row_to_saga(row: SagaExecutionRow) -> Result<SagaExecution, DomainError> {
    let status: SagaStatus = row
        .status
        .parse()
        .map_err(|e: String| DomainError::Internal(e))?;

    let step_history: Vec<StepRecord> = serde_json::from_value(row.step_history)
        .map_err(|e| DomainError::Internal(format!("Failed to parse step history: {e}")))?;

    Ok(SagaExecution {
        id: SagaId::from(row.id),
        saga_type: row.saga_type,
        saga_version: row.saga_version,
        tournament_id: row.tournament_id.map(portal_core::TournamentId::from),
        match_id: row.match_id.map(TournamentMatchId::from),
        correlation_id: row.correlation_id,
        input_data: row.input_data,
        current_step: row.current_step,
        status,
        step_history,
        last_error: row.last_error,
        retry_count: row.retry_count,
        max_retries: row.max_retries,
        started_at: row.started_at,
        completed_at: row.completed_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[async_trait]
impl SagaExecutionRepository for PgSagaExecutionRepository {
    async fn find_by_id(&self, id: SagaId) -> Result<Option<SagaExecution>, DomainError> {
        let row =
            sqlx::query_as::<_, SagaExecutionRow>("SELECT * FROM saga_executions WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(format!("Failed to find saga: {e}")))?;

        row.map(row_to_saga).transpose()
    }

    async fn create(&self, saga: CreateSagaExecution) -> Result<SagaExecution, DomainError> {
        let row = sqlx::query_as::<_, SagaExecutionRow>(
            r"INSERT INTO saga_executions (saga_type, saga_version, tournament_id, match_id, correlation_id, input_data, max_retries, status)
              VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')
              RETURNING *",
        )
        .bind(&saga.saga_type)
        .bind(saga.saga_version)
        .bind(saga.tournament_id.map(|id| id.as_uuid()))
        .bind(saga.match_id.map(|id| id.as_uuid()))
        .bind(&saga.correlation_id)
        .bind(&saga.input_data)
        .bind(saga.max_retries)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            // Partial unique index uq_saga_executions_live_per_match:
            // another live saga of this type already exists for the match
            // (e.g. two racing confirms). Surface as Conflict so callers
            // return 409 instead of double-running the completion.
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.constraint() == Some("uq_saga_executions_live_per_match")
            {
                return DomainError::Conflict(
                    "A completion is already in progress for this match".to_string(),
                );
            }
            DomainError::Internal(format!("Failed to create saga: {e}"))
        })?;

        row_to_saga(row)
    }

    async fn update(&self, saga: &SagaExecution) -> Result<SagaExecution, DomainError> {
        let step_history_json = serde_json::to_value(&saga.step_history)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize step history: {e}")))?;

        let row = sqlx::query_as::<_, SagaExecutionRow>(
            r"UPDATE saga_executions
              SET current_step = $2, status = $3, step_history = $4, last_error = $5,
                  retry_count = $6, started_at = $7, completed_at = $8
              WHERE id = $1
              RETURNING *",
        )
        .bind(saga.id.as_uuid())
        .bind(saga.current_step)
        .bind(saga.status.to_string())
        .bind(&step_history_json)
        .bind(&saga.last_error)
        .bind(saga.retry_count)
        .bind(saga.started_at)
        .bind(saga.completed_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to update saga: {e}")))?;

        row_to_saga(row)
    }

    async fn update_status(
        &self,
        id: SagaId,
        status: SagaStatus,
    ) -> Result<SagaExecution, DomainError> {
        let row = sqlx::query_as::<_, SagaExecutionRow>(
            r"UPDATE saga_executions SET status = $2 WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to update saga status: {e}")))?;

        row_to_saga(row)
    }

    async fn find_stuck(
        &self,
        running_since_before: DateTime<Utc>,
    ) -> Result<Vec<SagaExecution>, DomainError> {
        let rows = sqlx::query_as::<_, SagaExecutionRow>(
            r"SELECT * FROM saga_executions WHERE status = 'running' AND started_at < $1",
        )
        .bind(running_since_before)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find stuck sagas: {e}")))?;

        rows.into_iter().map(row_to_saga).collect()
    }

    async fn find_pending(&self) -> Result<Vec<SagaExecution>, DomainError> {
        let rows = sqlx::query_as::<_, SagaExecutionRow>(
            r"SELECT * FROM saga_executions WHERE status = 'pending' ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find pending sagas: {e}")))?;

        rows.into_iter().map(row_to_saga).collect()
    }

    async fn find_by_status(&self, status: SagaStatus) -> Result<Vec<SagaExecution>, DomainError> {
        let rows = sqlx::query_as::<_, SagaExecutionRow>(
            r"SELECT * FROM saga_executions WHERE status = $1 ORDER BY created_at ASC",
        )
        .bind(status.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find sagas by status: {e}")))?;

        rows.into_iter().map(row_to_saga).collect()
    }

    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<SagaExecution>, DomainError> {
        let rows = sqlx::query_as::<_, SagaExecutionRow>(
            r"SELECT * FROM saga_executions WHERE match_id = $1 ORDER BY created_at DESC",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find sagas by match: {e}")))?;

        rows.into_iter().map(row_to_saga).collect()
    }

    async fn find_by_tournament(
        &self,
        tournament_id: portal_core::TournamentId,
    ) -> Result<Vec<SagaExecution>, DomainError> {
        let rows = sqlx::query_as::<_, SagaExecutionRow>(
            r"SELECT * FROM saga_executions WHERE tournament_id = $1 ORDER BY created_at DESC",
        )
        .bind(tournament_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find sagas by tournament: {e}")))?;

        rows.into_iter().map(row_to_saga).collect()
    }
}

// =============================================================================
// PROGRESSION LOG REPOSITORY
// =============================================================================

/// PostgreSQL implementation of ProgressionLogRepository.
#[derive(Debug, Clone)]
pub struct PgProgressionLogRepository {
    pool: DbPool,
}

impl PgProgressionLogRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// Database row for progression_log table.
#[derive(Debug, FromRow)]
struct ProgressionLogRow {
    id: uuid::Uuid,
    source_match_id: uuid::Uuid,
    target_match_id: Option<uuid::Uuid>,
    registration_id: uuid::Uuid,
    progression_type: String,
    target_position: Option<i32>,
    saga_id: Option<uuid::Uuid>,
    progressed_at: DateTime<Utc>,
}

fn parse_progression_type(s: &str) -> Result<ProgressionType, DomainError> {
    match s {
        "winner_advance" => Ok(ProgressionType::WinnerAdvance),
        "loser_drop" => Ok(ProgressionType::LoserDrop),
        "loser_eliminate" => Ok(ProgressionType::LoserEliminate),
        "bye_advance" => Ok(ProgressionType::ByeAdvance),
        _ => Err(DomainError::Internal(format!(
            "Unknown progression type: {s}"
        ))),
    }
}

fn progression_type_to_str(pt: ProgressionType) -> &'static str {
    match pt {
        ProgressionType::WinnerAdvance => "winner_advance",
        ProgressionType::LoserDrop => "loser_drop",
        ProgressionType::LoserEliminate => "loser_eliminate",
        ProgressionType::ByeAdvance => "bye_advance",
    }
}

fn row_to_progression_log(row: ProgressionLogRow) -> Result<ProgressionLog, DomainError> {
    Ok(ProgressionLog {
        id: row.id,
        source_match_id: TournamentMatchId::from(row.source_match_id),
        target_match_id: row.target_match_id.map(TournamentMatchId::from),
        registration_id: TournamentRegistrationId::from(row.registration_id),
        progression_type: parse_progression_type(&row.progression_type)?,
        target_position: row.target_position,
        saga_id: row.saga_id.map(SagaId::from),
        progressed_at: row.progressed_at,
    })
}

#[async_trait]
impl ProgressionLogRepository for PgProgressionLogRepository {
    async fn log(&self, log: CreateProgressionLog) -> Result<ProgressionLog, DomainError> {
        let row = sqlx::query_as::<_, ProgressionLogRow>(
            r"INSERT INTO progression_log (source_match_id, target_match_id, registration_id, progression_type, target_position, saga_id)
              VALUES ($1, $2, $3, $4, $5, $6)
              RETURNING *",
        )
        .bind(log.source_match_id.as_uuid())
        .bind(log.target_match_id.map(|id| id.as_uuid()))
        .bind(log.registration_id.as_uuid())
        .bind(progression_type_to_str(log.progression_type))
        .bind(log.target_position)
        .bind(log.saga_id.map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to create progression log: {e}")))?;

        row_to_progression_log(row)
    }

    async fn find_by_source_match(
        &self,
        source_match_id: TournamentMatchId,
    ) -> Result<Vec<ProgressionLog>, DomainError> {
        let rows = sqlx::query_as::<_, ProgressionLogRow>(
            r"SELECT * FROM progression_log WHERE source_match_id = $1 ORDER BY progressed_at ASC",
        )
        .bind(source_match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find progression logs: {e}")))?;

        rows.into_iter().map(row_to_progression_log).collect()
    }

    async fn find_by_target_match(
        &self,
        target_match_id: TournamentMatchId,
    ) -> Result<Vec<ProgressionLog>, DomainError> {
        let rows = sqlx::query_as::<_, ProgressionLogRow>(
            r"SELECT * FROM progression_log WHERE target_match_id = $1 ORDER BY progressed_at ASC",
        )
        .bind(target_match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find progression logs: {e}")))?;

        rows.into_iter().map(row_to_progression_log).collect()
    }

    async fn find_by_saga(&self, saga_id: SagaId) -> Result<Vec<ProgressionLog>, DomainError> {
        let rows = sqlx::query_as::<_, ProgressionLogRow>(
            r"SELECT * FROM progression_log WHERE saga_id = $1 ORDER BY progressed_at ASC",
        )
        .bind(saga_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find progression logs: {e}")))?;

        rows.into_iter().map(row_to_progression_log).collect()
    }

    async fn delete_by_source_match(
        &self,
        source_match_id: TournamentMatchId,
    ) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM progression_log WHERE source_match_id = $1")
            .bind(source_match_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::Internal(format!("Failed to delete progression logs: {e}"))
            })?;

        Ok(())
    }
}
