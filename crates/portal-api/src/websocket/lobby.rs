//! Veto lobby for managing WebSocket connections.

use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::DashMap;
use portal_core::TournamentMatchId;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::connection::VetoConnection;
use super::messages::LobbyBroadcast;

/// Unique identifier for a WebSocket connection.
pub type ConnectionId = Uuid;

/// Broadcast channel capacity.
const BROADCAST_CAPACITY: usize = 256;

/// A veto lobby for a single match.
///
/// Manages WebSocket connections and broadcasts events to all connected clients.
pub struct VetoLobby {
    /// Match ID this lobby is for.
    pub match_id: TournamentMatchId,
    /// Broadcast channel sender.
    broadcast_tx: broadcast::Sender<LobbyBroadcast>,
    /// Connected clients.
    connections: DashMap<ConnectionId, VetoConnection>,
    /// Number of spectators.
    spectator_count: AtomicUsize,
}

impl VetoLobby {
    /// Create a new veto lobby.
    #[must_use]
    pub fn new(match_id: TournamentMatchId) -> Self {
        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            match_id,
            broadcast_tx,
            connections: DashMap::new(),
            spectator_count: AtomicUsize::new(0),
        }
    }

    /// Subscribe to lobby broadcasts.
    ///
    /// Returns a receiver that will receive all broadcast messages.
    pub fn subscribe(&self) -> broadcast::Receiver<LobbyBroadcast> {
        self.broadcast_tx.subscribe()
    }

    /// Broadcast a message to all connected clients.
    ///
    /// Clients are responsible for filtering messages based on their role/permissions.
    pub fn broadcast(&self, message: LobbyBroadcast) {
        // Ignore send errors (no receivers is fine)
        let _ = self.broadcast_tx.send(message);
    }

    /// Add a new connection to the lobby.
    pub fn add_connection(&self, id: ConnectionId, conn: VetoConnection) {
        if conn.is_spectator() {
            self.spectator_count.fetch_add(1, Ordering::SeqCst);
        }
        self.connections.insert(id, conn);
    }

    /// Remove a connection from the lobby.
    ///
    /// Returns the removed connection if it existed.
    pub fn remove_connection(&self, id: &ConnectionId) -> Option<VetoConnection> {
        if let Some((_, conn)) = self.connections.remove(id) {
            if conn.is_spectator() {
                self.spectator_count.fetch_sub(1, Ordering::SeqCst);
            }
            Some(conn)
        } else {
            None
        }
    }

    /// Get the current spectator count.
    #[must_use]
    pub fn spectator_count(&self) -> usize {
        self.spectator_count.load(Ordering::SeqCst)
    }

    /// Check if the lobby is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Get the number of connections.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Get registration IDs of connected participants.
    #[must_use]
    pub fn connected_participant_ids(&self) -> Vec<String> {
        self.connections
            .iter()
            .filter_map(|entry| {
                let conn = entry.value();
                if conn.is_participant() {
                    conn.registration_id.map(|id| id.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if a specific registration is connected.
    #[must_use]
    pub fn is_participant_connected(
        &self,
        registration_id: portal_core::TournamentRegistrationId,
    ) -> bool {
        self.connections.iter().any(|entry| {
            entry.value().registration_id == Some(registration_id)
        })
    }

    /// Get a connection by ID.
    pub fn get_connection(&self, id: &ConnectionId) -> Option<VetoConnection> {
        self.connections.get(id).map(|entry| entry.value().clone())
    }
}

impl std::fmt::Debug for VetoLobby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VetoLobby")
            .field("match_id", &self.match_id)
            .field("connections", &self.connections.len())
            .field("spectator_count", &self.spectator_count())
            .finish()
    }
}
