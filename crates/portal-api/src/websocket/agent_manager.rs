//! Server-agent connection manager.
//!
//! One outbound-WSS connection per registered game server (the agent dials
//! in; the portal never connects out). Commands are pushed down the socket
//! as JSON frames and awaited via per-command oneshot channels. Mirrors the
//! `VetoLobbyManager` shape: process-local `DashMap`, no cross-instance
//! fan-out. Design: docs/matchzy-integration.md §5.2.

use dashmap::DashMap;
use portal_core::errors::DomainError;
use portal_core::ids::GameServerId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// Default per-command timeout.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Outbound frame buffer per agent connection.
const OUTBOUND_BUFFER: usize = 32;

/// A command the portal sends to an agent (§5.2 protocol).
#[derive(Debug, Clone)]
pub enum AgentCommand {
    /// Run `matchzy_loadmatch_url` with an auth header.
    LoadMatch {
        url: String,
        header_name: String,
        header_value: String,
    },
    /// Run `css_endmatch` (reset the server).
    EndMatch,
    /// Run an arbitrary console command (admin passthrough — audited by
    /// the caller).
    Exec { command: String },
    /// Request an immediate `get5_status`.
    Status,
    /// Restore a round backup (`matchzy_loadbackup_url`).
    LoadBackup {
        url: String,
        header_name: String,
        header_value: String,
    },
    /// Mid-series roster edit: remove-then-add so the listed player count
    /// never exceeds `players_per_team` (§6.8).
    RosterEdit {
        /// SteamID64s to remove (kicked by MatchZy).
        remove: Vec<String>,
        /// `(steamid64, team1|team2, display name)` to add.
        add: Vec<(String, String, String)>,
    },
}

impl AgentCommand {
    fn cmd_name(&self) -> &'static str {
        match self {
            Self::LoadMatch { .. } => "load_match",
            Self::EndMatch => "end_match",
            Self::Exec { .. } => "exec",
            Self::Status => "status",
            Self::LoadBackup { .. } => "load_backup",
            Self::RosterEdit { .. } => "roster_edit",
        }
    }

    fn args(&self) -> Option<serde_json::Value> {
        match self {
            Self::LoadMatch {
                url,
                header_name,
                header_value,
            } => Some(serde_json::json!({
                "url": url,
                "header_name": header_name,
                "header_value": header_value,
            })),
            Self::Exec { command } => Some(serde_json::json!({ "command": command })),
            Self::LoadBackup {
                url,
                header_name,
                header_value,
            } => Some(serde_json::json!({
                "url": url,
                "header_name": header_name,
                "header_value": header_value,
            })),
            Self::RosterEdit { remove, add } => Some(serde_json::json!({
                "remove": remove,
                "add": add
                    .iter()
                    .map(|(steamid64, team, name)| serde_json::json!({
                        "steamid64": steamid64,
                        "team": team,
                        "name": name,
                    }))
                    .collect::<Vec<_>>(),
            })),
            Self::EndMatch | Self::Status => None,
        }
    }
}

/// Wire frame: portal → agent.
#[derive(Debug, Serialize)]
pub struct PortalCommandFrame {
    pub id: String,
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Wire frame: agent → portal command result.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentResultFrame {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Wire frame: agent → portal heartbeat.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentHeartbeatFrame {
    /// Discriminator; always `"heartbeat"`.
    #[serde(rename = "type")]
    pub frame_type: String,
    pub agent_version: String,
    pub rcon_ok: bool,
    /// Raw `get5_status` JSON as forwarded by the agent (absent when RCON
    /// was unreachable).
    #[serde(default)]
    pub get5_status: Option<serde_json::Value>,
}

/// Inbound frames, distinguished structurally: heartbeats carry `type`,
/// command results carry `id` + `ok`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AgentMessage {
    Heartbeat(AgentHeartbeatFrame),
    CommandResult(AgentResultFrame),
}

/// Result of a completed agent command.
#[derive(Debug, Clone)]
pub struct AgentCommandOutcome {
    pub ok: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

type PendingMap = Arc<DashMap<String, oneshot::Sender<AgentCommandOutcome>>>;

struct AgentHandle {
    connection_id: Uuid,
    outbound: mpsc::Sender<String>,
    pending: PendingMap,
}

/// A registered agent socket's server-side half, held by the WS task.
pub struct AgentSession {
    /// Identifies this connection so a reconnect doesn't get torn down by
    /// the old socket's cleanup.
    pub connection_id: Uuid,
    /// Frames to write to the socket.
    pub outbound_rx: mpsc::Receiver<String>,
    pending: PendingMap,
}

impl AgentSession {
    /// Route an inbound command-result frame to its waiter.
    pub fn resolve(&self, frame: AgentResultFrame) {
        if let Some((_, tx)) = self.pending.remove(&frame.id) {
            let _ = tx.send(AgentCommandOutcome {
                ok: frame.ok,
                output: frame.output,
                error: frame.error,
            });
        }
    }
}

/// Manages all connected agents, keyed by game server id.
#[derive(Default)]
pub struct AgentConnectionManager {
    agents: DashMap<GameServerId, AgentHandle>,
}

impl AgentConnectionManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new agent connection, replacing any previous one for the
    /// same server (the replaced socket's outbound channel closes, which
    /// ends its write loop).
    pub fn register(&self, server_id: GameServerId) -> AgentSession {
        let (tx, rx) = mpsc::channel(OUTBOUND_BUFFER);
        let pending: PendingMap = Arc::new(DashMap::new());
        let connection_id = Uuid::now_v7();
        self.agents.insert(
            server_id,
            AgentHandle {
                connection_id,
                outbound: tx,
                pending: Arc::clone(&pending),
            },
        );
        AgentSession {
            connection_id,
            outbound_rx: rx,
            pending,
        }
    }

