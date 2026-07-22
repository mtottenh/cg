//! `PostgreSQL` implementation of `TournamentRegistrationRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::tournament::TournamentRegistrationRow;
use crate::transaction::DbTransaction;
use portal_core::types::TournamentRegistrationStatus;
use portal_core::{
    DomainError, LeagueTeamSeasonId, PlayerId, TournamentId, TournamentRegistrationId, UserId,
};
use portal_domain::entities::tournament::TournamentRegistration;
use portal_domain::repositories::tournament::{
    CreateTournamentRegistration, TournamentRegistrationRepository, UpdateTournamentRegistration,
};

/// `PostgreSQL` implementation of `TournamentRegistrationRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentRegistrationRepository {
    pool: DbPool,
}

impl PgTournamentRegistrationRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentRegistrationRepository for PgTournamentRegistrationRepository {
    async fn find_by_id(
        &self,
        id: TournamentRegistrationId,
    ) -> Result<Option<TournamentRegistration>, DomainError> {
        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            "SELECT * FROM tournament_registrations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentRegistration::from))
    }

    async fn find_by_team_season(
        &self,
        tournament_id: TournamentId,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Option<TournamentRegistration>, DomainError> {
        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            SELECT * FROM tournament_registrations
            WHERE tournament_id = $1 AND team_season_id = $2
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(team_season_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentRegistration::from))
    }

