//! `PostgreSQL` implementation of `TournamentMatchGameRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::tournament::TournamentMatchGameRow;
use portal_core::{
    DomainError, TournamentMatchGameId, TournamentMatchId, TournamentRegistrationId,
};
use portal_domain::entities::tournament::{GameStatus, TournamentMatchGame};
use portal_domain::repositories::tournament::{
    CreateTournamentMatchGame, TournamentMatchGameRepository, UpdateTournamentMatchGame,
};

/// `PostgreSQL` implementation of `TournamentMatchGameRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentMatchGameRepository {
    pool: DbPool,
}

impl PgTournamentMatchGameRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentMatchGameRepository for PgTournamentMatchGameRepository {
    async fn find_by_id(
        &self,
        id: TournamentMatchGameId,
    ) -> Result<Option<TournamentMatchGame>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            "SELECT * FROM tournament_match_games WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMatchGame::from))
    }

    async fn find_by_number(
        &self,
        match_id: TournamentMatchId,
        game_number: i32,
    ) -> Result<Option<TournamentMatchGame>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            SELECT * FROM tournament_match_games
            WHERE match_id = $1 AND game_number = $2
            ",
        )
        .bind(match_id.as_uuid())
        .bind(game_number)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMatchGame::from))
    }

    async fn create(
        &self,
        cmd: CreateTournamentMatchGame,
    ) -> Result<TournamentMatchGame, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            INSERT INTO tournament_match_games (
                id, match_id, game_number, map_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.match_id.as_uuid())
        .bind(cmd.game_number)
        .bind(&cmd.map_id)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatchGame::from(row))
    }

    async fn update(
        &self,
        id: TournamentMatchGameId,
        update: UpdateTournamentMatchGame,
    ) -> Result<TournamentMatchGame, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            UPDATE tournament_match_games SET
                map_id = COALESCE($2, map_id),
                map_picked_by = COALESCE($3, map_picked_by),
                side_selection_by = COALESCE($4, side_selection_by),
                game_data = COALESCE($5, game_data),
                updated_at = $6
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.map_id)
        .bind(update.map_picked_by.map(|id| id.as_uuid()))
        .bind(update.side_selection_by.map(|id| id.as_uuid()))
        .bind(&update.game_data)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatchGame::from(row))
    }

    async fn update_status(
        &self,
        id: TournamentMatchGameId,
        status: GameStatus,
    ) -> Result<TournamentMatchGame, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            UPDATE tournament_match_games SET status = $2, updated_at = $3
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

        Ok(TournamentMatchGame::from(row))
    }

    async fn set_map(
        &self,
        id: TournamentMatchGameId,
        map_id: String,
        picked_by: Option<TournamentRegistrationId>,
    ) -> Result<TournamentMatchGame, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            UPDATE tournament_match_games SET
                map_id = $2,
                map_picked_by = $3,
                updated_at = $4
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&map_id)
        .bind(picked_by.map(|id| id.as_uuid()))
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatchGame::from(row))
    }

    async fn submit_result(
        &self,
        id: TournamentMatchGameId,
        participant1_score: i32,
        participant2_score: i32,
        winner_id: TournamentRegistrationId,
        duration_seconds: Option<i32>,
        game_data: Option<serde_json::Value>,
    ) -> Result<TournamentMatchGame, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            UPDATE tournament_match_games SET
                participant1_score = $2,
                participant2_score = $3,
                winner_registration_id = $4,
                duration_seconds = $5,
                game_data = COALESCE($6, game_data),
                completed_at = $7,
                status = 'completed',
                updated_at = $7
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(participant1_score)
        .bind(participant2_score)
        .bind(winner_id.as_uuid())
        .bind(duration_seconds)
        .bind(game_data)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatchGame::from(row))
    }

    async fn start(&self, id: TournamentMatchGameId) -> Result<TournamentMatchGame, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            UPDATE tournament_match_games SET
                started_at = $2,
                status = 'in_progress',
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

        Ok(TournamentMatchGame::from(row))
    }

    async fn list_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<TournamentMatchGame>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchGameRow>(
            r"
            SELECT * FROM tournament_match_games
            WHERE match_id = $1
            ORDER BY game_number ASC
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatchGame::from).collect())
    }

    async fn count_completed(&self, match_id: TournamentMatchId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM tournament_match_games
            WHERE match_id = $1 AND status = 'completed'
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn create_for_match(
        &self,
        match_id: TournamentMatchId,
        maps_required: i32,
    ) -> Result<Vec<TournamentMatchGame>, DomainError> {
        let mut games = Vec::with_capacity(maps_required as usize);

        for game_number in 1..=maps_required {
            let game = self
                .create(CreateTournamentMatchGame {
                    match_id,
                    game_number,
                    map_id: None,
                })
                .await?;
            games.push(game);
        }

        Ok(games)
    }

    async fn delete(&self, id: TournamentMatchGameId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_match_games WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn delete_by_match(&self, match_id: TournamentMatchId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_match_games WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
