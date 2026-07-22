//! Steam tracking repository trait.

use crate::entities::steam_tracking::{SteamTracking, UpdatePollResultCommand};
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, SteamTrackingId};

/// Repository trait for steam tracking operations.
#[async_trait]
pub trait SteamTrackingRepository: Send + Sync {
    /// Find a tracking entry by ID.
    async fn find_by_id(&self, id: SteamTrackingId) -> Result<Option<SteamTracking>, DomainError>;

    /// Find a tracking entry for a player and game.
    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<SteamTracking>, DomainError>;

    /// Create a new tracking entry.
    async fn create(&self, cmd: CreateSteamTracking) -> Result<SteamTracking, DomainError>;

    /// Update the game auth code.
    async fn update_auth_code(
        &self,
        id: SteamTrackingId,
        auth_code: &str,
    ) -> Result<SteamTracking, DomainError>;

    /// Deactivate tracking.
    async fn deactivate(&self, id: SteamTrackingId) -> Result<(), DomainError>;

    /// Delete tracking entry.
    async fn delete(&self, id: SteamTrackingId) -> Result<(), DomainError>;

    /// Get all active tracking entries for a game.
    async fn find_active_by_game(&self, game_id: GameId)
    -> Result<Vec<SteamTracking>, DomainError>;

    /// Update poll result (last_known_code, error, timestamps).
    async fn update_poll_result(
        &self,
        id: SteamTrackingId,
        cmd: UpdatePollResultCommand,
    ) -> Result<SteamTracking, DomainError>;
}

/// Data for creating a new steam tracking entry.
#[derive(Debug, Clone)]
pub struct CreateSteamTracking {
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    /// Initial share code to use as the polling cursor.
    pub initial_share_code: Option<String>,
}
