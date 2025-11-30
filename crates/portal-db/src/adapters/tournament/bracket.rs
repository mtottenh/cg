//! `PostgreSQL` implementation of `TournamentBracketRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::entities::tournament::TournamentBracketRow;
use crate::DbPool;
use portal_core::types::BracketStatus;
use portal_core::{DomainError, TournamentBracketId, TournamentId, TournamentStageId};
use portal_domain::entities::tournament::TournamentBracket;
use portal_domain::repositories::tournament::{
    CreateTournamentBracket, TournamentBracketRepository, UpdateTournamentBracket,
};

/// `PostgreSQL` implementation of `TournamentBracketRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentBracketRepository {
    pool: DbPool,
}

impl PgTournamentBracketRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentBracketRepository for PgTournamentBracketRepository {
    async fn find_by_id(
        &self,
        id: TournamentBracketId,
    ) -> Result<Option<TournamentBracket>, DomainError> {
        let row = sqlx::query_as::<_, TournamentBracketRow>(
            "SELECT * FROM tournament_brackets WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentBracket::from))
    }

    async fn create(&self, cmd: CreateTournamentBracket) -> Result<TournamentBracket, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentBracketRow>(
            r"
            INSERT INTO tournament_brackets (
                id, stage_id, tournament_id, name, bracket_type,
                total_rounds, current_round, group_number, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, 1, $7, $8, $9)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.stage_id.as_uuid())
        .bind(cmd.tournament_id.as_uuid())
        .bind(&cmd.name)
        .bind(cmd.bracket_type.to_string())
        .bind(cmd.total_rounds)
        .bind(cmd.group_number)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentBracket::from(row))
    }

    async fn update(
        &self,
        id: TournamentBracketId,
        update: UpdateTournamentBracket,
    ) -> Result<TournamentBracket, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentBracketRow>(
            r"
            UPDATE tournament_brackets SET
                name = COALESCE($2, name),
                total_rounds = COALESCE($3, total_rounds),
                current_round = COALESCE($4, current_round),
                updated_at = $5
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(update.total_rounds)
        .bind(update.current_round)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentBracket::from(row))
    }

    async fn update_status(
        &self,
        id: TournamentBracketId,
        status: BracketStatus,
    ) -> Result<TournamentBracket, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentBracketRow>(
            r"
            UPDATE tournament_brackets SET status = $2, updated_at = $3
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

        Ok(TournamentBracket::from(row))
    }

    async fn advance_round(
        &self,
        id: TournamentBracketId,
    ) -> Result<TournamentBracket, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentBracketRow>(
            r"
            UPDATE tournament_brackets SET
                current_round = current_round + 1,
                updated_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentBracket::from(row))
    }

    async fn list_by_stage(
        &self,
        stage_id: TournamentStageId,
    ) -> Result<Vec<TournamentBracket>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentBracketRow>(
            r"
            SELECT * FROM tournament_brackets
            WHERE stage_id = $1
            ORDER BY group_number ASC NULLS FIRST, name ASC
            ",
        )
        .bind(stage_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentBracket::from).collect())
    }

    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentBracket>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentBracketRow>(
            r"
            SELECT * FROM tournament_brackets
            WHERE tournament_id = $1
            ORDER BY group_number ASC NULLS FIRST, name ASC
            ",
        )
        .bind(tournament_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentBracket::from).collect())
    }

    async fn delete(&self, id: TournamentBracketId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_brackets WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
