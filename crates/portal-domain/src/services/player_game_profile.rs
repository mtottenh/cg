//! Player game profile service.

use crate::entities::PlayerGameProfile;
use crate::repositories::PlayerGameProfileRepository;
use portal_core::{DomainError, GameId, PlayerId, TournamentMatchId};
use std::sync::Arc;
use tracing::instrument;

/// Service for player game profile business logic.
pub struct PlayerGameProfileService<PGPR>
where
    PGPR: PlayerGameProfileRepository,
{
    profile_repo: Arc<PGPR>,
}

impl<PGPR: PlayerGameProfileRepository> PlayerGameProfileService<PGPR> {
    /// Create a new player game profile service.
    pub const fn new(profile_repo: Arc<PGPR>) -> Self {
        Self { profile_repo }
    }

    /// Get a player's profile for a specific game.
    #[instrument(skip(self))]
    pub async fn get_profile(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<PlayerGameProfile>, DomainError> {
        self.profile_repo
            .find_by_player_and_game(player_id, game_id)
            .await
    }

    /// List all game profiles for a player.
    #[instrument(skip(self))]
    pub async fn list_profiles(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerGameProfile>, DomainError> {
        self.profile_repo.list_by_player(player_id).await
    }

    /// Batch-fetch profiles for multiple players in a single game.
    #[instrument(skip(self, player_ids))]
    pub async fn find_by_players_and_game(
        &self,
        player_ids: &[PlayerId],
        game_id: GameId,
    ) -> Result<Vec<PlayerGameProfile>, DomainError> {
        self.profile_repo
            .find_by_players_and_game(player_ids, game_id)
            .await
    }

    /// Update stats after a match completes.
    ///
    /// Ensures the profile exists (via find_or_create) before updating. The
    /// update is idempotent per `(player_id, match_id)`, so a saga re-drive
    /// counts the match once.
    #[instrument(skip(self, new_stats))]
    pub async fn update_stats_after_match(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        match_id: TournamentMatchId,
        new_stats: serde_json::Value,
        is_win: bool,
        is_loss: bool,
        is_draw: bool,
    ) -> Result<PlayerGameProfile, DomainError> {
        // Ensure profile exists
        self.profile_repo.find_or_create(player_id, game_id).await?;

        self.profile_repo
            .update_stats_after_match(
                player_id, game_id, match_id, &new_stats, is_win, is_loss, is_draw,
            )
            .await
    }

    /// Ensure a player game profile exists (find or create).
    #[instrument(skip(self))]
    pub async fn ensure_profile_exists(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<PlayerGameProfile, DomainError> {
        self.profile_repo.find_or_create(player_id, game_id).await
    }

    /// Update a player's rating and optionally rank tier for a game.
    #[instrument(skip(self))]
    pub async fn update_rating(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        rating: i32,
        rating_deviation: i32,
        volatility: f64,
        rank_tier: Option<String>,
    ) -> Result<(), DomainError> {
        self.profile_repo
            .update_rating(
                player_id,
                game_id,
                rating,
                rating_deviation,
                volatility,
                rank_tier,
            )
            .await
    }
}

impl<PGPR: PlayerGameProfileRepository> Clone for PlayerGameProfileService<PGPR> {
    fn clone(&self) -> Self {
        Self {
            profile_repo: Arc::clone(&self.profile_repo),
        }
    }
}
