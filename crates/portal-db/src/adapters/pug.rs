//! PUG and ad-hoc team repository adapters.

use crate::DbPool;
use crate::entities::pug::{
    AdhocTeamMemberRow, AdhocTeamRow, PugPlayerRow, PugRow, PugWheelEntryRow, PugWheelSpinRow,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::types::PugStatus;
use portal_core::{
    AdhocTeamId, DomainError, GameId, PlayerId, PugId, PugWheelSpinId, TournamentId,
    TournamentMatchId, UserId,
};
use portal_domain::entities::pug::{
    AdhocTeam, AdhocTeamMember, Pug, PugPlayer, PugPlayerAggregates, PugWheelEntry, PugWheelSpin,
};
use portal_domain::repositories::pug::{
    AdhocTeamRepository, CreatePug, CreateWheelSpin, PugRepository,
};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<PugRow> for Pug {
    fn from(row: PugRow) -> Self {
        Self {
            id: PugId::from(row.id),
            game_id: GameId::from(row.game_id),
            created_by_user_id: UserId::from(row.created_by_user_id),
            join_code: row.join_code,
            status: row.status.parse().unwrap_or_default(),
            match_format: row.match_format.parse().unwrap_or_default(),
            map_selection_mode: row.map_selection_mode.parse().unwrap_or_default(),
            side_selection_mode: row.side_selection_mode.parse().unwrap_or_default(),
            team_size: row.team_size,
            region: row.region,
            map_pool: row.map_pool,
            listed: row.listed,
            tournament_id: row.tournament_id.map(TournamentId::from),
            match_id: row.match_id.map(TournamentMatchId::from),
            winner_team: row.winner_team,
            team1_score: row.team1_score,
            team2_score: row.team2_score,
            completed_at: row.completed_at,
            expires_at: row.expires_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<PugPlayerRow> for PugPlayer {
    fn from(row: PugPlayerRow) -> Self {
        Self {
            pug_id: PugId::from(row.pug_id),
            player_id: PlayerId::from(row.player_id),
            user_id: row.user_id.map(UserId::from),
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            has_steam_id: row.has_steam_id,
            team: row.team,
            is_captain: row.is_captain,
            joined_at: row.joined_at,
        }
    }
}

impl From<PugWheelEntryRow> for PugWheelEntry {
    fn from(row: PugWheelEntryRow) -> Self {
        Self {
            pug_id: PugId::from(row.pug_id),
            player_id: PlayerId::from(row.player_id),
            player_name: row.player_name,
            map_id: row.map_id,
            created_at: row.created_at,
        }
    }
}

impl From<PugWheelSpinRow> for PugWheelSpin {
    fn from(row: PugWheelSpinRow) -> Self {
        Self {
            id: PugWheelSpinId::from(row.id),
            pug_id: PugId::from(row.pug_id),
            game_number: row.game_number,
            entries: row.entries,
            winner_map_id: row.winner_map_id,
            spin_seed: row.spin_seed,
            spun_by_player_id: row.spun_by_player_id.map(PlayerId::from),
            spun_at: row.spun_at,
        }
    }
}

impl From<AdhocTeamRow> for AdhocTeam {
    fn from(row: AdhocTeamRow) -> Self {
        Self {
            id: AdhocTeamId::from(row.id),
            tournament_id: TournamentId::from(row.tournament_id),
            name: row.name,
            created_at: row.created_at,
        }
    }
}

impl From<AdhocTeamMemberRow> for AdhocTeamMember {
    fn from(row: AdhocTeamMemberRow) -> Self {
        Self {
            adhoc_team_id: AdhocTeamId::from(row.adhoc_team_id),
            player_id: PlayerId::from(row.player_id),
            is_captain: row.is_captain,
        }
    }
}

/// SELECT list for `pug_players` joined with `players` display info.
const PUG_PLAYER_SELECT: &str = r"
    SELECT pp.pug_id, pp.player_id, p.user_id, p.display_name, p.avatar_url,
           (p.steam_id IS NOT NULL OR p.steam_id_64 IS NOT NULL) AS has_steam_id,
           pp.team, pp.is_captain, pp.joined_at
      FROM pug_players pp
      JOIN players p ON p.id = pp.player_id
";

// =============================================================================
// PUG Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `PugRepository` trait.
#[derive(Clone)]
pub struct PgPugRepository {
    pool: DbPool,
}

impl PgPugRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PugRepository for PgPugRepository {
    async fn create(&self, cmd: CreatePug) -> Result<Pug, DomainError> {
        let row = sqlx::query_as::<_, PugRow>(
            r"
            INSERT INTO pugs (
                game_id, created_by_user_id, join_code, match_format,
                map_selection_mode, side_selection_mode, team_size, region,
                map_pool, listed, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(cmd.game_id.as_uuid())
        .bind(cmd.created_by_user_id.as_uuid())
        .bind(&cmd.join_code)
        .bind(cmd.match_format.to_string())
        .bind(cmd.map_selection_mode.to_string())
        .bind(cmd.side_selection_mode.to_string())
        .bind(cmd.team_size)
        .bind(&cmd.region)
        .bind(&cmd.map_pool)
        .bind(cmd.listed)
        .bind(cmd.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.into())
    }

    async fn find_by_id(&self, id: PugId) -> Result<Option<Pug>, DomainError> {
        let row = sqlx::query_as::<_, PugRow>("SELECT * FROM pugs WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(Into::into))
    }

    async fn find_by_join_code(&self, join_code: &str) -> Result<Option<Pug>, DomainError> {
        let row = sqlx::query_as::<_, PugRow>("SELECT * FROM pugs WHERE join_code = $1")
            .bind(join_code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(Into::into))
    }

    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<Pug>, DomainError> {
        let row = sqlx::query_as::<_, PugRow>("SELECT * FROM pugs WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(Into::into))
    }

    async fn list_by_participant(
        &self,
        player_id: PlayerId,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError> {
        let rows = sqlx::query_as::<_, PugRow>(
            r"
            SELECT pg.* FROM pugs pg
              JOIN pug_players pp ON pp.pug_id = pg.id
             WHERE pp.player_id = $1
             ORDER BY pg.created_at DESC
             LIMIT $2
            ",
        )
        .bind(player_id.as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count_active_created_by(&self, user_id: UserId) -> Result<i64, DomainError> {
        let count: i64 = sqlx::query_scalar(
            r"
            SELECT COUNT(*) FROM pugs
             WHERE created_by_user_id = $1
               AND status NOT IN ('completed', 'cancelled', 'expired')
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(count)
    }

    async fn list_open_listed(
        &self,
        game_id: Option<GameId>,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError> {
        let rows = sqlx::query_as::<_, PugRow>(
            r"
            SELECT * FROM pugs
             WHERE status = 'gathering' AND listed
               AND ($1::uuid IS NULL OR game_id = $1)
             ORDER BY created_at DESC
             LIMIT $2
            ",
        )
        .bind(game_id.map(|id| id.as_uuid()))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_recent_completed(&self, limit: i64) -> Result<Vec<Pug>, DomainError> {
        let rows = sqlx::query_as::<_, PugRow>(
            r"
            SELECT * FROM pugs
             WHERE status = 'completed'
             ORDER BY completed_at DESC NULLS LAST
             LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn transition_status(
        &self,
        id: PugId,
        expected: PugStatus,
        next: PugStatus,
    ) -> Result<Option<Pug>, DomainError> {
        let row = sqlx::query_as::<_, PugRow>(
            "UPDATE pugs SET status = $3 WHERE id = $1 AND status = $2 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(expected.to_string())
        .bind(next.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(Into::into))
    }

    async fn set_status(&self, id: PugId, status: PugStatus) -> Result<(), DomainError> {
        sqlx::query("UPDATE pugs SET status = $2 WHERE id = $1")
            .bind(id.as_uuid())
            .bind(status.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn set_join_code(&self, id: PugId, join_code: &str) -> Result<(), DomainError> {
        sqlx::query("UPDATE pugs SET join_code = $2 WHERE id = $1")
            .bind(id.as_uuid())
            .bind(join_code)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn set_materialized(
        &self,
        id: PugId,
        tournament_id: TournamentId,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE pugs
               SET tournament_id = $2, match_id = $3, status = 'map_selection'
             WHERE id = $1 AND status = 'gathering'
            ",
        )
        .bind(id.as_uuid())
        .bind(tournament_id.as_uuid())
        .bind(match_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn set_result(
        &self,
        id: PugId,
        winner_team: i16,
        team1_score: i32,
        team2_score: i32,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE pugs
               SET winner_team = $2, team1_score = $3, team2_score = $4,
                   completed_at = NOW(), status = 'completed'
             WHERE id = $1
            ",
        )
        .bind(id.as_uuid())
        .bind(winner_team)
        .bind(team1_score)
        .bind(team2_score)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn list_expired_gathering(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError> {
        let rows = sqlx::query_as::<_, PugRow>(
            r"
            SELECT * FROM pugs
             WHERE status = 'gathering' AND expires_at < $1
             ORDER BY expires_at
             LIMIT $2
            ",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_stalled_materialized(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<Pug>, DomainError> {
        let rows = sqlx::query_as::<_, PugRow>(
            r"
            SELECT * FROM pugs
             WHERE status IN ('map_selection', 'awaiting_server')
               AND updated_at < $1
             ORDER BY updated_at
             LIMIT $2
            ",
        )
        .bind(cutoff)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    // -- players --------------------------------------------------------------

    async fn add_player(&self, pug_id: PugId, player_id: PlayerId) -> Result<(), DomainError> {
        sqlx::query(
            r"
            INSERT INTO pug_players (pug_id, player_id)
            VALUES ($1, $2)
            ON CONFLICT (pug_id, player_id) DO NOTHING
            ",
        )
        .bind(pug_id.as_uuid())
        .bind(player_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn remove_player(&self, pug_id: PugId, player_id: PlayerId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM pug_players WHERE pug_id = $1 AND player_id = $2")
            .bind(pug_id.as_uuid())
            .bind(player_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        // Their nomination goes with them.
        sqlx::query("DELETE FROM pug_wheel_entries WHERE pug_id = $1 AND player_id = $2")
            .bind(pug_id.as_uuid())
            .bind(player_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn list_players(&self, pug_id: PugId) -> Result<Vec<PugPlayer>, DomainError> {
        let sql = format!("{PUG_PLAYER_SELECT} WHERE pp.pug_id = $1 ORDER BY pp.joined_at");
        let rows = sqlx::query_as::<_, PugPlayerRow>(&sql)
            .bind(pug_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn is_participant(
        &self,
        pug_id: PugId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pug_players WHERE pug_id = $1 AND player_id = $2)",
        )
        .bind(pug_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(exists)
    }

    async fn count_players(&self, pug_id: PugId) -> Result<i64, DomainError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pug_players WHERE pug_id = $1")
            .bind(pug_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(count)
    }

    async fn set_player_team(
        &self,
        pug_id: PugId,
        player_id: PlayerId,
        team: Option<i16>,
    ) -> Result<(), DomainError> {
        sqlx::query("UPDATE pug_players SET team = $3 WHERE pug_id = $1 AND player_id = $2")
            .bind(pug_id.as_uuid())
            .bind(player_id.as_uuid())
            .bind(team)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn set_player_captain(
        &self,
        pug_id: PugId,
        player_id: PlayerId,
        is_captain: bool,
    ) -> Result<(), DomainError> {
        sqlx::query("UPDATE pug_players SET is_captain = $3 WHERE pug_id = $1 AND player_id = $2")
            .bind(pug_id.as_uuid())
            .bind(player_id.as_uuid())
            .bind(is_captain)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn assign_teams(
        &self,
        pug_id: PugId,
        assignments: &[(PlayerId, Option<i16>)],
    ) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        for (player_id, team) in assignments {
            sqlx::query("UPDATE pug_players SET team = $3 WHERE pug_id = $1 AND player_id = $2")
                .bind(pug_id.as_uuid())
                .bind(player_id.as_uuid())
                .bind(team)
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        }
        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn swap_teams(&self, pug_id: PugId) -> Result<(), DomainError> {
        sqlx::query("UPDATE pug_players SET team = 3 - team WHERE pug_id = $1 AND team IS NOT NULL")
            .bind(pug_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    // -- wheel -----------------------------------------------------------------

    async fn upsert_wheel_entry(
        &self,
        pug_id: PugId,
        player_id: PlayerId,
        map_id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            INSERT INTO pug_wheel_entries (pug_id, player_id, map_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (pug_id, player_id)
            DO UPDATE SET map_id = EXCLUDED.map_id, created_at = NOW()
            ",
        )
        .bind(pug_id.as_uuid())
        .bind(player_id.as_uuid())
        .bind(map_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn list_wheel_entries(
        &self,
        pug_id: PugId,
    ) -> Result<Vec<PugWheelEntry>, DomainError> {
        let rows = sqlx::query_as::<_, PugWheelEntryRow>(
            r"
            SELECT we.pug_id, we.player_id, p.display_name AS player_name,
                   we.map_id, we.created_at
              FROM pug_wheel_entries we
              JOIN players p ON p.id = we.player_id
             WHERE we.pug_id = $1
             ORDER BY we.created_at
            ",
        )
        .bind(pug_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn record_spin(&self, cmd: CreateWheelSpin) -> Result<PugWheelSpin, DomainError> {
        let row = sqlx::query_as::<_, PugWheelSpinRow>(
            r"
            INSERT INTO pug_wheel_spins (
                pug_id, game_number, entries, winner_map_id, spin_seed, spun_by_player_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (pug_id, game_number) DO UPDATE SET game_number = EXCLUDED.game_number
            RETURNING *
            ",
        )
        .bind(cmd.pug_id.as_uuid())
        .bind(cmd.game_number)
        .bind(&cmd.entries)
        .bind(&cmd.winner_map_id)
        .bind(cmd.spin_seed)
        .bind(cmd.spun_by_player_id.map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.into())
    }

    async fn list_spins(&self, pug_id: PugId) -> Result<Vec<PugWheelSpin>, DomainError> {
        let rows = sqlx::query_as::<_, PugWheelSpinRow>(
            "SELECT * FROM pug_wheel_spins WHERE pug_id = $1 ORDER BY game_number",
        )
        .bind(pug_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn player_aggregates(
        &self,
        player_id: PlayerId,
    ) -> Result<PugPlayerAggregates, DomainError> {
        let (matches_played, wins, losses): (i64, i64, i64) = sqlx::query_as(
            r"
            SELECT COUNT(*)::bigint,
                   COUNT(*) FILTER (WHERE pp.team = pg.winner_team)::bigint,
                   COUNT(*) FILTER (
                       WHERE pp.team IS NOT NULL AND pp.team <> pg.winner_team
                   )::bigint
              FROM pugs pg
              JOIN pug_players pp ON pp.pug_id = pg.id
             WHERE pp.player_id = $1
               AND pg.status = 'completed'
               AND pg.winner_team IS NOT NULL
            ",
        )
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let (demos_counted, kills, deaths, assists, avg_adr, avg_hs): (
            i64,
            i64,
            i64,
            i64,
            f64,
            f64,
        ) = sqlx::query_as(
            r"
            SELECT COUNT(*)::bigint,
                   COALESCE(SUM(dp.kills), 0)::bigint,
                   COALESCE(SUM(dp.deaths), 0)::bigint,
                   COALESCE(SUM(dp.assists), 0)::bigint,
                   COALESCE(AVG(dp.adr), 0)::float8,
                   COALESCE(AVG(dp.hs_percentage), 0)::float8
              FROM demo_players dp
              JOIN demos d ON d.id = dp.demo_id
             WHERE d.category = 'pug' AND dp.player_id = $1
            ",
        )
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(PugPlayerAggregates {
            matches_played,
            wins,
            losses,
            demos_counted,
            kills,
            deaths,
            assists,
            avg_adr,
            avg_hs_percentage: avg_hs,
        })
    }
}

// =============================================================================
// Ad-hoc Team Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `AdhocTeamRepository` trait.
#[derive(Clone)]
pub struct PgAdhocTeamRepository {
    pool: DbPool,
}

impl PgAdhocTeamRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdhocTeamRepository for PgAdhocTeamRepository {
    async fn create(
        &self,
        tournament_id: TournamentId,
        name: &str,
        members: &[(PlayerId, bool)],
    ) -> Result<AdhocTeam, DomainError> {
        let mut tx = self.pool.begin().await.map_err(|e| DomainError::Internal(e.to_string()))?;

        let team = sqlx::query_as::<_, AdhocTeamRow>(
            r"
            INSERT INTO tournament_adhoc_teams (tournament_id, name)
            VALUES ($1, $2)
            RETURNING *
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(name)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        for (player_id, is_captain) in members {
            sqlx::query(
                r"
                INSERT INTO tournament_adhoc_team_members (adhoc_team_id, player_id, is_captain)
                VALUES ($1, $2, $3)
                ",
            )
            .bind(team.id)
            .bind(player_id.as_uuid())
            .bind(is_captain)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(team.into())
    }

    async fn find_by_id(&self, id: AdhocTeamId) -> Result<Option<AdhocTeam>, DomainError> {
        let row = sqlx::query_as::<_, AdhocTeamRow>(
            "SELECT * FROM tournament_adhoc_teams WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(Into::into))
    }

    async fn list_members(&self, id: AdhocTeamId) -> Result<Vec<AdhocTeamMember>, DomainError> {
        let rows = sqlx::query_as::<_, AdhocTeamMemberRow>(
            r"
            SELECT adhoc_team_id, player_id, is_captain
              FROM tournament_adhoc_team_members
             WHERE adhoc_team_id = $1
            ",
        )
        .bind(id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn is_member(&self, id: AdhocTeamId, player_id: PlayerId) -> Result<bool, DomainError> {
        let exists: bool = sqlx::query_scalar(
            r"
            SELECT EXISTS(
                SELECT 1 FROM tournament_adhoc_team_members
                 WHERE adhoc_team_id = $1 AND player_id = $2
            )
            ",
        )
        .bind(id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(exists)
    }

    async fn is_captain(&self, id: AdhocTeamId, player_id: PlayerId) -> Result<bool, DomainError> {
        let is_captain: bool = sqlx::query_scalar(
            r"
            SELECT COALESCE(
                (SELECT is_captain FROM tournament_adhoc_team_members
                  WHERE adhoc_team_id = $1 AND player_id = $2),
                FALSE
            )
            ",
        )
        .bind(id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(is_captain)
    }
}
