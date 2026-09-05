//! Lobby manager for veto WebSocket connections.
//!
//! Joining and the last-one-out teardown are serialised per match under
//! the map's shard lock, so a joiner can never be handed a lobby that is
//! about to be dropped — the split-lobby race where two people in the same
//! match end up in different lobbies.
//!
//! Process-local: one manager per API process, always behind an `Arc`.
//! It is deliberately not `Clone` — a `DashMap` clone is an independent
//! copy, not a shared handle, and a by-value clone would silently diverge.

use std::sync::Arc;

use dashmap::DashMap;
use portal_core::TournamentMatchId;
use tokio::sync::broadcast;

use super::connection::VetoConnection;
use super::lobby::{ConnectionId, VetoLobby};
use super::messages::LobbyBroadcast;

/// Manager for all active veto lobbies.
///
/// Provides thread-safe access to lobbies and handles lobby lifecycle.
#[derive(Debug)]
pub struct VetoLobbyManager {
    /// Active lobbies, keyed by match ID.
    lobbies: DashMap<TournamentMatchId, Arc<VetoLobby>>,
}

/// What a joiner gets back: the lobby, a receiver that was subscribed before
/// the connection was added (so no broadcast in between is missed), and
/// whether the join changed the announced presence.
pub struct Joined {
    pub lobby: Arc<VetoLobby>,
    pub broadcast_rx: broadcast::Receiver<LobbyBroadcast>,
    pub presence_changed: bool,
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

    /// Add a connection to a match's lobby, creating the lobby if needed.
    ///
    /// Subscribing and adding happen under the entry lock, so a concurrent
    /// `remove_if_empty` for the same match either runs before (and the
    /// joiner creates a fresh lobby) or after (and finds it non-empty).
    pub fn join(
        &self,
        match_id: TournamentMatchId,
        connection_id: ConnectionId,
        connection: VetoConnection,
    ) -> Joined {
        let entry = self
            .lobbies
            .entry(match_id)
            .or_insert_with(|| Arc::new(VetoLobby::new(match_id)));
        let lobby = Arc::clone(entry.value());
        let broadcast_rx = lobby.subscribe();
        let presence_changed = lobby.add_connection(connection_id, connection);
        drop(entry);
        Joined {
            lobby,
            broadcast_rx,
            presence_changed,
        }
    }

    /// Get an existing lobby if it exists.
    #[must_use]
    pub fn get_lobby(&self, match_id: &TournamentMatchId) -> Option<Arc<VetoLobby>> {
        self.lobbies.get(match_id).map(|entry| entry.clone())
    }

    /// Drop a match's lobby if — checked under the lock — nobody is in it.
    /// Returns whether it was removed.
    pub fn remove_if_empty(&self, match_id: &TournamentMatchId) -> bool {
        self.lobbies
            .remove_if(match_id, |_, lobby| lobby.is_empty())
            .is_some()
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

    /// Total open connections across all lobbies (metrics gauge source).
    #[must_use]
    pub fn total_connections(&self) -> usize {
        self.lobbies
            .iter()
            .map(|entry| entry.value().connection_count())
            .sum()
    }
}

impl Default for VetoLobbyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portal_core::{PlayerId, TournamentRegistrationId, UserId};

    fn participant() -> VetoConnection {
        VetoConnection::participant(
            UserId::new(),
            PlayerId::new(),
            "cap".into(),
            TournamentRegistrationId::new(),
            "Team".into(),
        )
    }

    #[test]
    fn teardown_only_removes_an_empty_lobby() {
        let manager = VetoLobbyManager::new();
        let match_id = TournamentMatchId::new();
        let id = ConnectionId::new_v4();
        let joined = manager.join(match_id, id, participant());
        assert!(joined.presence_changed);

        assert!(
            !manager.remove_if_empty(&match_id),
            "a lobby with someone in it stays"
        );
        assert_eq!(manager.lobby_count(), 1);

        joined.lobby.remove_connection(&id);
        assert!(manager.remove_if_empty(&match_id));
        assert_eq!(manager.lobby_count(), 0);
    }

    #[test]
    fn a_join_after_teardown_gets_a_fresh_lobby_everyone_shares() {
        let manager = VetoLobbyManager::new();
        let match_id = TournamentMatchId::new();
        let first = manager.join(match_id, ConnectionId::new_v4(), participant());
        let first_lobby = Arc::clone(&first.lobby);
        drop(first);
        first_lobby.remove_connection(&ConnectionId::new_v4());
        // Simulate the last socket leaving and tearing down.
        manager.cleanup_empty_lobbies();
        assert_eq!(manager.lobby_count(), 1, "someone is still in it");

        let a = manager.join(match_id, ConnectionId::new_v4(), participant());
        let b = manager.join(match_id, ConnectionId::new_v4(), participant());
        assert!(
            Arc::ptr_eq(&a.lobby, &b.lobby),
            "two joiners of one match share one lobby"
        );
    }
}
