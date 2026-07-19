//! `PostgreSQL` implementation of `TournamentMapPoolRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::tournament::TournamentMapPoolRow;
use portal_core::{DomainError, TournamentId, TournamentMapPoolId, TournamentStageId};
use portal_domain::entities::tournament::TournamentMapPool;
use portal_domain::repositories::tournament::{
    TournamentMapPoolRepository, UpsertTournamentMapPool,
};

/// `PostgreSQL` implementation of `TournamentMapPoolRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentMapPoolRepository {
    pool: DbPool,
}

impl PgTournamentMapPoolRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentMapPoolRepository for PgTournamentMapPoolRepository {
    async fn find_by_id(
        &self,
        id: TournamentMapPoolId,
    ) -> Result<Option<TournamentMapPool>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMapPoolRow>(
            "SELECT * FROM tournament_map_pools WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMapPool::from))
    }

    async fn find_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Option<TournamentMapPool>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMapPoolRow>(
            r"
            SELECT * FROM tournament_map_pools
            WHERE tournament_id = $1 AND stage_id IS NULL
            ",
        )
        .bind(tournament_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMapPool::from))
    }

    async fn find_by_stage(
        &self,
        stage_id: TournamentStageId,
    ) -> Result<Option<TournamentMapPool>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMapPoolRow>(
            r"
            SELECT * FROM tournament_map_pools
            WHERE stage_id = $1
            ",
        )
        .bind(stage_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMapPool::from))
    }

    async fn get_effective(
        &self,
        tournament_id: TournamentId,
        stage_id: Option<TournamentStageId>,
    ) -> Result<Option<TournamentMapPool>, DomainError> {
        // First try stage-specific pool, then fall back to tournament default
        if let Some(stage_id) = stage_id {
            if let Some(pool) = self.find_by_stage(stage_id).await? {
                return Ok(Some(pool));
            }
        }

        self.find_by_tournament(tournament_id).await
    }

    async fn upsert(&self, cmd: UpsertTournamentMapPool) -> Result<TournamentMapPool, DomainError> {
        let now = Utc::now();

        // Check if exists
        let existing = if let Some(stage_id) = cmd.stage_id {
            self.find_by_stage(stage_id).await?
        } else {
            self.find_by_tournament(cmd.tournament_id).await?
        };

        if let Some(existing) = existing {
            // Update existing
            let row = sqlx::query_as::<_, TournamentMapPoolRow>(
                r"
                UPDATE tournament_map_pools SET
                    maps = $2,
                    veto_format_id = $3,
                    updated_at = $4
                WHERE id = $1
                RETURNING *
                ",
            )
            .bind(existing.id.as_uuid())
            .bind(&cmd.maps)
            .bind(&cmd.veto_format_id)
            .bind(now)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

            Ok(TournamentMapPool::from(row))
        } else {
            // Insert new
            let id = uuid::Uuid::now_v7();

            let row = sqlx::query_as::<_, TournamentMapPoolRow>(
                r"
                INSERT INTO tournament_map_pools (
                    id, tournament_id, stage_id, maps, veto_format_id, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING *
                ",
            )
            .bind(id)
            .bind(cmd.tournament_id.as_uuid())
            .bind(cmd.stage_id.map(|id| id.as_uuid()))
            .bind(&cmd.maps)
            .bind(&cmd.veto_format_id)
            .bind(now)
            .bind(now)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

            Ok(TournamentMapPool::from(row))
        }
    }

    async fn delete(&self, id: TournamentMapPoolId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_map_pools WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
