//! PUG lobby broadcast manager.
//!
//! Gathering-phase lobbies have no match yet, so the veto lobby (keyed by
//! match id) can't carry them. This manager fans out tiny "doorbell" frames
//! keyed by pug id: every mutation broadcasts `pug_changed` and clients
//! refetch `GET /v1/pugs/{id}` — the response is viewer-specific (join code
//! visibility, my_registration_id), so pushing full state here would mean a
//! second, per-viewer serialization of the lobby. Same pattern as the veto
//! lobby's `LineupUpdate`.
//!
//! Process-local (`DashMap` + `tokio::broadcast`), matching
//! `VetoLobbyManager` and `AgentConnectionManager` — the single-instance
//! deployment assumption is documented on both.

use dashmap::DashMap;
use portal_core::PugId;
use serde::Serialize;
use tokio::sync::broadcast;

/// Broadcast channel capacity per lobby. Frames are tiny and lagging
/// receivers just refetch, so a small buffer is fine.
const CHANNEL_CAPACITY: usize = 64;

/// Server → client frames on the pug lobby socket.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PugLobbyBroadcast {
    /// Something about the lobby changed — refetch `GET /v1/pugs/{id}`.
    PugChanged {
        /// What changed, for debugging and optimistic UI (e.g. "player_joined").
        reason: String,
    },
    /// The creator started a rematch — the roster was copied to a new lobby.
    RematchCreated {
        /// The new pug to navigate to.
        pug_id: String,
    },
}

/// Manages broadcast channels for active PUG lobbies.
pub struct PugLobbyManager {
    lobbies: DashMap<PugId, broadcast::Sender<PugLobbyBroadcast>>,
}

impl PugLobbyManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            lobbies: DashMap::new(),
        }
    }

    /// Subscribe to a lobby's broadcasts, creating the channel if needed.
    pub fn subscribe(&self, pug_id: PugId) -> broadcast::Receiver<PugLobbyBroadcast> {
        self.lobbies
            .entry(pug_id)
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// Broadcast a frame to a lobby's subscribers. Channels nobody listens
    /// to are dropped lazily here rather than via a cleanup task.
    pub fn broadcast(&self, pug_id: PugId, message: PugLobbyBroadcast) {
        // NB: the guard's DashMap ref must drop before remove_if re-locks the
        // shard, hence the boolean instead of removing inside the borrow.
        let remove = if let Some(sender) = self.lobbies.get(&pug_id) {
            sender.receiver_count() == 0 || sender.send(message).is_err()
        } else {
            false
        };
        if remove {
            self.lobbies
                .remove_if(&pug_id, |_, sender| sender.receiver_count() == 0);
        }
    }

    /// Doorbell: the lobby changed, clients should refetch.
    pub fn notify_changed(&self, pug_id: PugId, reason: &str) {
        self.broadcast(
            pug_id,
            PugLobbyBroadcast::PugChanged {
                reason: reason.to_string(),
            },
        );
    }
}

impl Default for PugLobbyManager {
    fn default() -> Self {
        Self::new()
    }
}
