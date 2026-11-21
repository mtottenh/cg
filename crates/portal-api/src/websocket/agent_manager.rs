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
    /// Start the update helper's `check` (agent 0.3.0+). The result
    /// arrives as an `update_state` frame.
    UpdateCheck,
    /// Start an update operation under the portal's job id. The agent
    /// answers at once; progress and the result arrive as pushed frames.
    UpdateStart { operation: String, job_id: String },
    /// Ask for the helper's current state file.
    UpdateStatus,
    /// Acknowledge a stored `update_result`, so the agent stops replaying it.
    UpdateAck { job_id: String },
    /// Replace the portal-managed MatchZy overrides on the host.
    SetMatchzyOverrides {
        overrides: serde_json::Map<String, serde_json::Value>,
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
            Self::UpdateCheck => "update_check",
            Self::UpdateStart { .. } => "update_start",
            Self::UpdateStatus => "update_status",
            Self::UpdateAck { .. } => "update_ack",
            Self::SetMatchzyOverrides { .. } => "set_matchzy_overrides",
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
            Self::UpdateStart { operation, job_id } => Some(serde_json::json!({
                "operation": operation,
                "job_id": job_id,
            })),
            Self::UpdateAck { job_id } => Some(serde_json::json!({ "job_id": job_id })),
            Self::SetMatchzyOverrides { overrides } => {
                Some(serde_json::json!({ "overrides": overrides }))
            }
            Self::EndMatch | Self::Status | Self::UpdateCheck | Self::UpdateStatus => None,
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
    /// Raw CS2 `status` output (agent 0.2.0+), at most 8 KiB as sent.
    #[serde(default)]
    pub status_output: Option<String>,
    /// What the agent can do (agent 0.3.0+), e.g. `lgsm_update`. Absent
    /// from older agents, which reads as none.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Whether the demo-upload block is intact in the MatchZy config file
    /// (0.3.0+); `null`/absent = unknown.
    #[serde(default)]
    pub demo_upload_wired: Option<bool>,
    /// Whether the portal-managed overrides are in that file (0.3.0+).
    #[serde(default)]
    pub overrides_applied: Option<bool>,
    /// A repair changed the config and the server has not re-read it yet.
    #[serde(default)]
    pub reload_pending: Option<bool>,
    /// The update operation in flight: `{job_id, operation, phase, status}`.
    #[serde(default)]
    pub update_job: Option<serde_json::Value>,
}

/// Inbound frames. Everything the agent pushes carries `type`; a command
/// result carries `id` + `ok` and no `type`.
#[derive(Debug, Clone)]
pub enum AgentMessage {
    Heartbeat(Box<AgentHeartbeatFrame>),
    CommandResult(AgentResultFrame),
    /// `update_progress` (agent 0.3.0+): a running job moved on.
    UpdateProgress(serde_json::Value),
    /// `update_result`: a job reached a terminal status.
    UpdateResult(serde_json::Value),
    /// `update_state`: the last check and the recorded installed versions.
    UpdateState(serde_json::Value),
    /// A typed frame this portal does not know; ignored.
    Unknown(String),
}

impl AgentMessage {
    /// Parse one text frame.
    ///
    /// # Errors
    ///
    /// The frame is not JSON, or has the shape of neither a pushed frame
    /// nor a command result.
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        let value: serde_json::Value = serde_json::from_str(text)?;
        let frame_type = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        match frame_type.as_deref() {
            Some("heartbeat") => Ok(Self::Heartbeat(Box::new(serde_json::from_value(value)?))),
            Some("update_progress") => Ok(Self::UpdateProgress(value)),
            Some("update_result") => Ok(Self::UpdateResult(value)),
            Some("update_state") => Ok(Self::UpdateState(value)),
            Some(other) => Ok(Self::Unknown(other.to_string())),
            None => Ok(Self::CommandResult(serde_json::from_value(value)?)),
        }
    }
}

/// Why a command got no answer from the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// No agent is connected for the server.
    NotConnected,
    /// The connection went away before the frame was written: the agent
    /// never saw the command.
    NotSent,
    /// The connection went away after the frame was written: the agent may
    /// or may not have acted on it.
    Disconnected,
    /// No answer within [`COMMAND_TIMEOUT`]: likewise unknown.
    TimedOut,
    /// The command could not be encoded; nothing was sent.
    Unserializable,
}

impl SendError {
    /// Whether the agent certainly never received the command.
    #[must_use]
    pub const fn not_delivered(self) -> bool {
        matches!(
            self,
            Self::NotConnected | Self::NotSent | Self::Unserializable
        )
    }
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => f.write_str("no agent connected for this server"),
            Self::NotSent => f.write_str("agent disconnected before the command was sent"),
            Self::Disconnected => f.write_str("agent disconnected while executing the command"),
            Self::Unserializable => f.write_str("the command could not be encoded"),
            Self::TimedOut => write!(
                f,
                "agent did not answer within {}s",
                COMMAND_TIMEOUT.as_secs()
            ),
        }
    }
}