    async fn find_by_player(
        &self,
        tournament_id: TournamentId,
        player_id: PlayerId,
    ) -> Result<Option<TournamentRegistration>, DomainError> {
        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            SELECT * FROM tournament_registrations
            WHERE tournament_id = $1 AND player_id = $2
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentRegistration::from))
    }

    async fn create(
        &self,
        cmd: CreateTournamentRegistration,
    ) -> Result<TournamentRegistration, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            INSERT INTO tournament_registrations (
                id, tournament_id, team_season_id, player_id, adhoc_team_id,
                participant_name, participant_logo_url, registered_by,
                registered_at, seed_rating, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.tournament_id.as_uuid())
        .bind(cmd.team_season_id.map(|id| id.as_uuid()))
        .bind(cmd.player_id.map(|id| id.as_uuid()))
        .bind(cmd.adhoc_team_id)
        .bind(&cmd.participant_name)
        .bind(&cmd.participant_logo_url)
        .bind(cmd.registered_by.as_uuid())
        .bind(now)
        .bind(cmd.seed_rating)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentRegistration::from(row))
    }

    async fn create_with_capacity_check(
        &self,
        cmd: CreateTournamentRegistration,
        replace_terminal: Option<TournamentRegistrationId>,
    ) -> Result<TournamentRegistration, DomainError> {
        // Capacity check + insert in one tx behind a row lock on the
        // tournament. `SELECT ... FOR UPDATE` serialises every concurrent
        // registration for the same tournament, so the COUNT below is a
        // true pre-insert count rather than a stale read taken on another
        // connection. Without this, N racers all read `count = max - 1` and
        // all inserted. See audit "capacity check is count-then-insert".
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to begin transaction: {e}")))?;

        let max_participants: i32 =
            sqlx::query_scalar("SELECT max_participants FROM tournaments WHERE id = $1 FOR UPDATE")
                .bind(cmd.tournament_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?
                .ok_or(DomainError::TournamentNotFound(cmd.tournament_id))?;

        // Drop the superseded terminal registration inside the same tx so
        // the slot it frees can't be taken by a racer mid-flight.
        if let Some(old_id) = replace_terminal {
            sqlx::query("DELETE FROM tournament_registrations WHERE id = $1")
                .bind(old_id.as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        let count: i64 = sqlx::query_scalar(
            r"
            SELECT COUNT(*) FROM tournament_registrations
            WHERE tournament_id = $1 AND status NOT IN ('withdrawn', 'rejected')
            ",
        )
        .bind(cmd.tournament_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if count >= i64::from(max_participants) {
            return Err(DomainError::TournamentFull);
        }

        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            INSERT INTO tournament_registrations (
                id, tournament_id, team_season_id, player_id, adhoc_team_id,
                participant_name, participant_logo_url, registered_by,
                registered_at, seed_rating, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.tournament_id.as_uuid())
        .bind(cmd.team_season_id.map(|id| id.as_uuid()))
        .bind(cmd.player_id.map(|id| id.as_uuid()))
        .bind(cmd.adhoc_team_id)
        .bind(&cmd.participant_name)
        .bind(&cmd.participant_logo_url)
        .bind(cmd.registered_by.as_uuid())
        .bind(now)
        .bind(cmd.seed_rating)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to commit: {e}")))?;

        Ok(TournamentRegistration::from(row))
    }

    async fn update_status_with_capacity_check(
        &self,
        id: TournamentRegistrationId,
        status: TournamentRegistrationStatus,
    ) -> Result<TournamentRegistration, DomainError> {
        // Admin approval of a pending registration had the same
        // count-then-write race as create; approve the row under the same
        // tournament lock so a burst of approvals can't overflow capacity.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to begin transaction: {e}")))?;

        let tournament_uuid: uuid::Uuid = sqlx::query_scalar(
            r"
            SELECT t.id FROM tournaments t
            JOIN tournament_registrations r ON r.tournament_id = t.id
            WHERE r.id = $1
            FOR UPDATE OF t
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::TournamentRegistrationNotFound(id))?;

        let max_participants: i32 =
            sqlx::query_scalar("SELECT max_participants FROM tournaments WHERE id = $1")
                .bind(tournament_uuid)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Count everything that occupies a slot *other than* this row — the
        // registration being approved keeps whichever slot it already held.
        let count: i64 = sqlx::query_scalar(
            r"
            SELECT COUNT(*) FROM tournament_registrations
            WHERE tournament_id = $1 AND id <> $2
              AND status NOT IN ('withdrawn', 'rejected')
            ",
        )
        .bind(tournament_uuid)
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if count >= i64::from(max_participants) {
            return Err(DomainError::TournamentFull);
        }

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            UPDATE tournament_registrations SET status = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(Utc::now())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to commit: {e}")))?;

        Ok(TournamentRegistration::from(row))
    }

    async fn update(
        &self,
        id: TournamentRegistrationId,
        update: UpdateTournamentRegistration,
    ) -> Result<TournamentRegistration, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            UPDATE tournament_registrations SET
                participant_name = COALESCE($2, participant_name),
                participant_logo_url = COALESCE($3, participant_logo_url),
                seed = COALESCE($4, seed),
                seed_rating = COALESCE($5, seed_rating),
                admin_notes = COALESCE($6, admin_notes),
                updated_at = $7
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.participant_name)
        .bind(&update.participant_logo_url)
        .bind(update.seed)
        .bind(update.seed_rating)
        .bind(&update.admin_notes)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentRegistration::from(row))
    }

    async fn update_status(
        &self,
        id: TournamentRegistrationId,
        status: TournamentRegistrationStatus,
    ) -> Result<TournamentRegistration, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            UPDATE tournament_registrations SET status = $2, updated_at = $3
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

        Ok(TournamentRegistration::from(row))
    }

    async fn check_in(
        &self,
        id: TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentRegistration, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            UPDATE tournament_registrations SET
                checked_in = true,
                checked_in_at = $2,
                checked_in_by = $3,
                status = 'checked_in',
                updated_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .bind(checked_in_by.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentRegistration::from(row))
    }

    async fn update_seed(
        &self,
        id: TournamentRegistrationId,
        seed: i32,
    ) -> Result<TournamentRegistration, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            UPDATE tournament_registrations SET seed = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(seed)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentRegistration::from(row))
    }

    async fn withdraw(
        &self,
        id: TournamentRegistrationId,
    ) -> Result<TournamentRegistration, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            UPDATE tournament_registrations SET
                status = 'withdrawn',
                withdrawn_at = $2,
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

        Ok(TournamentRegistration::from(row))
    }

    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
        status_filter: Option<TournamentRegistrationStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TournamentRegistration>, i64), DomainError> {
        let rows = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            SELECT * FROM tournament_registrations
            WHERE tournament_id = $1
              AND ($2::text IS NULL OR status = $2)
            ORDER BY seed ASC NULLS LAST, registered_at ASC
            LIMIT $3 OFFSET $4
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(status_filter.map(|s| s.to_string()))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM tournament_registrations
            WHERE tournament_id = $1
              AND ($2::text IS NULL OR status = $2)
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(status_filter.map(|s| s.to_string()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((
            rows.into_iter().map(TournamentRegistration::from).collect(),
            count.0,
        ))
    }

    async fn list_checked_in(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentRegistration>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            SELECT * FROM tournament_registrations
            WHERE tournament_id = $1 AND checked_in = true AND status IN ('checked_in', 'active')
            ORDER BY seed ASC NULLS LAST, registered_at ASC
            ",
        )
        .bind(tournament_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentRegistration::from).collect())
    }

    async fn list_seeded(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentRegistration>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentRegistrationRow>(
            r"
            SELECT * FROM tournament_registrations
            WHERE tournament_id = $1
              AND status IN ('checked_in', 'approved', 'active')
            ORDER BY seed ASC NULLS LAST, seed_rating DESC NULLS LAST, registered_at ASC
            ",
        )
        .bind(tournament_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentRegistration::from).collect())
    }

    async fn count_by_status(
        &self,
        tournament_id: TournamentId,
        status: TournamentRegistrationStatus,
    ) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM tournament_registrations
            WHERE tournament_id = $1 AND status = $2
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn bulk_update_seeds(
        &self,
        seeds: Vec<(TournamentRegistrationId, i32)>,
    ) -> Result<(), DomainError> {
        let now = Utc::now();

        for (id, seed) in seeds {
            sqlx::query(
                r"
                UPDATE tournament_registrations SET seed = $2, updated_at = $3
                WHERE id = $1
                ",
            )
            .bind(id.as_uuid())
            .bind(seed)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        Ok(())
    }

    async fn clear_seeds(&self, tournament_id: TournamentId) -> Result<(), DomainError> {
        let now = Utc::now();

        sqlx::query(
            r"
            UPDATE tournament_registrations SET seed = NULL, seed_rating = NULL, updated_at = $2
            WHERE tournament_id = $1
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: TournamentRegistrationId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_registrations WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}

// =============================================================================
// TRANSACTIONAL METHODS
// =============================================================================

impl PgTournamentRegistrationRepository {
    /// Find a registration by ID within a transaction.
    pub async fn find_by_id_in_tx(
        tx: &mut DbTransaction<'_>,
        id: TournamentRegistrationId,
    ) -> Result<Option<TournamentRegistration>, DomainError> {
        let row = sqlx::query_as::<_, TournamentRegistrationRow>(
            "SELECT * FROM tournament_registrations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentRegistration::from))
    }
}
