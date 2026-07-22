//! Player MM stats repository adapter.

use crate::DbPool;
use crate::entities::player_mm_stats::PlayerMmStatsRow;
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, PlayerMmStatsId};
use portal_domain::entities::player_mm_stats::PlayerMmStats;
use portal_domain::repositories::player_mm_stats::{AccumulateMatchStats, PlayerMmStatsRepository};

impl From<PlayerMmStatsRow> for PlayerMmStats {
    fn from(row: PlayerMmStatsRow) -> Self {
        Self {
            id: PlayerMmStatsId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            game_id: GameId::from(row.game_id),
            matches_played: row.matches_played,
            wins: row.wins,
            losses: row.losses,
            draws: row.draws,
            kills: row.kills,
            deaths: row.deaths,
            assists: row.assists,
            headshots: row.headshots,
            mvps: row.mvps,
            entry_3k: row.entry_3k,
            entry_4k: row.entry_4k,
            entry_5k: row.entry_5k,
            total_score: row.total_score,
            total_duration_secs: row.total_duration_secs,
            first_match_at: row.first_match_at,
            last_match_at: row.last_match_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct PgPlayerMmStatsRepository {
    pool: DbPool,
}

impl PgPlayerMmStatsRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlayerMmStatsRepository for PgPlayerMmStatsRepository {
    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<PlayerMmStats>, DomainError> {
        let row = sqlx::query_as::<_, PlayerMmStatsRow>(
            "SELECT * FROM player_mm_stats WHERE player_id = $1 AND game_id = $2",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(PlayerMmStats::from))
    }

    async fn accumulate_match_stats(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        stats: &AccumulateMatchStats,
    ) -> Result<PlayerMmStats, DomainError> {
        let win_inc: i32 = i32::from(stats.is_win);
        let loss_inc: i32 = i32::from(stats.is_loss);
        let draw_inc: i32 = i32::from(stats.is_draw);

        let row = sqlx::query_as::<_, PlayerMmStatsRow>(
            r"
            INSERT INTO player_mm_stats (player_id, game_id,
                matches_played, wins, losses, draws,
                kills, deaths, assists, headshots, mvps,
                entry_3k, entry_4k, entry_5k,
                total_score, total_duration_secs,
                first_match_at, last_match_at)
            VALUES ($1, $2,
                1, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13,
                $14, $15,
                $16, $16)
            ON CONFLICT (player_id, game_id) DO UPDATE SET
                matches_played = player_mm_stats.matches_played + 1,
                wins = player_mm_stats.wins + EXCLUDED.wins,
                losses = player_mm_stats.losses + EXCLUDED.losses,
                draws = player_mm_stats.draws + EXCLUDED.draws,
                kills = player_mm_stats.kills + EXCLUDED.kills,
                deaths = player_mm_stats.deaths + EXCLUDED.deaths,
                assists = player_mm_stats.assists + EXCLUDED.assists,
                headshots = player_mm_stats.headshots + EXCLUDED.headshots,
                mvps = player_mm_stats.mvps + EXCLUDED.mvps,
                entry_3k = player_mm_stats.entry_3k + EXCLUDED.entry_3k,
                entry_4k = player_mm_stats.entry_4k + EXCLUDED.entry_4k,
                entry_5k = player_mm_stats.entry_5k + EXCLUDED.entry_5k,
                total_score = player_mm_stats.total_score + EXCLUDED.total_score,
                total_duration_secs = player_mm_stats.total_duration_secs + EXCLUDED.total_duration_secs,
                first_match_at = COALESCE(player_mm_stats.first_match_at, EXCLUDED.first_match_at),
                last_match_at = GREATEST(player_mm_stats.last_match_at, EXCLUDED.last_match_at)
            RETURNING *
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .bind(win_inc)
        .bind(loss_inc)
        .bind(draw_inc)
        .bind(stats.kills)
        .bind(stats.deaths)
        .bind(stats.assists)
        .bind(stats.headshots)
        .bind(stats.mvps)
        .bind(stats.entry_3k)
        .bind(stats.entry_4k)
        .bind(stats.entry_5k)
        .bind(stats.score)
        .bind(stats.duration_secs)
        .bind(stats.match_time)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(PlayerMmStats::from(row))
    }
}