impl From<SendError> for DomainError {
    fn from(e: SendError) -> Self {
        match e {
            SendError::Unserializable => Self::Internal(e.to_string()),
            _ => Self::Conflict(e.to_string()),
        }
    }
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
        self.try_send_command(server_id, command)
            .await
            .map_err(DomainError::from)
    }

    /// [`Self::send_command`], telling apart the ways an answer can fail to
    /// arrive — a caller that started something needs to know whether the
    /// agent can have seen the command.
    pub async fn try_send_command(
        &self,
        server_id: GameServerId,
        command: AgentCommand,
    ) -> Result<AgentCommandOutcome, SendError> {
        let id = Uuid::now_v7().to_string();
        let frame = PortalCommandFrame {
            id: id.clone(),
            cmd: command.cmd_name().to_string(),
            args: command.args(),
        };
        let payload = serde_json::to_string(&frame).map_err(|e| {
            tracing::error!(error = %e, "agent command did not serialize");
            SendError::Unserializable
        })?;

        let (result_tx, result_rx) = oneshot::channel();
        let (outbound, pending) = {
            let handle = self.agents.get(&server_id).ok_or(SendError::NotConnected)?;
            handle.pending.insert(id.clone(), result_tx);
            (handle.outbound.clone(), Arc::clone(&handle.pending))
            // guard dropped here — never held across an await
        };

        if outbound.send(payload).await.is_err() {
            pending.remove(&id);
            return Err(SendError::NotSent);
        }

        match tokio::time::timeout(COMMAND_TIMEOUT, result_rx).await {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(_)) => Err(SendError::Disconnected),
            Err(_) => {
                pending.remove(&id);
                Err(SendError::TimedOut)
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

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn an_old_agents_heartbeat_parses_with_no_update_fields() {
        let msg = AgentMessage::parse(
            r#"{"type":"heartbeat","agent_version":"0.2.1","rcon_ok":true,"get5_status":{"gamestate":"none"}}"#,
        )
        .unwrap();
        let AgentMessage::Heartbeat(hb) = msg else {
            panic!("not a heartbeat");
        };
        assert!(hb.capabilities.is_empty());
        assert_eq!(hb.demo_upload_wired, None);
        assert_eq!(hb.overrides_applied, None);
        assert_eq!(hb.reload_pending, None);
        assert!(hb.update_job.is_none());
    }

    #[test]
    fn a_030_heartbeat_carries_the_update_fields() {
        let msg = AgentMessage::parse(
            r#"{"type":"heartbeat","agent_version":"0.3.0","rcon_ok":false,
                "capabilities":["lgsm_update"],"demo_upload_wired":true,
                "overrides_applied":null,"reload_pending":false,
                "update_job":{"job_id":"j","operation":"mods","phase":"verify","status":"running"}}"#,
        )
        .unwrap();
        let AgentMessage::Heartbeat(hb) = msg else {
            panic!("not a heartbeat");
        };
        assert_eq!(hb.capabilities, vec!["lgsm_update".to_string()]);
        assert_eq!(hb.demo_upload_wired, Some(true));
        assert_eq!(hb.overrides_applied, None);
        assert_eq!(hb.reload_pending, Some(false));
        assert_eq!(hb.update_job.unwrap()["phase"], "verify");
    }

    #[test]
    fn pushed_frames_are_told_apart_by_type_and_results_by_shape() {
        assert!(matches!(
            AgentMessage::parse(r#"{"type":"update_progress","job_id":"j"}"#).unwrap(),
            AgentMessage::UpdateProgress(_)
        ));
        assert!(matches!(
            AgentMessage::parse(r#"{"type":"update_result","job_id":"j","ok":true,"id":"x"}"#)
                .unwrap(),
            AgentMessage::UpdateResult(_)
        ));
        assert!(matches!(
            AgentMessage::parse(r#"{"type":"update_state","check":null}"#).unwrap(),
            AgentMessage::UpdateState(_)
        ));
        assert!(matches!(
            AgentMessage::parse(r#"{"type":"from_the_future"}"#).unwrap(),
            AgentMessage::Unknown(t) if t == "from_the_future"
        ));
        assert!(matches!(
            AgentMessage::parse(r#"{"id":"c1","ok":true,"output":"x"}"#).unwrap(),
            AgentMessage::CommandResult(_)
        ));
        assert!(AgentMessage::parse(r#"{"neither":1}"#).is_err());
        assert!(AgentMessage::parse("not json").is_err());
    }

    #[test]
    fn update_commands_serialize_as_the_agent_expects() {
        let start = AgentCommand::UpdateStart {
            operation: "repair-config".into(),
            job_id: "0193a5b0-7c1e-7000-8000-000000000001".into(),
        };
        assert_eq!(start.cmd_name(), "update_start");
        assert_eq!(
            start.args().unwrap(),
            serde_json::json!({"operation": "repair-config",
                "job_id": "0193a5b0-7c1e-7000-8000-000000000001"})
        );
        assert_eq!(AgentCommand::UpdateCheck.cmd_name(), "update_check");
        assert!(AgentCommand::UpdateCheck.args().is_none());
        assert!(AgentCommand::UpdateStatus.args().is_none());
        let ack = AgentCommand::UpdateAck { job_id: "j".into() };
        assert_eq!(ack.args().unwrap(), serde_json::json!({"job_id": "j"}));
        let mut overrides = serde_json::Map::new();
        overrides.insert("matchzy_x".into(), serde_json::json!(1));
        let set = AgentCommand::SetMatchzyOverrides { overrides };
        assert_eq!(set.cmd_name(), "set_matchzy_overrides");
        assert_eq!(
            set.args().unwrap(),
            serde_json::json!({"overrides": {"matchzy_x": 1}})
        );
    }
}
