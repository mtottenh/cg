//! Veto lobby for managing WebSocket connections.
//!
//! Presence is tracked per **registration**, not per socket: a team's
//! captain with two tabs open, or a client whose reconnect overlaps the old
//! socket's teardown, is one present team. `add_connection` and
//! `remove_connection` say whether the announced presence actually changed,
//! so the handler only broadcasts a team's arrival on its first socket and
//! its departure on its last.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use dashmap::DashMap;
use portal_core::{TournamentMatchId, TournamentRegistrationId};
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
    /// Live participant sockets per registration — the presence refcount.
    presence: Mutex<HashMap<TournamentRegistrationId, usize>>,
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
            presence: Mutex::new(HashMap::new()),
            spectator_count: AtomicUsize::new(0),
        }
    }

    /// The presence refcount. A poisoned lock still holds a usable map — a
    /// panic elsewhere must not take the whole lobby's presence with it.
    fn presence(&self) -> MutexGuard<'_, HashMap<TournamentRegistrationId, usize>> {
        self.presence.lock().unwrap_or_else(PoisonError::into_inner)
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

    /// Add a connection.
    ///
    /// Returns whether the lobby's announced presence changed: `true` for a
    /// participant's first live socket and for every spectator (the count
    /// moved); `false` for a further socket of a team already present.
    pub fn add_connection(&self, id: ConnectionId, conn: VetoConnection) -> bool {
        let announce = if conn.is_spectator() {
            self.spectator_count.fetch_add(1, Ordering::SeqCst);
            true
        } else if let Some(reg_id) = conn.registration_id.filter(|_| conn.is_participant()) {
            let mut presence = self.presence();
            let count = presence.entry(reg_id).or_insert(0);
            *count += 1;
            let first = *count == 1;
            drop(presence);
            first
        } else {
            false
        };
        self.connections.insert(id, conn);
        announce
    }

    /// Remove a connection.
    ///
    /// Returns the removed connection and whether the announced presence
    /// changed: `true` when a participant's last live socket left or a
    /// spectator left; `false` when the team is still present on another
    /// socket.
    pub fn remove_connection(&self, id: &ConnectionId) -> Option<(VetoConnection, bool)> {
        let (_, conn) = self.connections.remove(id)?;
        let announce = if conn.is_spectator() {
            self.spectator_count.fetch_sub(1, Ordering::SeqCst);
            true
        } else if let Some(reg_id) = conn.registration_id.filter(|_| conn.is_participant()) {
            let mut presence = self.presence();
            let last = match presence.get_mut(&reg_id) {
                Some(count) if *count > 1 => {
                    *count -= 1;
                    false
                }
                Some(_) => {
                    presence.remove(&reg_id);
                    true
                }
                None => true,
            };
            drop(presence);
            last
        } else {
            false
        };
        Some((conn, announce))
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

    /// Registrations with at least one live participant socket.
    #[must_use]
    pub fn connected_participant_ids(&self) -> Vec<String> {
        self.presence().keys().map(ToString::to_string).collect()
    }

    /// Whether a registration has at least one live participant socket.
    #[must_use]
    pub fn is_participant_connected(&self, registration_id: TournamentRegistrationId) -> bool {
        self.presence().contains_key(&registration_id)
    }

    /// The username of one connected participant socket for a registration,
    /// for the presence snapshot a joiner receives.
    #[must_use]
    pub fn connected_username(&self, registration_id: TournamentRegistrationId) -> Option<String> {
        self.connections
            .iter()
            .find(|entry| {
                let c = entry.value();
                c.is_participant() && c.registration_id == Some(registration_id)
            })
            .map(|entry| entry.value().username.clone())
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
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portal_core::{PlayerId, UserId};

    fn participant(reg: TournamentRegistrationId, name: &str) -> VetoConnection {
        VetoConnection::participant(
            UserId::new(),
            PlayerId::new(),
            name.to_string(),
            reg,
            "Team".to_string(),
        )
    }

    #[test]
    fn a_team_is_announced_on_its_first_socket_and_its_last() {
        let lobby = VetoLobby::new(TournamentMatchId::new());
        let reg = TournamentRegistrationId::new();
        let (tab1, tab2) = (ConnectionId::new_v4(), ConnectionId::new_v4());

        assert!(lobby.add_connection(tab1, participant(reg, "cap")));
        assert!(
            !lobby.add_connection(tab2, participant(reg, "cap")),
            "a second tab is not a second arrival"
        );
        assert!(lobby.is_participant_connected(reg));
        assert_eq!(lobby.connected_participant_ids(), vec![reg.to_string()]);

        let (_, announce) = lobby.remove_connection(&tab1).unwrap();
        assert!(!announce, "closing one tab does not announce a departure");
        assert!(lobby.is_participant_connected(reg));

        let (_, announce) = lobby.remove_connection(&tab2).unwrap();
        assert!(announce, "the last socket leaving is the departure");
        assert!(!lobby.is_participant_connected(reg));
        assert!(lobby.is_empty());
    }

    #[test]
    fn spectators_always_move_the_count() {
        let lobby = VetoLobby::new(TournamentMatchId::new());
        let id = ConnectionId::new_v4();
        let spec = VetoConnection::spectator(UserId::new(), PlayerId::new(), "s".into());
        assert!(lobby.add_connection(id, spec));
        assert_eq!(lobby.spectator_count(), 1);
        let (_, announce) = lobby.remove_connection(&id).unwrap();
        assert!(announce);
        assert_eq!(lobby.spectator_count(), 0);
    }

    #[test]
    fn removing_an_unknown_connection_is_a_no_op() {
        let lobby = VetoLobby::new(TournamentMatchId::new());
        assert!(lobby.remove_connection(&ConnectionId::new_v4()).is_none());
    }
}
