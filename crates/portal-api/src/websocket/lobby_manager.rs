//! Lobby manager for veto WebSocket connections.

use std::sync::Arc;

use dashmap::DashMap;
use portal_core::TournamentMatchId;

use super::lobby::VetoLobby;

/// Manager for all active veto lobbies.
///
/// Provides thread-safe access to lobbies and handles lobby lifecycle.
#[derive(Debug)]
pub struct VetoLobbyManager {
    /// Active lobbies, keyed by match ID.
    lobbies: DashMap<TournamentMatchId, Arc<VetoLobby>>,
}

impl VetoLobbyManager {
    /// Create a new lobby manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lobbies: DashMap::new(),
        }
    }

    /// Get or create a lobby for a match.
    ///
    /// If a lobby doesn't exist, one is created.
    pub fn get_or_create_lobby(&self, match_id: TournamentMatchId) -> Arc<VetoLobby> {
        self.lobbies
            .entry(match_id)
            .or_insert_with(|| Arc::new(VetoLobby::new(match_id)))
            .clone()
    }

    /// Get an existing lobby if it exists.
    #[must_use]
    pub fn get_lobby(&self, match_id: &TournamentMatchId) -> Option<Arc<VetoLobby>> {
        self.lobbies.get(match_id).map(|entry| entry.clone())
    }

    /// Remove a lobby.
    ///
    /// Returns the removed lobby if it existed.
    pub fn remove_lobby(&self, match_id: &TournamentMatchId) -> Option<Arc<VetoLobby>> {
        self.lobbies.remove(match_id).map(|(_, lobby)| lobby)
    }

    /// Remove empty lobbies.
    ///
    /// This should be called periodically to clean up unused lobbies.
    pub fn cleanup_empty_lobbies(&self) {
        self.lobbies.retain(|_, lobby| !lobby.is_empty());
    }

    /// Get the number of active lobbies.
    #[must_use]
    pub fn lobby_count(&self) -> usize {
        self.lobbies.len()
    }

    /// Get all match IDs with active lobbies.
    #[must_use]
    pub fn active_match_ids(&self) -> Vec<TournamentMatchId> {
        self.lobbies.iter().map(|entry| *entry.key()).collect()
    }
}

impl Default for VetoLobbyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for VetoLobbyManager {
    fn clone(&self) -> Self {
        // Create a new manager that shares the same lobbies
        // This works because DashMap is internally reference-counted
        Self {
            lobbies: self.lobbies.clone(),
        }
    }
}
