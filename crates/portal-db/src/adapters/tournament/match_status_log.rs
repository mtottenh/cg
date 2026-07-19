//! PostgreSQL implementation of MatchStatusLogRepository.

use crate::DbPool;
use crate::entities::{MatchStatusLogRow, NewMatchStatusLog};
use async_trait::async_trait;
use portal_core::types::TournamentMatchStatus;
use portal_core::{DomainError, MatchStatusLogId, TournamentMatchId, UserId};
use portal_domain::entities::MatchStatusLog;
use portal_domain::repositories::{CreateMatchStatusLog, MatchStatusLogRepository};

/// PostgreSQL implementation of MatchStatusLogRepository.
#[derive(Debug, Clone)]
pub struct PgMatchStatusLogRepository {
    pool: DbPool,
}

impl PgMatchStatusLogRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MatchStatusLogRepository for PgMatchStatusLogRepository {
    async fn create(&self, log: CreateMatchStatusLog) -> Result<MatchStatusLog, DomainError> {
        let new_log = NewMatchStatusLog {
            match_id: log.match_id.as_uuid(),
            from_status: log.from_status.to_string(),
            to_status: log.to_status.to_string(),
            transition_reason: log.transition_reason,
            triggered_by_user_id: log.triggered_by_user_id.map(|id| id.as_uuid()),
            triggered_by_system: log.triggered_by_system,
            metadata: log.metadata,
        };

        let row = sqlx::query_as::<_, MatchStatusLogRow>(
            r"
            INSERT INTO match_status_log (
                match_id, from_status, to_status, transition_reason,
                triggered_by_user_id, triggered_by_system, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
        )
        .bind(new_log.match_id)
        .bind(&new_log.from_status)
        .bind(&new_log.to_status)
        .bind(&new_log.transition_reason)
        .bind(new_log.triggered_by_user_id)
        .bind(new_log.triggered_by_system)
        .bind(&new_log.metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to create match status log: {e}")))?;

        row_to_domain(row)
    }

    async fn find_by_id(
        &self,
        id: MatchStatusLogId,
    ) -> Result<Option<MatchStatusLog>, DomainError> {
        let row =
            sqlx::query_as::<_, MatchStatusLogRow>(r"SELECT * FROM match_status_log WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("Failed to find match status log: {e}"))
                })?;

        row.map(row_to_domain).transpose()
    }

    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchStatusLog>, DomainError> {
        let rows = sqlx::query_as::<_, MatchStatusLogRow>(
            r"
            SELECT * FROM match_status_log
            WHERE match_id = $1
            ORDER BY transitioned_at ASC
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find match status logs: {e}")))?;

        rows.into_iter().map(row_to_domain).collect()
    }

    async fn find_latest_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<MatchStatusLog>, DomainError> {
        let row = sqlx::query_as::<_, MatchStatusLogRow>(
            r"
            SELECT * FROM match_status_log
            WHERE match_id = $1
            ORDER BY transitioned_at DESC
            LIMIT 1
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            DomainError::Internal(format!("Failed to find latest match status log: {e}"))
        })?;

        row.map(row_to_domain).transpose()
    }

    async fn count_by_match_id(&self, match_id: TournamentMatchId) -> Result<i64, DomainError> {
        let count: (i64,) =
            sqlx::query_as(r"SELECT COUNT(*) FROM match_status_log WHERE match_id = $1")
                .bind(match_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("Failed to count match status logs: {e}"))
                })?;

        Ok(count.0)
    }
}

/// Convert a database row to a domain entity.
fn row_to_domain(row: MatchStatusLogRow) -> Result<MatchStatusLog, DomainError> {
    let from_status: TournamentMatchStatus = row
        .from_status
        .parse()
        .map_err(|e: String| DomainError::Internal(format!("Invalid from_status: {e}")))?;

    let to_status: TournamentMatchStatus = row
        .to_status
        .parse()
        .map_err(|e: String| DomainError::Internal(format!("Invalid to_status: {e}")))?;

    Ok(MatchStatusLog {
        id: MatchStatusLogId::from_uuid(row.id),
        match_id: TournamentMatchId::from_uuid(row.match_id),
        from_status,
        to_status,
        transition_reason: row.transition_reason,
        triggered_by_user_id: row.triggered_by_user_id.map(UserId::from_uuid),
        triggered_by_system: row.triggered_by_system,
        metadata: row.metadata,
        transitioned_at: row.transitioned_at,
    })
}
