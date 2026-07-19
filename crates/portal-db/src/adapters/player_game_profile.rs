//! Player game profile repository adapter.

use crate::DbPool;
use crate::entities::PlayerGameProfileRow;
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerGameProfileId, PlayerId};
use portal_domain::entities::PlayerGameProfile;
use portal_domain::repositories::PlayerGameProfileRepository;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<PlayerGameProfileRow> for PlayerGameProfile {
    fn from(row: PlayerGameProfileRow) -> Self {
        Self {
            id: PlayerGameProfileId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            game_id: GameId::from(row.game_id),
            rating: row.rating,
            rating_deviation: row.rating_deviation,
            volatility: row.volatility,
            peak_rating: row.peak_rating,
            peak_rating_at: row.peak_rating_at,
            rank_tier: row.rank_tier,
            rank_division: row.rank_division,
            rank_points: row.rank_points,
            matches_played: row.matches_played,
            wins: row.wins,
            losses: row.losses,
            draws: row.draws,
            win_streak: row.win_streak,
            best_win_streak: row.best_win_streak,
            total_playtime_minutes: row.total_playtime_minutes,
            game_specific_stats: row.game_specific_stats,
            first_match_at: row.first_match_at,
            last_match_at: row.last_match_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `PlayerGameProfileRepository` trait.
#[derive(Clone)]
pub struct PgPlayerGameProfileRepository {
    pool: DbPool,
}

impl PgPlayerGameProfileRepository {
    /// Create a new PostgreSQL player game profile repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlayerGameProfileRepository for PgPlayerGameProfileRepository {
    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<PlayerGameProfile>, DomainError> {
        let row = sqlx::query_as::<_, PlayerGameProfileRow>(
            "SELECT * FROM player_game_profiles WHERE player_id = $1 AND game_id = $2",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(PlayerGameProfile::from))
    }

    async fn list_by_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerGameProfile>, DomainError> {
        let rows = sqlx::query_as::<_, PlayerGameProfileRow>(
            "SELECT * FROM player_game_profiles WHERE player_id = $1 ORDER BY matches_played DESC",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(PlayerGameProfile::from).collect())
    }

    async fn find_or_create(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<PlayerGameProfile, DomainError> {
        let row = sqlx::query_as::<_, PlayerGameProfileRow>(
            r"
            INSERT INTO player_game_profiles (player_id, game_id)
            VALUES ($1, $2)
            ON CONFLICT (player_id, game_id) DO UPDATE SET updated_at = NOW()
            RETURNING *
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(PlayerGameProfile::from(row))
    }

    async fn find_by_players_and_game(
        &self,
        player_ids: &[PlayerId],
        game_id: GameId,
    ) -> Result<Vec<PlayerGameProfile>, DomainError> {
        let uuids: Vec<uuid::Uuid> = player_ids
            .iter()
            .map(portal_core::PlayerId::as_uuid)
            .collect();
        let rows = sqlx::query_as::<_, PlayerGameProfileRow>(
            "SELECT * FROM player_game_profiles WHERE player_id = ANY($1) AND game_id = $2",
        )
        .bind(&uuids)
        .bind(game_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(PlayerGameProfile::from).collect())
    }

    async fn update_stats_after_match(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        new_stats: &serde_json::Value,
        is_win: bool,
        is_loss: bool,
        is_draw: bool,
    ) -> Result<PlayerGameProfile, DomainError> {
        let row = sqlx::query_as::<_, PlayerGameProfileRow>(
            r"
            UPDATE player_game_profiles SET
                game_specific_stats = $3,
                matches_played = matches_played + 1,
                wins = wins + CASE WHEN $4 THEN 1 ELSE 0 END,
                losses = losses + CASE WHEN $5 THEN 1 ELSE 0 END,
                draws = draws + CASE WHEN $6 THEN 1 ELSE 0 END,
                win_streak = CASE WHEN $4 THEN win_streak + 1 ELSE 0 END,
                best_win_streak = GREATEST(best_win_streak, CASE WHEN $4 THEN win_streak + 1 ELSE win_streak END),
                last_match_at = NOW(),
                first_match_at = COALESCE(first_match_at, NOW()),
                updated_at = NOW()
            WHERE player_id = $1 AND game_id = $2
            RETURNING *
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .bind(new_stats)
        .bind(is_win)
        .bind(is_loss)
        .bind(is_draw)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| {
            DomainError::Internal(format!(
                "Player game profile not found for {player_id}/{game_id}"
            ))
        })?;

        Ok(PlayerGameProfile::from(row))
    }

    async fn update_rating(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        rating: i32,
        rating_deviation: i32,
        volatility: f64,
        rank_tier: Option<String>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE player_game_profiles SET
                rating = $3,
                rating_deviation = $4,
                volatility = $5,
                rank_tier = COALESCE($6, rank_tier),
                peak_rating = GREATEST(peak_rating, $3),
                peak_rating_at = CASE WHEN $3 > peak_rating THEN NOW() ELSE peak_rating_at END,
                updated_at = NOW()
            WHERE player_id = $1 AND game_id = $2
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .bind(rating)
        .bind(rating_deviation)
        .bind(volatility)
        .bind(rank_tier)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
