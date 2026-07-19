//! `PostgreSQL` implementation of `TournamentStageRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::tournament::TournamentStageRow;
use portal_core::types::StageStatus;
use portal_core::{DomainError, TournamentId, TournamentStageId};
use portal_domain::entities::tournament::TournamentStage;
use portal_domain::repositories::tournament::{
    CreateTournamentStage, TournamentStageRepository, UpdateTournamentStage,
};

/// `PostgreSQL` implementation of `TournamentStageRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentStageRepository {
    pool: DbPool,
}

impl PgTournamentStageRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentStageRepository for PgTournamentStageRepository {
    async fn find_by_id(
        &self,
        id: TournamentStageId,
    ) -> Result<Option<TournamentStage>, DomainError> {
        let row = sqlx::query_as::<_, TournamentStageRow>(
            "SELECT * FROM tournament_stages WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentStage::from))
    }

    async fn create(&self, cmd: CreateTournamentStage) -> Result<TournamentStage, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentStageRow>(
            r"
            INSERT INTO tournament_stages (
                id, tournament_id, name, stage_order, format, format_settings,
                advancement_count, advancement_rule, match_format, map_veto_format,
                starts_at, ends_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.tournament_id.as_uuid())
        .bind(&cmd.name)
        .bind(cmd.stage_order)
        .bind(cmd.format.to_string())
        .bind(&cmd.format_settings)
        .bind(cmd.advancement_count)
        .bind(cmd.advancement_rule.to_string())
        .bind(cmd.match_format.map(|f| f.to_string()))
        .bind(&cmd.map_veto_format)
        .bind(cmd.starts_at)
        .bind(cmd.ends_at)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentStage::from(row))
    }

    async fn update(
        &self,
        id: TournamentStageId,
        update: UpdateTournamentStage,
    ) -> Result<TournamentStage, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentStageRow>(
            r"
            UPDATE tournament_stages SET
                name = COALESCE($2, name),
                format_settings = COALESCE($3, format_settings),
                advancement_count = COALESCE($4, advancement_count),
                advancement_rule = COALESCE($5, advancement_rule),
                match_format = COALESCE($6, match_format),
                map_veto_format = COALESCE($7, map_veto_format),
                starts_at = COALESCE($8, starts_at),
                ends_at = COALESCE($9, ends_at),
                updated_at = $10
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.format_settings)
        .bind(update.advancement_count)
        .bind(update.advancement_rule.map(|r| r.to_string()))
        .bind(update.match_format.map(|f| f.to_string()))
        .bind(&update.map_veto_format)
        .bind(update.starts_at)
        .bind(update.ends_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentStage::from(row))
    }

    async fn update_status(
        &self,
        id: TournamentStageId,
        status: StageStatus,
    ) -> Result<TournamentStage, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentStageRow>(
            r"
            UPDATE tournament_stages SET status = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentStage::from(row))
    }

    async fn transition_stages(
        &self,
        from_stage_id: TournamentStageId,
        from_status: StageStatus,
        to_stage_id: TournamentStageId,
        to_status: StageStatus,
    ) -> Result<(TournamentStage, TournamentStage), DomainError> {
        // Both updates share one transaction. If either fails the tx
        // drops without committing, so the tournament can never be
        // left with a Completed-but-no-next-stage gap. See audit I5.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let now = Utc::now();

        let from_row = sqlx::query_as::<_, TournamentStageRow>(
            r"
            UPDATE tournament_stages SET status = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(from_stage_id.as_uuid())
        .bind(from_status.to_string())
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let to_row = sqlx::query_as::<_, TournamentStageRow>(
            r"
            UPDATE tournament_stages SET status = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(to_stage_id.as_uuid())
        .bind(to_status.to_string())
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((
            TournamentStage::from(from_row),
            TournamentStage::from(to_row),
        ))
    }

    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentStage>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentStageRow>(
            r"
            SELECT * FROM tournament_stages
            WHERE tournament_id = $1
            ORDER BY stage_order ASC
            ",
        )
        .bind(tournament_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentStage::from).collect())
    }

    async fn find_next_stage(
        &self,
        tournament_id: TournamentId,
        current_order: i32,
    ) -> Result<Option<TournamentStage>, DomainError> {
        let row = sqlx::query_as::<_, TournamentStageRow>(
            r"
            SELECT * FROM tournament_stages
            WHERE tournament_id = $1 AND stage_order > $2
            ORDER BY stage_order ASC
            LIMIT 1
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(current_order)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentStage::from))
    }

    async fn delete(&self, id: TournamentStageId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_stages WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
