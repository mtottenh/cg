//! `PostgreSQL` implementation of `TournamentRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::tournament::TournamentRow;
use portal_core::types::TournamentStatus;
use portal_core::{DomainError, GameId, LeagueId, TournamentId, UserId};
use portal_domain::entities::tournament::Tournament;
use portal_domain::repositories::tournament::{
    CreateTournament, TournamentFilters, TournamentRepository, UpdateTournament,
};

/// `PostgreSQL` implementation of `TournamentRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentRepository {
    pool: DbPool,
}

impl PgTournamentRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentRepository for PgTournamentRepository {
    async fn find_by_id(&self, id: TournamentId) -> Result<Option<Tournament>, DomainError> {
        let row = sqlx::query_as::<_, TournamentRow>("SELECT * FROM tournaments WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(Tournament::from))
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Tournament>, DomainError> {
        let row = sqlx::query_as::<_, TournamentRow>("SELECT * FROM tournaments WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(Tournament::from))
    }

    async fn create(&self, cmd: CreateTournament) -> Result<Tournament, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            INSERT INTO tournaments (
                id, game_id, league_id, season_id, name, slug, description,
                format, format_settings, participant_type, team_size,
                min_participants, max_participants, registration_type,
                registration_start, registration_end, check_in_required,
                check_in_start, check_in_end, scheduling_mode, starts_at,
                default_match_format, default_map_veto_format,
                withdrawal_policy, rules_url, settings, created_by,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29
            )
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.game_id.as_uuid())
        .bind(cmd.league_id.map(|id| id.as_uuid()))
        .bind(cmd.season_id.map(|id| id.as_uuid()))
        .bind(&cmd.name)
        .bind(&cmd.slug)
        .bind(&cmd.description)
        .bind(cmd.format.to_string())
        .bind(&cmd.format_settings)
        .bind(cmd.participant_type.to_string())
        .bind(cmd.team_size)
        .bind(cmd.min_participants)
        .bind(cmd.max_participants)
        .bind(cmd.registration_type.to_string())
        .bind(cmd.registration_start)
        .bind(cmd.registration_end)
        .bind(cmd.check_in_required)
        .bind(cmd.check_in_start)
        .bind(cmd.check_in_end)
        .bind(cmd.scheduling_mode.to_string())
        .bind(cmd.starts_at)
        .bind(cmd.default_match_format.to_string())
        .bind(&cmd.default_map_veto_format)
        .bind(cmd.withdrawal_policy.to_string())
        .bind(&cmd.rules_url)
        .bind(&cmd.settings)
        .bind(cmd.created_by.as_uuid())
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Tournament::from(row))
    }

