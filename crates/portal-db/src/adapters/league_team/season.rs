//! `PostgreSQL` implementation of `LeagueSeasonRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::entities::league_team::LeagueSeasonRow;
use crate::DbPool;
use portal_core::types::{RosterLockStatus, SeasonStatus};
use portal_core::{DomainError, LeagueId, LeagueSeasonId, UserId};
use portal_domain::entities::league_team::LeagueSeason;
use portal_domain::repositories::league_team::{
    CreateLeagueSeason, LeagueSeasonRepository, UpdateLeagueSeason,
};

/// `PostgreSQL` implementation of `LeagueSeasonRepository`.
#[derive(Debug, Clone)]
pub struct PgLeagueSeasonRepository {
    pool: DbPool,
}

impl PgLeagueSeasonRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LeagueSeasonRepository for PgLeagueSeasonRepository {
    async fn find_by_id(&self, id: LeagueSeasonId) -> Result<Option<LeagueSeason>, DomainError> {
        let row = sqlx::query_as::<_, LeagueSeasonRow>(
            "SELECT * FROM league_seasons WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueSeason::from))
    }

    async fn find_by_slug(
        &self,
        league_id: LeagueId,
        slug: &str,
    ) -> Result<Option<LeagueSeason>, DomainError> {
        let row = sqlx::query_as::<_, LeagueSeasonRow>(
            "SELECT * FROM league_seasons WHERE league_id = $1 AND slug = $2",
        )
        .bind(league_id.as_uuid())
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueSeason::from))
    }

    async fn create(&self, cmd: CreateLeagueSeason) -> Result<LeagueSeason, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            INSERT INTO league_seasons (
                id, league_id, name, slug, description,
                registration_start, registration_end, season_start, season_end,
                team_size_min, team_size_max, max_substitutes, max_teams,
                created_by, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9,
                COALESCE($10, 1), COALESCE($11, 5), COALESCE($12, 2), $13,
                $14, $15, $16
            )
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.league_id.as_uuid())
        .bind(&cmd.name)
        .bind(&cmd.slug)
        .bind(&cmd.description)
        .bind(cmd.registration_start)
        .bind(cmd.registration_end)
        .bind(cmd.season_start)
        .bind(cmd.season_end)
        .bind(cmd.team_size_min)
        .bind(cmd.team_size_max)
        .bind(cmd.max_substitutes)
        .bind(cmd.max_teams)
        .bind(cmd.created_by.as_uuid())
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueSeason::from(row))
    }

    async fn update(
        &self,
        id: LeagueSeasonId,
        update: UpdateLeagueSeason,
    ) -> Result<LeagueSeason, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            UPDATE league_seasons SET
                name = COALESCE($2, name),
                slug = COALESCE($3, slug),
                description = COALESCE($4, description),
                registration_start = COALESCE($5, registration_start),
                registration_end = COALESCE($6, registration_end),
                season_start = COALESCE($7, season_start),
                season_end = COALESCE($8, season_end),
                team_size_min = COALESCE($9, team_size_min),
                team_size_max = COALESCE($10, team_size_max),
                max_substitutes = COALESCE($11, max_substitutes),
                max_teams = COALESCE($12, max_teams),
                status = COALESCE($13, status),
                settings = COALESCE($14, settings),
                updated_at = $15
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.slug)
        .bind(&update.description)
        .bind(update.registration_start)
        .bind(update.registration_end)
        .bind(update.season_start)
        .bind(update.season_end)
        .bind(update.team_size_min)
        .bind(update.team_size_max)
        .bind(update.max_substitutes)
        .bind(update.max_teams)
        .bind(update.status.map(|s| s.to_string()))
        .bind(&update.settings)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueSeason::from(row))
    }

    async fn list_by_league(&self, league_id: LeagueId) -> Result<Vec<LeagueSeason>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            SELECT * FROM league_seasons
            WHERE league_id = $1
            ORDER BY created_at DESC
            ",
        )
        .bind(league_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueSeason::from).collect())
    }

    async fn list_active_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueSeason>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            SELECT * FROM league_seasons
            WHERE league_id = $1 AND status IN ('draft', 'registration', 'active', 'playoffs')
            ORDER BY created_at DESC
            ",
        )
        .bind(league_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueSeason::from).collect())
    }

    async fn find_current_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Option<LeagueSeason>, DomainError> {
        // Note: The migration adds a current_season_id column to leagues table
        // and a trigger to set it. We can use that directly.
        let row = sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            SELECT s.* FROM league_seasons s
            JOIN leagues l ON l.current_season_id = s.id
            WHERE l.id = $1
            ",
        )
        .bind(league_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueSeason::from))
    }

    async fn slug_exists(&self, league_id: LeagueId, slug: &str) -> Result<bool, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_seasons WHERE league_id = $1 AND slug = $2",
        )
        .bind(league_id.as_uuid())
        .bind(slug)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn update_roster_lock(
        &self,
        id: LeagueSeasonId,
        status: RosterLockStatus,
        locked_by: Option<UserId>,
    ) -> Result<LeagueSeason, DomainError> {
        let now = Utc::now();
        let locked_at = if status == RosterLockStatus::HardLock {
            Some(now)
        } else {
            None
        };

        let row = sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            UPDATE league_seasons SET
                roster_lock_status = $2,
                roster_locked_at = $3,
                roster_locked_by = $4,
                updated_at = $5
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(locked_at)
        .bind(locked_by.map(|u| u.as_uuid()))
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueSeason::from(row))
    }

    async fn update_status(
        &self,
        id: LeagueSeasonId,
        status: SeasonStatus,
    ) -> Result<LeagueSeason, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            UPDATE league_seasons SET
                status = $2,
                updated_at = $3
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

        Ok(LeagueSeason::from(row))
    }

    async fn count_teams(&self, season_id: LeagueSeasonId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_seasons WHERE season_id = $1 AND status NOT IN ('withdrawn', 'disqualified')",
        )
        .bind(season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn count_participants(&self, season_id: LeagueSeasonId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_season_participants WHERE season_id = $1 AND status NOT IN ('withdrawn', 'disqualified')",
        )
        .bind(season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }
}
