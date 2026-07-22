//! `PostgreSQL` implementation of `LeagueSeasonParticipantRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::league_team::LeagueSeasonParticipantRow;
use portal_core::{DomainError, LeagueSeasonId, PlayerId};
use portal_domain::entities::league_team::LeagueSeasonParticipant;
use portal_domain::repositories::league_team::{
    LeagueSeasonParticipantRepository, RegisterLeagueSeasonParticipant,
};

/// `PostgreSQL` implementation of `LeagueSeasonParticipantRepository`.
#[derive(Debug, Clone)]
pub struct PgLeagueSeasonParticipantRepository {
    pool: DbPool,
}

impl PgLeagueSeasonParticipantRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LeagueSeasonParticipantRepository for PgLeagueSeasonParticipantRepository {
    async fn find_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<LeagueSeasonParticipant>, DomainError> {
        let row = sqlx::query_as::<_, LeagueSeasonParticipantRow>(
            "SELECT * FROM league_season_participants WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueSeasonParticipant::from))
    }

    async fn find_by_season_and_player(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueSeasonParticipant>, DomainError> {
        let row = sqlx::query_as::<_, LeagueSeasonParticipantRow>(
            "SELECT * FROM league_season_participants WHERE season_id = $1 AND player_id = $2",
        )
        .bind(season_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueSeasonParticipant::from))
    }

    async fn register(
        &self,
        cmd: RegisterLeagueSeasonParticipant,
    ) -> Result<LeagueSeasonParticipant, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueSeasonParticipantRow>(
            r"
            INSERT INTO league_season_participants (
                id, season_id, player_id, registered_at
            )
            VALUES ($1, $2, $3, $4)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.season_id.as_uuid())
        .bind(cmd.player_id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueSeasonParticipant::from(row))
    }

    async fn list_by_season(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueSeasonParticipant>, i64), DomainError> {
        let rows = if let Some(status) = &status_filter {
            sqlx::query_as::<_, LeagueSeasonParticipantRow>(
                r"
                SELECT * FROM league_season_participants
                WHERE season_id = $1 AND status = $2
                ORDER BY registered_at ASC
                LIMIT $3 OFFSET $4
                ",
            )
            .bind(season_id.as_uuid())
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, LeagueSeasonParticipantRow>(
                r"
                SELECT * FROM league_season_participants
                WHERE season_id = $1
                ORDER BY registered_at ASC
                LIMIT $2 OFFSET $3
                ",
            )
            .bind(season_id.as_uuid())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM league_season_participants WHERE season_id = $1")
                .bind(season_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((
            rows.into_iter()
                .map(LeagueSeasonParticipant::from)
                .collect(),
            count.0,
        ))
    }

    async fn update_status(
        &self,
        id: uuid::Uuid,
        status: String,
    ) -> Result<LeagueSeasonParticipant, DomainError> {
        let now = Utc::now();
        let withdrawn_at = if status == "withdrawn" {
            Some(now)
        } else {
            None
        };

        let row = sqlx::query_as::<_, LeagueSeasonParticipantRow>(
            r"
            UPDATE league_season_participants SET
                status = $2,
                withdrawn_at = COALESCE($3, withdrawn_at)
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id)
        .bind(&status)
        .bind(withdrawn_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueSeasonParticipant::from(row))
    }

    async fn withdraw(&self, id: uuid::Uuid) -> Result<LeagueSeasonParticipant, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueSeasonParticipantRow>(
            r"
            UPDATE league_season_participants SET
                status = 'withdrawn',
                withdrawn_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueSeasonParticipant::from(row))
    }

    async fn is_registered(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_season_participants WHERE season_id = $1 AND player_id = $2 AND status NOT IN ('withdrawn', 'disqualified')",
        )
        .bind(season_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }
}
