//! Player rating history repository adapter.

use crate::entities::{PlayerRatingHistoryRow, RatingStatsRow};
use crate::DbPool;
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, PlayerRatingHistoryId};
use portal_domain::entities::PlayerRatingHistory;
use portal_domain::repositories::player_rating_history::{
    CreatePlayerRatingHistory, PlayerRatingHistoryRepository, RatingStats,
};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<PlayerRatingHistoryRow> for PlayerRatingHistory {
    fn from(row: PlayerRatingHistoryRow) -> Self {
        Self {
            id: PlayerRatingHistoryId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            game_id: GameId::from(row.game_id),
            rating: row.rating,
            source: row.source,
            recorded_at: row.recorded_at,
            created_at: row.created_at,
        }
    }
}

impl From<RatingStatsRow> for RatingStats {
    fn from(row: RatingStatsRow) -> Self {
        Self {
            current_rating: row.current_rating,
            peak_rating: row.peak_rating,
            average_rating: row.average_rating,
            median_rating: row.median_rating,
            data_points: row.data_points,
        }
    }
}

// =============================================================================
// Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the `PlayerRatingHistoryRepository` trait.
#[derive(Clone)]
pub struct PgPlayerRatingHistoryRepository {
    pool: DbPool,
}

impl PgPlayerRatingHistoryRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlayerRatingHistoryRepository for PgPlayerRatingHistoryRepository {
    async fn create(
        &self,
        input: CreatePlayerRatingHistory,
    ) -> Result<PlayerRatingHistory, DomainError> {
        let row = sqlx::query_as::<_, PlayerRatingHistoryRow>(
            r"
            INSERT INTO player_rating_history (player_id, game_id, rating, source, recorded_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            ",
        )
        .bind(input.player_id.as_uuid())
        .bind(input.game_id.as_uuid())
        .bind(input.rating)
        .bind(&input.source)
        .bind(input.recorded_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(PlayerRatingHistory::from(row))
    }

    async fn list_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        limit: Option<i64>,
    ) -> Result<Vec<PlayerRatingHistory>, DomainError> {
        let rows = sqlx::query_as::<_, PlayerRatingHistoryRow>(
            r"
            SELECT * FROM player_rating_history
            WHERE player_id = $1 AND game_id = $2
            ORDER BY recorded_at DESC
            LIMIT $3
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .bind(limit.unwrap_or(100))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(PlayerRatingHistory::from).collect())
    }

    async fn get_rating_stats(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<RatingStats>, DomainError> {
        let row = sqlx::query_as::<_, RatingStatsRow>(
            r"
            SELECT
                pgp.rating AS current_rating,
                pgp.peak_rating,
                COALESCE(h.avg_rating, pgp.rating::float8) AS average_rating,
                COALESCE(h.median_rating, pgp.rating::float8) AS median_rating,
                COALESCE(h.cnt, 0) AS data_points
            FROM player_game_profiles pgp
            LEFT JOIN LATERAL (
                SELECT
                    AVG(prh.rating)::float8 AS avg_rating,
                    PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY prh.rating)::float8 AS median_rating,
                    COUNT(*)::bigint AS cnt
                FROM player_rating_history prh
                WHERE prh.player_id = pgp.player_id AND prh.game_id = pgp.game_id
            ) h ON true
            WHERE pgp.player_id = $1 AND pgp.game_id = $2
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(RatingStats::from))
    }
}
