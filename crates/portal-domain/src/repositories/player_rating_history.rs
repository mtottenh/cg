//! Player rating history repository trait.

use crate::entities::PlayerRatingHistory;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{DomainError, GameId, PlayerId};

/// Aggregate rating statistics for a player in a game.
///
/// Computed from the rating history table joined with the current
/// profile for current/peak values.
#[derive(Debug, Clone)]
pub struct RatingStats {
    /// Current rating from player_game_profiles.
    pub current_rating: i32,
    /// All-time peak rating from player_game_profiles.
    pub peak_rating: i32,
    /// Average rating across all history entries.
    pub average_rating: f64,
    /// Median rating across all history entries.
    pub median_rating: f64,
    /// Number of history data points.
    pub data_points: i64,
}

/// Input for creating a new rating history entry.
pub struct CreatePlayerRatingHistory {
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub rating: i32,
    pub source: String,
    pub recorded_at: DateTime<Utc>,
}

/// Repository trait for player rating history operations.
#[async_trait]
pub trait PlayerRatingHistoryRepository: Send + Sync + 'static {
    /// Insert a new rating history entry.
    async fn create(
        &self,
        input: CreatePlayerRatingHistory,
    ) -> Result<PlayerRatingHistory, DomainError>;

    /// List rating history for a player in a game, ordered by recorded_at DESC.
    async fn list_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        limit: Option<i64>,
    ) -> Result<Vec<PlayerRatingHistory>, DomainError>;

    /// Get aggregate rating statistics for a player in a game.
    ///
    /// Joins player_game_profiles (for current/peak) with aggregates
    /// from the history table. Returns None if the player has no profile
    /// for this game.
    async fn get_rating_stats(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<RatingStats>, DomainError>;
}
