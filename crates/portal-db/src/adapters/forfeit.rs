//! Forfeit record repository adapter.

use crate::DbPool;
use crate::entities::ForfeitRecordRow;
use async_trait::async_trait;
use portal_core::{DomainError, ForfeitRecordId, TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::forfeit::{ForfeitRecord, ForfeitType};
use portal_domain::repositories::forfeit::{CreateForfeitRecord, ForfeitRecordRepository};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<ForfeitRecordRow> for ForfeitRecord {
    fn from(row: ForfeitRecordRow) -> Self {
        Self {
            id: ForfeitRecordId::from(row.id),
            match_id: TournamentMatchId::from(row.match_id),
            forfeiting_registration_id: TournamentRegistrationId::from(
                row.forfeiting_registration_id,
            ),
            forfeit_type: row.forfeit_type.parse().unwrap_or(ForfeitType::NoShow),
            reason: row.reason,
            triggered_by_user_id: row.triggered_by_user_id.map(portal_core::UserId::from),
            triggered_by_system: row.triggered_by_system,
            forfeited_at: row.forfeited_at,
        }
    }
}

// =============================================================================
// Forfeit Record Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `ForfeitRecordRepository` trait.
#[derive(Clone)]
pub struct PgForfeitRecordRepository {
    pool: DbPool,
}

impl PgForfeitRecordRepository {
    /// Create a new `PostgreSQL` forfeit record repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ForfeitRecordRepository for PgForfeitRecordRepository {
    /// Idempotent create: a second attempt for the same
    /// (match, forfeiting registration) returns the record that already
    /// exists instead of failing, so a forfeit whose match update never
    /// landed can be retried and recovered (see migration 0072).
    async fn create(&self, data: CreateForfeitRecord) -> Result<ForfeitRecord, DomainError> {
        let inserted = sqlx::query_as::<_, ForfeitRecordRow>(
            r"
            INSERT INTO forfeit_records (
                match_id, forfeiting_registration_id, forfeit_type, reason,
                triggered_by_user_id, triggered_by_system
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (match_id, forfeiting_registration_id) DO NOTHING
            RETURNING *
            ",
        )
        .bind(data.match_id.as_uuid())
        .bind(data.forfeiting_registration_id.as_uuid())
        .bind(data.forfeit_type.to_string())
        .bind(&data.reason)
        .bind(data.triggered_by_user_id.map(|id| id.as_uuid()))
        .bind(data.triggered_by_system)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let record = match inserted {
            Some(row) => row,
            None => sqlx::query_as::<_, ForfeitRecordRow>(
                r"
                SELECT * FROM forfeit_records
                WHERE match_id = $1 AND forfeiting_registration_id = $2
                ",
            )
            .bind(data.match_id.as_uuid())
            .bind(data.forfeiting_registration_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?,
        };

        Ok(ForfeitRecord::from(record))
    }

    async fn find_by_id(&self, id: ForfeitRecordId) -> Result<Option<ForfeitRecord>, DomainError> {
        let record =
            sqlx::query_as::<_, ForfeitRecordRow>("SELECT * FROM forfeit_records WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(record.map(ForfeitRecord::from))
    }

    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ForfeitRecord>, DomainError> {
        let record = sqlx::query_as::<_, ForfeitRecordRow>(
            "SELECT * FROM forfeit_records WHERE match_id = $1 ORDER BY forfeited_at DESC LIMIT 1",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(record.map(ForfeitRecord::from))
    }

    async fn find_by_registration(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<ForfeitRecord>, DomainError> {
        let records = sqlx::query_as::<_, ForfeitRecordRow>(
            r"
            SELECT * FROM forfeit_records
            WHERE forfeiting_registration_id = $1
            ORDER BY forfeited_at DESC
            ",
        )
        .bind(registration_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(records.into_iter().map(ForfeitRecord::from).collect())
    }

    async fn exists_for_match(&self, match_id: TournamentMatchId) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM forfeit_records WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }
}