    /// Remove a connection — only if it is still the current one.
    pub fn remove(&self, server_id: GameServerId, connection_id: Uuid) {
        self.agents.remove_if(&server_id, |_, handle| {
            handle.connection_id == connection_id
        });
    }

    /// Whether an agent is currently connected for this server.
    #[must_use]
    pub fn is_connected(&self, server_id: GameServerId) -> bool {
        self.agents.contains_key(&server_id)
    }

    /// Number of currently connected agents (metrics gauge source; bounded
    /// by the admin-curated `game_servers` registry).
    #[must_use]
    pub fn connected_count(&self) -> usize {
        self.agents.len()
    }

    /// Forcibly drop a server's connection regardless of which socket holds
    /// it (revocation path). Closing the outbound channel ends the WS task.
    pub fn disconnect(&self, server_id: GameServerId) {
        self.agents.remove(&server_id);
    }

    /// Send a command and await its result (with [`COMMAND_TIMEOUT`]).
    ///
    /// Fails fast with `Conflict` when no agent is connected.
    pub async fn send_command(
        &self,
        server_id: GameServerId,
        command: AgentCommand,
    ) -> Result<AgentCommandOutcome, DomainError> {
        let id = Uuid::now_v7().to_string();
        let frame = PortalCommandFrame {
            id: id.clone(),
            cmd: command.cmd_name().to_string(),
            args: command.args(),
        };
        let payload = serde_json::to_string(&frame)
            .map_err(|e| DomainError::Internal(format!("serialize agent command: {e}")))?;

        let (result_tx, result_rx) = oneshot::channel();
        let (outbound, pending) = {
            let handle = self.agents.get(&server_id).ok_or_else(|| {
                DomainError::Conflict("no agent connected for this server".into())
            })?;
            handle.pending.insert(id.clone(), result_tx);
            (handle.outbound.clone(), Arc::clone(&handle.pending))
            // guard dropped here — never held across an await
        };

        if outbound.send(payload).await.is_err() {
            pending.remove(&id);
            return Err(DomainError::Conflict(
                "agent disconnected before the command was sent".into(),
            ));
        }

        match tokio::time::timeout(COMMAND_TIMEOUT, result_rx).await {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(_)) => Err(DomainError::Conflict(
                "agent disconnected while executing the command".into(),
            )),
            Err(_) => {
                pending.remove(&id);
                Err(DomainError::Conflict(format!(
                    "agent did not answer within {}s",
                    COMMAND_TIMEOUT.as_secs()
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn command_to_disconnected_server_fails_fast() {
        let manager = AgentConnectionManager::new();
        let err = manager
            .send_command(GameServerId::new(), AgentCommand::Status)
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[tokio::test]
    async fn command_round_trip_resolves() {
        let manager = AgentConnectionManager::new();
        let server_id = GameServerId::new();
        let mut session = manager.register(server_id);

        let send = manager.send_command(server_id, AgentCommand::Status);
        let echo = async {
            let frame_json = session.outbound_rx.recv().await.expect("frame sent");
            let frame: serde_json::Value = serde_json::from_str(&frame_json).unwrap();
            assert_eq!(frame["cmd"], "status");
            session.resolve(AgentResultFrame {
                id: frame["id"].as_str().unwrap().to_string(),
                ok: true,
                output: Some("{}".into()),
                error: None,
            });
        };
        let (outcome, ()) = tokio::join!(send, echo);
        let outcome = outcome.unwrap();
        assert!(outcome.ok);
    }

    #[tokio::test]
    async fn reconnect_replaces_previous_session() {
        let manager = AgentConnectionManager::new();
        let server_id = GameServerId::new();
        let old = manager.register(server_id);
        let new = manager.register(server_id);
        // Old connection's cleanup must not evict the new session.
        manager.remove(server_id, old.connection_id);
        assert!(manager.is_connected(server_id));
        manager.remove(server_id, new.connection_id);
        assert!(!manager.is_connected(server_id));
    }
}
