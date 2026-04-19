//! `PostgreSQL` implementation of `LeagueTeamRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::entities::league_team::{LeagueTeamRow, LeagueTeamSeasonRow};
use crate::DbPool;
use portal_core::types::{LeagueTeamRole, LeagueTeamStatus};
use portal_core::{DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, PlayerId};
use portal_domain::entities::league_team::{LeagueTeam, LeagueTeamSeason};
use portal_domain::repositories::league_team::{
    CreateLeagueTeam, LeagueTeamRepository, UpdateLeagueTeam,
};

/// `PostgreSQL` implementation of `LeagueTeamRepository`.
#[derive(Debug, Clone)]
pub struct PgLeagueTeamRepository {
    pool: DbPool,
}

impl PgLeagueTeamRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn normalize_name(name: &str) -> String {
        name.to_lowercase().trim().to_string()
    }
}

#[async_trait]
impl LeagueTeamRepository for PgLeagueTeamRepository {
    async fn find_by_id(&self, id: LeagueTeamId) -> Result<Option<LeagueTeam>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            "SELECT * FROM league_teams WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeam::from))
    }

    async fn find_by_name(
        &self,
        league_id: LeagueId,
        name: &str,
    ) -> Result<Option<LeagueTeam>, DomainError> {
        let normalized = Self::normalize_name(name);
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            "SELECT * FROM league_teams WHERE league_id = $1 AND name_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeam::from))
    }

    async fn find_by_tag(
        &self,
        league_id: LeagueId,
        tag: &str,
    ) -> Result<Option<LeagueTeam>, DomainError> {
        let normalized = Self::normalize_name(tag);
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            "SELECT * FROM league_teams WHERE league_id = $1 AND tag_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeam::from))
    }

    async fn create(&self, cmd: CreateLeagueTeam) -> Result<LeagueTeam, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        // Note: name_normalized and tag_normalized are GENERATED columns
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            INSERT INTO league_teams (
                id, league_id, name, tag,
                description, logo_url, primary_color, secondary_color,
                owner_player_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.league_id.as_uuid())
        .bind(&cmd.name)
        .bind(&cmd.tag)
        .bind(&cmd.description)
        .bind(&cmd.logo_url)
        .bind(&cmd.primary_color)
        .bind(&cmd.secondary_color)
        .bind(cmd.owner_player_id.as_uuid())
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn update(
        &self,
        id: LeagueTeamId,
        update: UpdateLeagueTeam,
    ) -> Result<LeagueTeam, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams SET
                name = COALESCE($2, name),
                tag = COALESCE($3, tag),
                description = COALESCE($4, description),
                logo_url = COALESCE($5, logo_url),
                banner_url = COALESCE($6, banner_url),
                primary_color = COALESCE($7, primary_color),
                secondary_color = COALESCE($8, secondary_color),
                updated_at = $9
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.tag)
        .bind(&update.description)
        .bind(&update.logo_url)
        .bind(&update.banner_url)
        .bind(&update.primary_color)
        .bind(&update.secondary_color)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn list_by_league(
        &self,
        league_id: LeagueId,
        status_filter: Option<LeagueTeamStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeam>, i64), DomainError> {
        let search_pattern = search.as_ref().map(|s| format!("%{s}%"));

        let rows = if let (Some(status), Some(pattern)) = (&status_filter, &search_pattern) {
            sqlx::query_as::<_, LeagueTeamRow>(
                r"
                SELECT * FROM league_teams
                WHERE league_id = $1 AND status = $2 AND (name ILIKE $3 OR tag ILIKE $3)
                ORDER BY name ASC
                LIMIT $4 OFFSET $5
                ",
            )
            .bind(league_id.as_uuid())
            .bind(status.to_string())
            .bind(pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else if let Some(status) = &status_filter {
            sqlx::query_as::<_, LeagueTeamRow>(
                r"
                SELECT * FROM league_teams
                WHERE league_id = $1 AND status = $2
                ORDER BY name ASC
                LIMIT $3 OFFSET $4
                ",
            )
            .bind(league_id.as_uuid())
            .bind(status.to_string())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else if let Some(pattern) = &search_pattern {
            sqlx::query_as::<_, LeagueTeamRow>(
                r"
                SELECT * FROM league_teams
                WHERE league_id = $1 AND (name ILIKE $2 OR tag ILIKE $2)
                ORDER BY name ASC
                LIMIT $3 OFFSET $4
                ",
            )
            .bind(league_id.as_uuid())
            .bind(pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, LeagueTeamRow>(
                r"
                SELECT * FROM league_teams
                WHERE league_id = $1
                ORDER BY name ASC
                LIMIT $2 OFFSET $3
                ",
            )
            .bind(league_id.as_uuid())
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Get total count
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM league_teams WHERE league_id = $1")
            .bind(league_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(LeagueTeam::from).collect(), count.0))
    }

    async fn list_by_owner(
        &self,
        league_id: LeagueId,
        owner_player_id: PlayerId,
    ) -> Result<Vec<LeagueTeam>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            SELECT * FROM league_teams
            WHERE league_id = $1 AND owner_player_id = $2 AND status != 'disbanded'
            ORDER BY name ASC
            ",
        )
        .bind(league_id.as_uuid())
        .bind(owner_player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeam::from).collect())
    }

    async fn update_status(
        &self,
        id: LeagueTeamId,
        status: LeagueTeamStatus,
    ) -> Result<LeagueTeam, DomainError> {
        let now = Utc::now();
        let disbanded_at = if status == LeagueTeamStatus::Disbanded {
            Some(now)
        } else {
            None
        };

        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams SET
                status = $2,
                disbanded_at = COALESCE($3, disbanded_at),
                updated_at = $4
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(disbanded_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn transfer_ownership(
        &self,
        id: LeagueTeamId,
        new_owner_player_id: PlayerId,
    ) -> Result<LeagueTeam, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams SET
                owner_player_id = $2,
                updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(new_owner_player_id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn name_exists(
        &self,
        league_id: LeagueId,
        name: &str,
    ) -> Result<bool, DomainError> {
        let normalized = Self::normalize_name(name);
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_teams WHERE league_id = $1 AND name_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn tag_exists(&self, league_id: LeagueId, tag: &str) -> Result<bool, DomainError> {
        let normalized = Self::normalize_name(tag);
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_teams WHERE league_id = $1 AND tag_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn delete(&self, id: LeagueTeamId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM league_teams WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn create_team_with_season_and_captain(
        &self,
        cmd: CreateLeagueTeam,
        season_id: LeagueSeasonId,
        captain_player_id: PlayerId,
    ) -> Result<(LeagueTeam, LeagueTeamSeason), DomainError> {
        // All three writes share a single transaction. Any error below
        // causes the transaction to roll back on drop, so a failed
        // team_season insert doesn't orphan the league_teams row, and a
        // failed member insert doesn't leave an empty team_season.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let now = Utc::now();
        let team_id = uuid::Uuid::now_v7();
        let team_row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            INSERT INTO league_teams (
                id, league_id, name, tag,
                description, logo_url, primary_color, secondary_color,
                owner_player_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(team_id)
        .bind(cmd.league_id.as_uuid())
        .bind(&cmd.name)
        .bind(&cmd.tag)
        .bind(&cmd.description)
        .bind(&cmd.logo_url)
        .bind(&cmd.primary_color)
        .bind(&cmd.secondary_color)
        .bind(cmd.owner_player_id.as_uuid())
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let team_season_id = uuid::Uuid::now_v7();
        let team_season_row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            r"
            INSERT INTO league_team_seasons (
                id, team_id, season_id, status, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(team_season_id)
        .bind(team_id)
        .bind(season_id.as_uuid())
        .bind("forming")
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let member_id = uuid::Uuid::now_v7();
        sqlx::query(
            r"
            INSERT INTO league_team_members (
                id, team_season_id, player_id, role, position, jersey_number, added_by, joined_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(member_id)
        .bind(team_season_id)
        .bind(captain_player_id.as_uuid())
        .bind(LeagueTeamRole::Captain.to_string())
        .bind(None::<String>)
        .bind(None::<i32>)
        .bind(None::<uuid::Uuid>)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((LeagueTeam::from(team_row), LeagueTeamSeason::from(team_season_row)))
    }
}
