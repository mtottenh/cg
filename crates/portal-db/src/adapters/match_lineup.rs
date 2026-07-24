//! Match lineup repository adapter.

use crate::DbPool;
use crate::entities::{MatchLineupPlayerRow, MatchLineupRow};
use async_trait::async_trait;
use portal_core::types::{LineupSource, LineupStatus, ParticipationStatus};
use portal_core::{
    DomainError, MatchLineupId, MatchLineupPlayerId, PlayerId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use portal_domain::entities::match_lineup::{
    DeclareLineupCommand, MatchLineup, MatchLineupPlayer, MatchLineupWithPlayers,
};
use portal_domain::repositories::match_lineup::{MatchLineupRepository, MaterializeDemoLineup};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<MatchLineupRow> for MatchLineup {
    fn from(row: MatchLineupRow) -> Self {
        Self {
            id: MatchLineupId::from(row.id),
            match_id: TournamentMatchId::from(row.match_id),
            registration_id: TournamentRegistrationId::from(row.registration_id),
            status: row.status.parse().unwrap_or(LineupStatus::Draft),
            declared_by: row.declared_by.map(UserId::from),
            declared_at: row.declared_at,
            locked_at: row.locked_at,
            short_handed: row.short_handed,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<MatchLineupPlayerRow> for MatchLineupPlayer {
    fn from(row: MatchLineupPlayerRow) -> Self {
        Self {
            id: MatchLineupPlayerId::from(row.id),
            lineup_id: MatchLineupId::from(row.lineup_id),
            player_id: PlayerId::from(row.player_id),
            source: row.source.parse().unwrap_or(LineupSource::Declared),
            game_number: row.game_number,
            is_substitute: row.is_substitute,
            was_rostered: row.was_rostered,
            participation_status: row
                .participation_status
                .parse()
                .unwrap_or(ParticipationStatus::Confirmed),
            created_at: row.created_at,
        }
    }
}

// =============================================================================
// Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `MatchLineupRepository` trait.
#[derive(Clone)]
pub struct PgMatchLineupRepository {
    pool: DbPool,
}

impl PgMatchLineupRepository {
    /// Create a new `PostgreSQL` match lineup repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    async fn fetch_players(
        &self,
        lineup_id: MatchLineupId,
    ) -> Result<Vec<MatchLineupPlayer>, DomainError> {
        let rows = sqlx::query_as::<_, MatchLineupPlayerRow>(
            r"SELECT * FROM match_lineup_players WHERE lineup_id = $1
              ORDER BY game_number NULLS FIRST, created_at",
        )
        .bind(lineup_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(MatchLineupPlayer::from).collect())
    }
}

#[async_trait]
impl MatchLineupRepository for PgMatchLineupRepository {
    async fn declare(&self, cmd: DeclareLineupCommand) -> Result<MatchLineup, DomainError> {
        let status = if cmd.submit {
            LineupStatus::Submitted
        } else {
            LineupStatus::Draft
        };

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Upsert the lineup container. A locked lineup is protected by the
        // service; the WHERE guard here refuses to overwrite a lock defensively.
        let lineup = sqlx::query_as::<_, MatchLineupRow>(
            r"
            INSERT INTO match_lineups
                (match_id, registration_id, status, declared_by, declared_at,
                 short_handed, notes, updated_at)
            VALUES ($1, $2, $3, $4, NOW(), $5, $6, NOW())
            ON CONFLICT (match_id, registration_id) DO UPDATE
            SET status       = EXCLUDED.status,
                declared_by  = EXCLUDED.declared_by,
                declared_at  = NOW(),
                short_handed = EXCLUDED.short_handed,
                notes        = EXCLUDED.notes,
                updated_at   = NOW()
            WHERE match_lineups.locked_at IS NULL
            RETURNING *
            ",
        )
        .bind(cmd.match_id.as_uuid())
        .bind(cmd.registration_id.as_uuid())
        .bind(status.to_string())
        .bind(cmd.declared_by.as_uuid())
        .bind(cmd.short_handed)
        .bind(cmd.notes.as_deref())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| {
            DomainError::InvalidState("Lineup is locked and can no longer be edited".to_string())
        })?;

        let lineup_id = lineup.id;

        // Replace the provisional (declared) rows only. Demo/evidence/admin rows
        // are authoritative and must not be touched by a re-declaration.
        sqlx::query(
            "DELETE FROM match_lineup_players WHERE lineup_id = $1 AND source = 'declared'",
        )
        .bind(lineup_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        for p in &cmd.players {
            sqlx::query(
                r"
                INSERT INTO match_lineup_players
                    (lineup_id, player_id, source, game_number, is_substitute,
                     was_rostered, participation_status)
                VALUES ($1, $2, 'declared', NULL, $3, $4, $5)
                ",
            )
            .bind(lineup_id)
            .bind(p.player_id.as_uuid())
            // A declared non-rostered player is provisionally a substitute.
            .bind(!p.was_rostered)
            .bind(p.was_rostered)
            .bind(p.participation_status.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                if let sqlx::Error::Database(ref db_err) = e
                    && db_err.constraint().is_some()
                {
                    return DomainError::Conflict(
                        "Duplicate player in declared lineup".to_string(),
                    );
                }
                DomainError::Internal(e.to_string())
            })?;
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(MatchLineup::from(lineup))
    }

    async fn find_by_match_registration(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Option<MatchLineup>, DomainError> {
        let row = sqlx::query_as::<_, MatchLineupRow>(
            "SELECT * FROM match_lineups WHERE match_id = $1 AND registration_id = $2",
        )
        .bind(match_id.as_uuid())
        .bind(registration_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(MatchLineup::from))
    }

    async fn list_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchLineupWithPlayers>, DomainError> {
        let lineups = sqlx::query_as::<_, MatchLineupRow>(
            "SELECT * FROM match_lineups WHERE match_id = $1 ORDER BY created_at",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut out = Vec::with_capacity(lineups.len());
        for row in lineups {
            let lineup = MatchLineup::from(row);
            let players = self.fetch_players(lineup.id).await?;
            out.push(MatchLineupWithPlayers { lineup, players });
        }
        Ok(out)
    }

    async fn get_with_players(
        &self,
        id: MatchLineupId,
    ) -> Result<Option<MatchLineupWithPlayers>, DomainError> {
        let row = sqlx::query_as::<_, MatchLineupRow>("SELECT * FROM match_lineups WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let Some(row) = row else { return Ok(None) };
        let lineup = MatchLineup::from(row);
        let players = self.fetch_players(lineup.id).await?;
        Ok(Some(MatchLineupWithPlayers { lineup, players }))
    }

    async fn lock_for_match(&self, match_id: TournamentMatchId) -> Result<u64, DomainError> {
        let res = sqlx::query(
            r"
            UPDATE match_lineups
            SET status = 'locked', locked_at = COALESCE(locked_at, NOW()), updated_at = NOW()
            WHERE match_id = $1 AND status <> 'locked'
            ",
        )
        .bind(match_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(res.rows_affected())
    }

    async fn materialize_demo(
        &self,
        cmd: MaterializeDemoLineup,
    ) -> Result<MatchLineup, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Ensure a lineup container exists. A demo lands after the match is
        // played, so the container is locked (opponent-visible) on creation.
        let lineup = sqlx::query_as::<_, MatchLineupRow>(
            r"
            INSERT INTO match_lineups (match_id, registration_id, status, locked_at, updated_at)
            VALUES ($1, $2, 'locked', NOW(), NOW())
            ON CONFLICT (match_id, registration_id) DO UPDATE
            SET updated_at = NOW()
            RETURNING *
            ",
        )
        .bind(cmd.match_id.as_uuid())
        .bind(cmd.registration_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let lineup_id = lineup.id;

        // Replace demo rows for this game_number (per-map, re-ingestion safe).
        sqlx::query(
            r"DELETE FROM match_lineup_players
              WHERE lineup_id = $1 AND source = 'demo'
                AND game_number IS NOT DISTINCT FROM $2",
        )
        .bind(lineup_id)
        .bind(cmd.game_number)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        for p in &cmd.players {
            sqlx::query(
                r"
                INSERT INTO match_lineup_players
                    (lineup_id, player_id, source, game_number, is_substitute,
                     was_rostered, participation_status)
                VALUES ($1, $2, 'demo', $3, $4, $5, $6)
                ON CONFLICT (lineup_id, player_id, game_number, source) DO NOTHING
                ",
            )
            .bind(lineup_id)
            .bind(p.player_id.as_uuid())
            .bind(cmd.game_number)
            // Authoritative: a demo player not on the roster is a substitute.
            .bind(!p.was_rostered)
            .bind(p.was_rostered)
            .bind(p.participation_status.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(MatchLineup::from(lineup))
    }

    async fn distinct_participants(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<PlayerId>, DomainError> {
        let ids = sqlx::query_scalar::<_, uuid::Uuid>(
            r"
            SELECT DISTINCT mlp.player_id
            FROM match_lineup_players mlp
            JOIN match_lineups ml ON ml.id = mlp.lineup_id
            WHERE ml.match_id = $1 AND ml.registration_id = $2 AND mlp.source = 'demo'
            ",
        )
        .bind(match_id.as_uuid())
        .bind(registration_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(ids.into_iter().map(PlayerId::from).collect())
    }

    async fn is_lineup_required_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<bool, DomainError> {
        let required = sqlx::query_scalar::<_, bool>(
            r"
            SELECT COALESCE(ls.lineup_required, false)
            FROM tournament_matches tm
            JOIN tournaments t ON t.id = tm.tournament_id
            LEFT JOIN league_seasons ls ON ls.id = t.season_id
            WHERE tm.id = $1
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .unwrap_or(false);
        Ok(required)
    }

    async fn list_players_by_source(
        &self,
        match_id: TournamentMatchId,
        source: LineupSource,
    ) -> Result<Vec<MatchLineupPlayer>, DomainError> {
        let rows = sqlx::query_as::<_, MatchLineupPlayerRow>(
            r"
            SELECT mlp.* FROM match_lineup_players mlp
            JOIN match_lineups ml ON ml.id = mlp.lineup_id
            WHERE ml.match_id = $1 AND mlp.source = $2
            ORDER BY mlp.game_number NULLS FIRST, mlp.created_at
            ",
        )
        .bind(match_id.as_uuid())
        .bind(source.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(MatchLineupPlayer::from).collect())
    }
}
