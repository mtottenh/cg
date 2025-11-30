//! `PostgreSQL` implementation of `LeagueTeamSeasonRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::entities::league_team::{LeagueTeamSeasonRow, LeagueTeamSummaryRow};
use crate::DbPool;
use portal_core::types::LeagueTeamSeasonStatus;
use portal_core::{DomainError, LeagueSeasonId, LeagueTeamId, LeagueTeamSeasonId};
use portal_domain::entities::league_team::{LeagueTeamSeason, LeagueTeamSummary};
use portal_domain::repositories::league_team::{
    CreateLeagueTeamSeason, LeagueTeamSeasonRepository, UpdateLeagueTeamSeason,
};

/// `PostgreSQL` implementation of `LeagueTeamSeasonRepository`.
#[derive(Debug, Clone)]
pub struct PgLeagueTeamSeasonRepository {
    pool: DbPool,
}

impl PgLeagueTeamSeasonRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LeagueTeamSeasonRepository for PgLeagueTeamSeasonRepository {
    async fn find_by_id(
        &self,
        id: LeagueTeamSeasonId,
    ) -> Result<Option<LeagueTeamSeason>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            "SELECT * FROM league_team_seasons WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeamSeason::from))
    }

    async fn find_by_team_and_season(
        &self,
        team_id: LeagueTeamId,
        season_id: LeagueSeasonId,
    ) -> Result<Option<LeagueTeamSeason>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            "SELECT * FROM league_team_seasons WHERE team_id = $1 AND season_id = $2",
        )
        .bind(team_id.as_uuid())
        .bind(season_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeamSeason::from))
    }

    async fn create(
        &self,
        cmd: CreateLeagueTeamSeason,
    ) -> Result<LeagueTeamSeason, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            r"
            INSERT INTO league_team_seasons (
                id, team_id, season_id, status, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.team_id.as_uuid())
        .bind(cmd.season_id.as_uuid())
        .bind("forming")
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamSeason::from(row))
    }

    async fn list_by_season(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<LeagueTeamSeasonStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeamSeason>, i64), DomainError> {
        let search_pattern = search.as_ref().map(|s| format!("%{}%", s.to_lowercase()));

        let rows = match (&status_filter, &search_pattern) {
            (Some(status), Some(pattern)) => {
                sqlx::query_as::<_, LeagueTeamSeasonRow>(
                    r"
                    SELECT lts.* FROM league_team_seasons lts
                    JOIN league_teams lt ON lt.id = lts.team_id
                    WHERE lts.season_id = $1 AND lts.status = $2
                      AND (LOWER(lt.name) LIKE $3 OR LOWER(lt.tag) LIKE $3)
                    ORDER BY lts.created_at ASC
                    LIMIT $4 OFFSET $5
                    ",
                )
                .bind(season_id.as_uuid())
                .bind(status.to_string())
                .bind(pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            }
            (Some(status), None) => {
                sqlx::query_as::<_, LeagueTeamSeasonRow>(
                    r"
                    SELECT * FROM league_team_seasons
                    WHERE season_id = $1 AND status = $2
                    ORDER BY created_at ASC
                    LIMIT $3 OFFSET $4
                    ",
                )
                .bind(season_id.as_uuid())
                .bind(status.to_string())
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            }
            (None, Some(pattern)) => {
                sqlx::query_as::<_, LeagueTeamSeasonRow>(
                    r"
                    SELECT lts.* FROM league_team_seasons lts
                    JOIN league_teams lt ON lt.id = lts.team_id
                    WHERE lts.season_id = $1
                      AND (LOWER(lt.name) LIKE $2 OR LOWER(lt.tag) LIKE $2)
                    ORDER BY lts.created_at ASC
                    LIMIT $3 OFFSET $4
                    ",
                )
                .bind(season_id.as_uuid())
                .bind(pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            }
            (None, None) => {
                sqlx::query_as::<_, LeagueTeamSeasonRow>(
                    r"
                    SELECT * FROM league_team_seasons
                    WHERE season_id = $1
                    ORDER BY created_at ASC
                    LIMIT $2 OFFSET $3
                    ",
                )
                .bind(season_id.as_uuid())
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_seasons WHERE season_id = $1",
        )
        .bind(season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(LeagueTeamSeason::from).collect(), count.0))
    }

    async fn list_by_team(&self, team_id: LeagueTeamId) -> Result<Vec<LeagueTeamSeason>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            "SELECT * FROM league_team_seasons WHERE team_id = $1 ORDER BY created_at DESC",
        )
        .bind(team_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeamSeason::from).collect())
    }

    async fn update_status(
        &self,
        id: LeagueTeamSeasonId,
        status: LeagueTeamSeasonStatus,
    ) -> Result<LeagueTeamSeason, DomainError> {
        let now = Utc::now();
        let registered_at = if status == LeagueTeamSeasonStatus::Registered {
            Some(now)
        } else {
            None
        };

        let row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            r"
            UPDATE league_team_seasons SET
                status = $2,
                registered_at = COALESCE($3, registered_at),
                updated_at = $4
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(registered_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamSeason::from(row))
    }

    async fn list_summaries(
        &self,
        season_id: LeagueSeasonId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeamSummary>, i64), DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamSummaryRow>(
            "SELECT * FROM v_league_team_summary WHERE season_id = $1 ORDER BY team_name ASC LIMIT $2 OFFSET $3",
        )
        .bind(season_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM v_league_team_summary WHERE season_id = $1",
        )
        .bind(season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(LeagueTeamSummary::from).collect(), count.0))
    }

    async fn update(
        &self,
        id: LeagueTeamSeasonId,
        update: UpdateLeagueTeamSeason,
    ) -> Result<LeagueTeamSeason, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            r"
            UPDATE league_team_seasons SET
                status = COALESCE($2, status),
                registration_notes = COALESCE($3, registration_notes),
                seed = COALESCE($4, seed),
                rating = COALESCE($5, rating),
                updated_at = $6
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(update.status.map(|s| s.to_string()))
        .bind(update.registration_notes)
        .bind(update.seed)
        .bind(update.rating)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamSeason::from(row))
    }

    async fn is_registered(
        &self,
        team_id: LeagueTeamId,
        season_id: LeagueSeasonId,
    ) -> Result<bool, DomainError> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM league_team_seasons WHERE team_id = $1 AND season_id = $2)",
        )
        .bind(team_id.as_uuid())
        .bind(season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.0)
    }
}
