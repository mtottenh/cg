//! Player game profile repository trait.

use crate::entities::PlayerGameProfile;
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, TournamentMatchId};

/// Repository trait for player game profile operations.
#[async_trait]
pub trait PlayerGameProfileRepository: Send + Sync {
    /// Find a profile by player and game.
    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<PlayerGameProfile>, DomainError>;

    /// List all profiles for a player.
    async fn list_by_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerGameProfile>, DomainError>;

    /// Find or create a profile for the given player and game.
    async fn find_or_create(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<PlayerGameProfile, DomainError>;

    /// Update stats and match counts after a match completes.
    ///
    /// Match-scoped idempotency: the update is keyed on `(player_id, match_id)`
    /// via the `player_match_stats_applied` ledger. The counter bump
    /// (`matches_played`, `wins`, `losses`, `win_streak`) is applied at most
    /// once per match per player, so a saga re-drive is a no-op. On replay the
    /// current (unchanged) profile is returned.
    async fn update_stats_after_match(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        match_id: TournamentMatchId,
        new_stats: &serde_json::Value,
        is_win: bool,
        is_loss: bool,
        is_draw: bool,
    ) -> Result<PlayerGameProfile, DomainError>;

    /// Batch-fetch profiles for multiple players in a single game.
    async fn find_by_players_and_game(
        &self,
        player_ids: &[PlayerId],
        game_id: GameId,
    ) -> Result<Vec<PlayerGameProfile>, DomainError>;

    /// Update a player's rating values and optionally their rank tier.
    async fn update_rating(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        rating: i32,
        rating_deviation: i32,
        volatility: f64,
        rank_tier: Option<String>,
    ) -> Result<(), DomainError>;
}
