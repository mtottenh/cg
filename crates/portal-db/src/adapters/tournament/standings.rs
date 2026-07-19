//! `PostgreSQL` implementation of `TournamentStandingsRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::tournament::TournamentStandingRow;
use crate::transaction::DbTransaction;
use portal_core::{DomainError, TournamentBracketId, TournamentRegistrationId};
use portal_domain::entities::tournament::TournamentStanding;
use portal_domain::repositories::tournament::{
    CreateTournamentStanding, TournamentStandingsRepository, UpdateTournamentStanding,
};

/// `PostgreSQL` implementation of `TournamentStandingsRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentStandingsRepository {
    pool: DbPool,
}

impl PgTournamentStandingsRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentStandingsRepository for PgTournamentStandingsRepository {
    async fn find(
        &self,
        bracket_id: TournamentBracketId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Option<TournamentStanding>, DomainError> {
        let row = sqlx::query_as::<_, TournamentStandingRow>(
            r"
            SELECT * FROM tournament_standings
            WHERE bracket_id = $1 AND registration_id = $2
            ",
        )
        .bind(bracket_id.as_uuid())
        .bind(registration_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentStanding::from))
    }

    async fn create(
        &self,
        cmd: CreateTournamentStanding,
    ) -> Result<TournamentStanding, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentStandingRow>(
            r"
            INSERT INTO tournament_standings (
                id, bracket_id, registration_id, position, updated_at
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.bracket_id.as_uuid())
        .bind(cmd.registration_id.as_uuid())
        .bind(cmd.position)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentStanding::from(row))
    }

    async fn update_after_match(
        &self,
        update: UpdateTournamentStanding,
    ) -> Result<TournamentStanding, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentStandingRow>(
            r"
            UPDATE tournament_standings SET
                matches_played = matches_played + 1,
                matches_won = matches_won + $3,
                matches_lost = matches_lost + $4,
                matches_drawn = matches_drawn + $5,
                game_wins = game_wins + $6,
                game_losses = game_losses + $7,
                game_differential = game_differential + ($6 - $7),
                points = points + $8,
                updated_at = $9
            WHERE bracket_id = $1 AND registration_id = $2
            RETURNING *
            ",
        )
        .bind(update.bracket_id.as_uuid())
        .bind(update.registration_id.as_uuid())
        .bind(update.matches_won_delta)
        .bind(update.matches_lost_delta)
        .bind(update.matches_drawn_delta)
        .bind(update.game_wins_delta)
        .bind(update.game_losses_delta)
        .bind(update.points_delta)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentStanding::from(row))
    }

    async fn recalculate_positions(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        let now = Utc::now();

        // Update positions based on points, then game differential, then match wins
        sqlx::query(
            r"
            WITH ranked AS (
                SELECT id,
                    ROW_NUMBER() OVER (
                        ORDER BY points DESC, game_differential DESC, matches_won DESC
                    ) as new_position
                FROM tournament_standings
                WHERE bracket_id = $1
            )
            UPDATE tournament_standings s
            SET position = r.new_position, updated_at = $2
            FROM ranked r
            WHERE s.id = r.id
            ",
        )
        .bind(bracket_id.as_uuid())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Return updated standings
        self.list_by_bracket(bracket_id).await
    }

    async fn list_by_bracket(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentStandingRow>(
            r"
            SELECT ts.*, tr.participant_name
            FROM tournament_standings ts
            JOIN tournament_registrations tr ON tr.id = ts.registration_id
            WHERE ts.bracket_id = $1
            ORDER BY ts.position ASC
            ",
        )
        .bind(bracket_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentStanding::from).collect())
    }

    async fn bulk_create(
        &self,
        standings: Vec<CreateTournamentStanding>,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        if standings.is_empty() {
            return Ok(Vec::new());
        }

        // Previously this looped `self.create(cmd).await?` — N round-trips
        // per call, and non-atomic: a failure halfway through left the
        // first few inserts committed. Now we bind one parallel-array
        // query per tuple slot and atomically insert everything in one
        // statement. `UNNEST` preserves order, so the returned rows match
        // the input order (which callers rely on when pairing seeds to
        // standings).
        let len = standings.len();
        let mut ids: Vec<uuid::Uuid> = Vec::with_capacity(len);
        let mut bracket_ids: Vec<uuid::Uuid> = Vec::with_capacity(len);
        let mut registration_ids: Vec<uuid::Uuid> = Vec::with_capacity(len);
        let mut positions: Vec<i32> = Vec::with_capacity(len);

        for cmd in standings {
            ids.push(uuid::Uuid::now_v7());
            bracket_ids.push(cmd.bracket_id.as_uuid());
            registration_ids.push(cmd.registration_id.as_uuid());
            positions.push(cmd.position);
        }

        let now = Utc::now();

        let rows = sqlx::query_as::<_, TournamentStandingRow>(
            r"
            INSERT INTO tournament_standings (
                id, bracket_id, registration_id, position, updated_at
            )
            SELECT * FROM UNNEST(
                $1::uuid[],
                $2::uuid[],
                $3::uuid[],
                $4::int4[]
            ) AS t(id, bracket_id, registration_id, position)
            CROSS JOIN (SELECT $5::timestamptz AS updated_at) u
            RETURNING *
            ",
        )
        .bind(&ids)
        .bind(&bracket_ids)
        .bind(&registration_ids)
        .bind(&positions)
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentStanding::from).collect())
    }
}

