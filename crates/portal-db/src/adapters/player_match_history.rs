//! Player match history repository adapter.

use crate::DbPool;
use crate::entities::player_match_history::PlayerMatchHistoryRow;
use async_trait::async_trait;
use portal_core::{DiscoveredMatchId, DomainError, GameId, PlayerId, PlayerMatchHistoryId};
use portal_domain::entities::player_match_history::PlayerMatchHistory;
use portal_domain::repositories::player_match_history::{
    CreatePlayerMatchHistory, PlayerMatchHistoryRepository,
};
use portal_domain::repositories::player_mm_stats::AccumulateMatchStats;

impl From<PlayerMatchHistoryRow> for PlayerMatchHistory {
    fn from(row: PlayerMatchHistoryRow) -> Self {
        Self {
            id: PlayerMatchHistoryId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            game_id: GameId::from(row.game_id),
            discovered_match_id: DiscoveredMatchId::from(row.discovered_match_id),
            map: row.map,
            match_time: row.match_time,
            team_scores: row.team_scores,
            match_duration_secs: row.match_duration_secs,
            match_result: row.match_result,
            kills: row.kills,
            deaths: row.deaths,
            assists: row.assists,
            score: row.score,
            headshots: row.headshots,
            mvps: row.mvps,
            entry_3k: row.entry_3k,
            entry_4k: row.entry_4k,
            entry_5k: row.entry_5k,
            created_at: row.created_at,
        }
    }
}

#[derive(Clone)]
pub struct PgPlayerMatchHistoryRepository {
    pool: DbPool,
}

impl PgPlayerMatchHistoryRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlayerMatchHistoryRepository for PgPlayerMatchHistoryRepository {
    async fn create(
        &self,
        input: CreatePlayerMatchHistory,
    ) -> Result<(PlayerMatchHistory, bool), DomainError> {
        // ON CONFLICT DO NOTHING + RETURNING yields a row only when a NEW row
        // was inserted; a conflict (the entry already exists) returns nothing.
        // This is the match-scoped idempotency signal callers gate on.
        let inserted = sqlx::query_as::<_, PlayerMatchHistoryRow>(
            r"
            INSERT INTO player_match_history (
                player_id, game_id, discovered_match_id,
                map, match_time, team_scores, match_duration_secs, match_result,
                kills, deaths, assists, score, headshots, mvps,
                entry_3k, entry_4k, entry_5k
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            ON CONFLICT (player_id, discovered_match_id) DO NOTHING
            RETURNING *
            ",
        )
        .bind(input.player_id.as_uuid())
        .bind(input.game_id.as_uuid())
        .bind(input.discovered_match_id.as_uuid())
        .bind(&input.map)
        .bind(input.match_time)
        .bind(&input.team_scores)
        .bind(input.match_duration_secs)
        .bind(&input.match_result)
        .bind(input.kills)
        .bind(input.deaths)
        .bind(input.assists)
        .bind(input.score)
        .bind(input.headshots)
        .bind(input.mvps)
        .bind(input.entry_3k)
        .bind(input.entry_4k)
        .bind(input.entry_5k)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if let Some(row) = inserted {
            return Ok((PlayerMatchHistory::from(row), true));
        }

        // Conflict: return the pre-existing row and signal "not new".
        let existing = sqlx::query_as::<_, PlayerMatchHistoryRow>(
            r"
            SELECT * FROM player_match_history
            WHERE player_id = $1 AND discovered_match_id = $2
            ",
        )
        .bind(input.player_id.as_uuid())
        .bind(input.discovered_match_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((PlayerMatchHistory::from(existing), false))
    }

    async fn claim_and_accumulate(
        &self,
        input: CreatePlayerMatchHistory,
        stats: &AccumulateMatchStats,
    ) -> Result<bool, DomainError> {
        // ONE transaction: the history-row idempotency claim and the aggregate
        // accumulate commit together or not at all. If the accumulate fails
        // (e.g. INTEGER overflow), the history insert rolls back with it, so a
        // retry re-claims the row and applies both exactly once — closing the
        // split-autocommit residual where a committed-but-orphaned history row
        // suppressed the accumulate forever.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to begin transaction: {e}")))?;

        // Idempotency claim: a row comes back only when NEWLY inserted; a
        // conflict (already delivered) returns nothing.
        let inserted = sqlx::query_as::<_, PlayerMatchHistoryRow>(
            r"
            INSERT INTO player_match_history (
                player_id, game_id, discovered_match_id,
                map, match_time, team_scores, match_duration_secs, match_result,
                kills, deaths, assists, score, headshots, mvps,
                entry_3k, entry_4k, entry_5k
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            ON CONFLICT (player_id, discovered_match_id) DO NOTHING
            RETURNING *
            ",
        )
        .bind(input.player_id.as_uuid())
        .bind(input.game_id.as_uuid())
        .bind(input.discovered_match_id.as_uuid())
        .bind(&input.map)
        .bind(input.match_time)
        .bind(&input.team_scores)
        .bind(input.match_duration_secs)
        .bind(&input.match_result)
        .bind(input.kills)
        .bind(input.deaths)
        .bind(input.assists)
        .bind(input.score)
        .bind(input.headshots)
        .bind(input.mvps)
        .bind(input.entry_3k)
        .bind(input.entry_4k)
        .bind(input.entry_5k)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if inserted.is_none() {
            // Re-delivery: nothing claimed, so skip the accumulate. No writes
            // landed; the commit is a no-op.
            tx.commit()
                .await
                .map_err(|e| DomainError::Internal(format!("Failed to commit: {e}")))?;
            return Ok(false);
        }

        // Newly claimed → accumulate the aggregate on the SAME transaction.
        let win_inc: i32 = i32::from(stats.is_win);
        let loss_inc: i32 = i32::from(stats.is_loss);
        let draw_inc: i32 = i32::from(stats.is_draw);

        sqlx::query(
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
            ",
        )
        .bind(input.player_id.as_uuid())
        .bind(input.game_id.as_uuid())
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
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to commit: {e}")))?;

        Ok(true)
    }

    async fn list_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerMatchHistory>, DomainError> {
        let rows = sqlx::query_as::<_, PlayerMatchHistoryRow>(
            r"
            SELECT * FROM player_match_history
            WHERE player_id = $1 AND game_id = $2
            ORDER BY match_time DESC NULLS LAST
            LIMIT $3 OFFSET $4
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(PlayerMatchHistory::from).collect())
    }
}
