//! `PostgreSQL` implementation of `TournamentStandingsRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::tournament::TournamentStandingRow;
use crate::transaction::DbTransaction;
use portal_core::{DomainError, TournamentBracketId, TournamentRegistrationId};
use portal_domain::entities::tournament::TournamentStanding;
use portal_domain::repositories::tournament::{
    CreateTournamentStanding, TournamentStandingsRepository,
};

/// Points awarded for a match win (and for a bye).
///
/// A loss is worth 0. This mirrors the constant the old delta call sites
/// hard-coded in six places.
const WIN_POINTS: i32 = 3;

/// Derives every mutable column of `tournament_standings` for one bracket
/// from the rows that are its source of truth.
///
/// Two contributions are unioned:
///
/// 1. **Completed match rows** — one row per side. `status = 'completed'`
///    only: forfeits (`status = 'forfeit'`) never fed standings under the
///    delta model either, and folding them in now would silently invent
///    points for every already-forfeited match. Winner/loser game scores are
///    read from the participant slot the winner occupies (not
///    `GREATEST`/`LEAST`), exactly as the old `standings_deltas` did, so an
///    overturned result whose winner has the lower score is attributed the
///    same way it always was.
/// 2. **Recorded byes** — `tournament_bracket_byes`. A Swiss bye has no
///    match row; without this union a recompute would erase its points.
///
/// `matches_drawn` is always 0: nothing in the schema can express a draw
/// (`winner_registration_id`/`loser_registration_id` are both required to
/// count a result), and no production path ever wrote a non-zero value.
///
/// `buchholz_score` / `opponent_match_wins` are deliberately left untouched —
/// they were never persisted by the delta path either.
const DERIVE_STANDINGS_SQL: &str = r"
WITH contributions AS (
    SELECT
        m.winner_registration_id AS registration_id,
        1 AS played, 1 AS won, 0 AS lost,
        CASE WHEN m.participant1_registration_id = m.winner_registration_id
             THEN m.participant1_score ELSE m.participant2_score END AS game_wins,
        CASE WHEN m.participant1_registration_id = m.winner_registration_id
             THEN m.participant2_score ELSE m.participant1_score END AS game_losses,
        $2::int4 AS points
    FROM tournament_matches m
    WHERE m.bracket_id = $1
      AND m.status = 'completed'
      AND m.winner_registration_id IS NOT NULL
      AND m.loser_registration_id IS NOT NULL

    UNION ALL

    SELECT
        m.loser_registration_id AS registration_id,
        1 AS played, 0 AS won, 1 AS lost,
        CASE WHEN m.participant1_registration_id = m.winner_registration_id
             THEN m.participant2_score ELSE m.participant1_score END AS game_wins,
        CASE WHEN m.participant1_registration_id = m.winner_registration_id
             THEN m.participant1_score ELSE m.participant2_score END AS game_losses,
        0 AS points
    FROM tournament_matches m
    WHERE m.bracket_id = $1
      AND m.status = 'completed'
      AND m.winner_registration_id IS NOT NULL
      AND m.loser_registration_id IS NOT NULL

    UNION ALL

    SELECT b.registration_id, 1, 1, 0, 0, 0, b.points
    FROM tournament_bracket_byes b
    WHERE b.bracket_id = $1
),
aggregated AS (
    SELECT
        registration_id,
        SUM(played)::int4      AS matches_played,
        SUM(won)::int4         AS matches_won,
        SUM(lost)::int4        AS matches_lost,
        SUM(game_wins)::int4   AS game_wins,
        SUM(game_losses)::int4 AS game_losses,
        SUM(points)::int4      AS points
    FROM contributions
    GROUP BY registration_id
),
derived AS (
    SELECT
        s.id,
        COALESCE(a.matches_played, 0) AS matches_played,
        COALESCE(a.matches_won, 0)    AS matches_won,
        COALESCE(a.matches_lost, 0)   AS matches_lost,
        COALESCE(a.game_wins, 0)      AS game_wins,
        COALESCE(a.game_losses, 0)    AS game_losses,
        COALESCE(a.points, 0)         AS points
    FROM tournament_standings s
    LEFT JOIN aggregated a ON a.registration_id = s.registration_id
    WHERE s.bracket_id = $1
)
UPDATE tournament_standings s SET
    matches_played    = d.matches_played,
    matches_won       = d.matches_won,
    matches_lost      = d.matches_lost,
    matches_drawn     = 0,
    game_wins         = d.game_wins,
    game_losses       = d.game_losses,
    game_differential = d.game_wins - d.game_losses,
    points            = d.points,
    updated_at        = $3
FROM derived d
WHERE s.id = d.id
";

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

    async fn recompute_bracket(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        // BEGIN → derive → re-rank → COMMIT, modelled on
        // `PgAwardRepository::replace_results_and_finalize`: the derived
        // rows and the positions computed from them are never visible
        // apart.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Self::recompute_bracket_in_tx(&mut tx, bracket_id).await?;
        Self::recalculate_positions_in_tx(&mut tx, bracket_id).await?;
        let standings = Self::list_by_bracket_in_tx(&mut tx, bracket_id).await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(standings)
    }

    async fn record_bye(
        &self,
        bracket_id: TournamentBracketId,
        registration_id: TournamentRegistrationId,
        round: i32,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            INSERT INTO tournament_bracket_byes (bracket_id, registration_id, round, points)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (bracket_id, registration_id, round) DO NOTHING
            ",
        )
        .bind(bracket_id.as_uuid())
        .bind(registration_id.as_uuid())
        .bind(round)
        .bind(WIN_POINTS)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
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
    /// Recompute every standing in a bracket from the completed match rows
    /// (and recorded byes) that are their source of truth, inside a caller-
    /// supplied transaction.
    ///
    /// Idempotent: the written values are a pure function of the source
    /// rows, so running it again converges rather than double-counting.
    /// Callers normally follow it with
    /// [`Self::recalculate_positions_in_tx`].
    pub async fn recompute_bracket_in_tx(
        tx: &mut DbTransaction<'_>,
        bracket_id: TournamentBracketId,
    ) -> Result<(), DomainError> {
        sqlx::query(DERIVE_STANDINGS_SQL)
            .bind(bracket_id.as_uuid())
            .bind(WIN_POINTS)
            .bind(Utc::now())
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
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