// =============================================================================
// TRANSACTIONAL METHODS
// =============================================================================

impl PgTournamentStandingsRepository {
    /// Update standings after a match within a transaction.
    pub async fn update_after_match_in_tx(
        tx: &mut DbTransaction<'_>,
        update: UpdateTournamentStanding,
    ) -> Result<TournamentStanding, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentStandingRow>(
            r"
            UPDATE tournament_standings SET
                matches_played = matches_played + 1,
                matches_won = matches_won + $3,
                matches_lost = matches_lost + $4,
                matches_drawn = matches_drawn + $5,
                game_wins = game_wins + $6,
                game_losses = game_losses + $7,
                game_differential = game_differential + ($6 - $7),
                points = points + $8,
                updated_at = $9
            WHERE bracket_id = $1 AND registration_id = $2
            RETURNING *
            ",
        )
        .bind(update.bracket_id.as_uuid())
        .bind(update.registration_id.as_uuid())
        .bind(update.matches_won_delta)
        .bind(update.matches_lost_delta)
        .bind(update.matches_drawn_delta)
        .bind(update.game_wins_delta)
        .bind(update.game_losses_delta)
        .bind(update.points_delta)
        .bind(now)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentStanding::from(row))
    }

    /// Recalculate standings positions within a transaction.
    pub async fn recalculate_positions_in_tx(
        tx: &mut DbTransaction<'_>,
        bracket_id: TournamentBracketId,
    ) -> Result<(), DomainError> {
        let now = Utc::now();

        sqlx::query(
            r"
            WITH ranked AS (
                SELECT id,
                    ROW_NUMBER() OVER (
                        ORDER BY points DESC, game_differential DESC, matches_won DESC
                    ) as new_position
                FROM tournament_standings
                WHERE bracket_id = $1
            )
            UPDATE tournament_standings s
            SET position = r.new_position, updated_at = $2
            FROM ranked r
            WHERE s.id = r.id
            ",
        )
        .bind(bracket_id.as_uuid())
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    /// List standings by bracket within a transaction.
    pub async fn list_by_bracket_in_tx(
        tx: &mut DbTransaction<'_>,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentStandingRow>(
            r"
            SELECT ts.*, tr.participant_name
            FROM tournament_standings ts
            JOIN tournament_registrations tr ON tr.id = ts.registration_id
            WHERE ts.bracket_id = $1
            ORDER BY ts.position ASC
            ",
        )
        .bind(bracket_id.as_uuid())
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentStanding::from).collect())
    }
}