    async fn update(
        &self,
        id: TournamentId,
        update: UpdateTournament,
    ) -> Result<Tournament, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            UPDATE tournaments SET
                name = COALESCE($2, name),
                slug = COALESCE($3, slug),
                description = COALESCE($4, description),
                format_settings = COALESCE($5, format_settings),
                min_participants = COALESCE($6, min_participants),
                max_participants = COALESCE($7, max_participants),
                registration_start = COALESCE($8, registration_start),
                registration_end = COALESCE($9, registration_end),
                check_in_required = COALESCE($10, check_in_required),
                check_in_start = COALESCE($11, check_in_start),
                check_in_end = COALESCE($12, check_in_end),
                starts_at = COALESCE($13, starts_at),
                ends_at = COALESCE($14, ends_at),
                timezone_hint = COALESCE($15, timezone_hint),
                default_match_format = COALESCE($16, default_match_format),
                default_map_veto_format = COALESCE($17, default_map_veto_format),
                prize_pool = COALESCE($18, prize_pool),
                rules_url = COALESCE($19, rules_url),
                settings = COALESCE($20, settings),
                withdrawal_policy = COALESCE($21, withdrawal_policy),
                updated_at = $22
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.slug)
        .bind(&update.description)
        .bind(&update.format_settings)
        .bind(update.min_participants)
        .bind(update.max_participants)
        .bind(update.registration_start)
        .bind(update.registration_end)
        .bind(update.check_in_required)
        .bind(update.check_in_start)
        .bind(update.check_in_end)
        .bind(update.starts_at)
        .bind(update.ends_at)
        .bind(&update.timezone_hint)
        .bind(update.default_match_format.map(|f| f.to_string()))
        .bind(&update.default_map_veto_format)
        .bind(&update.prize_pool)
        .bind(&update.rules_url)
        .bind(&update.settings)
        .bind(update.withdrawal_policy.map(|p| p.to_string()))
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Tournament::from(row))
    }

    async fn update_status(
        &self,
        id: TournamentId,
        status: TournamentStatus,
    ) -> Result<Tournament, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            UPDATE tournaments SET status = $2, updated_at = $3
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

        Ok(Tournament::from(row))
    }

    async fn update_logo(
        &self,
        id: TournamentId,
        logo_url: Option<String>,
    ) -> Result<Tournament, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            UPDATE tournaments SET logo_url = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&logo_url)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Tournament::from(row))
    }

    async fn update_banner(
        &self,
        id: TournamentId,
        banner_url: Option<String>,
    ) -> Result<Tournament, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            UPDATE tournaments SET banner_url = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&banner_url)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Tournament::from(row))
    }

    async fn mark_started(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            UPDATE tournaments SET started_at = $2, status = 'in_progress', updated_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Tournament::from(row))
    }

    async fn mark_completed(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            UPDATE tournaments SET completed_at = $2, status = 'completed', updated_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Tournament::from(row))
    }

    async fn mark_published(&self, id: TournamentId) -> Result<Tournament, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRow>(
            r"
            UPDATE tournaments SET published_at = $2, status = 'published', updated_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Tournament::from(row))
    }

    async fn list(
        &self,
        filters: TournamentFilters,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError> {
        // For simplicity, we use parameterized queries with NULL checks
        // since SQLx doesn't easily support dynamic query building.
        // Instead, we filter with Option checks in SQL.
        let rows = sqlx::query_as::<_, TournamentRow>(r"
            SELECT * FROM tournaments
            WHERE ($1::uuid IS NULL OR game_id = $1)
              AND ($2::uuid IS NULL OR league_id = $2)
              AND ($3::uuid IS NULL OR season_id = $3)
              AND ($4::text IS NULL OR status = $4)
              AND ($5::text IS NULL OR format = $5)
              AND ($6::text IS NULL OR participant_type = $6)
              AND ($7::text IS NULL OR name ILIKE $7 OR slug ILIKE $7)
              AND ($8::bool IS NULL OR NOT $8 OR starts_at > NOW())
              AND ($9::bool IS NULL OR NOT $9 OR status IN ('published', 'registration', 'check_in', 'in_progress'))
            ORDER BY starts_at DESC NULLS LAST, created_at DESC
            LIMIT $10 OFFSET $11
            ")
        .bind(filters.game_id.map(|id| id.as_uuid()))
        .bind(filters.league_id.map(|id| id.as_uuid()))
        .bind(filters.season_id.map(|id| id.as_uuid()))
        .bind(filters.status.map(|s| s.to_string()))
        .bind(filters.format.map(|f| f.to_string()))
        .bind(filters.participant_type.map(|p| p.to_string()))
        .bind(filters.search.as_ref().map(|s| format!("%{s}%")))
        .bind(filters.upcoming)
        .bind(filters.active)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Get count with same filters
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM tournaments
            WHERE ($1::uuid IS NULL OR game_id = $1)
              AND ($2::uuid IS NULL OR league_id = $2)
              AND ($3::uuid IS NULL OR season_id = $3)
              AND ($4::text IS NULL OR status = $4)
              AND ($5::text IS NULL OR format = $5)
              AND ($6::text IS NULL OR participant_type = $6)
              AND ($7::text IS NULL OR name ILIKE $7 OR slug ILIKE $7)
              AND ($8::bool IS NULL OR NOT $8 OR starts_at > NOW())
              AND ($9::bool IS NULL OR NOT $9 OR status IN ('published', 'registration', 'check_in', 'in_progress'))
            ",
        )
        .bind(filters.game_id.map(|id| id.as_uuid()))
        .bind(filters.league_id.map(|id| id.as_uuid()))
        .bind(filters.season_id.map(|id| id.as_uuid()))
        .bind(filters.status.map(|s| s.to_string()))
        .bind(filters.format.map(|f| f.to_string()))
        .bind(filters.participant_type.map(|p| p.to_string()))
        .bind(filters.search.as_ref().map(|s| format!("%{s}%")))
        .bind(filters.upcoming)
        .bind(filters.active)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(Tournament::from).collect(), count.0))
    }

    async fn list_by_game(
        &self,
        game_id: GameId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError> {
        let rows = sqlx::query_as::<_, TournamentRow>(
            r"
            SELECT * FROM tournaments
            WHERE game_id = $1
            ORDER BY starts_at DESC NULLS LAST, created_at DESC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(game_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tournaments WHERE game_id = $1")
            .bind(game_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(Tournament::from).collect(), count.0))
    }

    async fn list_by_league(
        &self,
        league_id: LeagueId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError> {
        let rows = sqlx::query_as::<_, TournamentRow>(
            r"
            SELECT * FROM tournaments
            WHERE league_id = $1
            ORDER BY starts_at DESC NULLS LAST, created_at DESC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(league_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tournaments WHERE league_id = $1")
            .bind(league_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(Tournament::from).collect(), count.0))
    }

    async fn list_by_creator(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Tournament>, i64), DomainError> {
        let rows = sqlx::query_as::<_, TournamentRow>(
            r"
            SELECT * FROM tournaments
            WHERE created_by = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(user_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM tournaments WHERE created_by = $1")
                .bind(user_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(Tournament::from).collect(), count.0))
    }

    async fn slug_exists(&self, slug: &str) -> Result<bool, DomainError> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tournaments WHERE slug = $1")
            .bind(slug)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn count_registrations(&self, id: TournamentId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM tournament_registrations
            WHERE tournament_id = $1 AND status NOT IN ('withdrawn', 'rejected')
            ",
        )
        .bind(id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn delete(&self, id: TournamentId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournaments WHERE id = $1 AND status = 'draft'")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
