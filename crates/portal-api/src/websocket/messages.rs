//! WebSocket message types for veto lobby communication.

use chrono::{DateTime, Utc};
use portal_core::TournamentRegistrationId;
use portal_domain::entities::VetoLobbyMessage;
use serde::{Deserialize, Serialize};

use crate::dto::responses::veto::{VetoActionResponse, VetoSessionResponse};

// =============================================================================
// Client -> Server Messages
// =============================================================================

/// Messages sent from client to server.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Authentication message (must be first message after connection).
    Auth {
        /// JWT token for authentication.
        token: String,
    },
    /// Chat message.
    Chat {
        /// Type of chat (team or all).
        chat_type: ClientChatType,
        /// Message content.
        content: String,
    },
    /// Veto action (ban, pick, or side selection).
    VetoAction {
        /// The action to perform.
        action: ClientVetoAction,
    },
    /// Ping message for keepalive.
    Ping,
}

/// Chat type for client messages.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientChatType {
    /// Private team chat.
    Team,
    /// Public chat visible to all.
    All,
}

/// Veto action from client.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ClientVetoAction {
    /// Ban a map.
    Ban {
        /// Map ID to ban.
        map_id: String,
    },
    /// Pick a map.
    Pick {
        /// Map ID to pick.
        map_id: String,
    },
    /// Select a side for a picked map.
    SelectSide {
        /// Action number (which pick this is for).
        action_number: u32,
        /// Side to select (e.g., "ct" or "t").
        side: String,
    },
}

// =============================================================================
// Server -> Client Messages
// =============================================================================

/// Messages sent from server to client.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Authentication success.
    AuthSuccess {
        /// Role assigned to this connection.
        role: String,
        /// Registration ID if participant.
        registration_id: Option<String>,
        /// Team name if participant.
        team_name: Option<String>,
        /// Current lobby state.
        lobby_state: LobbyStatePayload,
    },
    /// Authentication error.
    AuthError {
        /// Error message.
        error: String,
    },
    /// Chat message.
    Chat {
        /// Message ID.
        id: String,
        /// Chat type (team, all, admin, system).
        chat_type: String,
        /// Author information.
        author: ChatAuthorPayload,
        /// Message content.
        content: String,
        /// When the message was sent.
        timestamp: DateTime<Utc>,
    },
    /// Chat history on join.
    ChatHistory {
        /// Historical messages.
        messages: Vec<ChatMessagePayload>,
    },
    /// Veto session state update.
    VetoStateUpdate {
        /// Updated session state.
        session: VetoSessionResponse,
    },
    /// Veto action was performed.
    VetoActionPerformed {
        /// Updated session state.
        session: VetoSessionResponse,
        /// The action that was performed.
        action: VetoActionResponse,
        /// Whether the veto is now complete.
        is_complete: bool,
    },
    /// Veto session completed.
    VetoComplete {
        /// Final selected maps in play order.
        selected_maps: Vec<String>,
        /// Final session state.
        session: VetoSessionResponse,
    },
    /// Timeout warning.
    TimeoutWarning {
        /// Seconds remaining.
        seconds_remaining: u32,
        /// Team that needs to act.
        current_team: String,
        /// Registration ID of the team.
        current_team_registration_id: String,
    },
    /// Player connected to lobby.
    PlayerConnected {
        /// Registration ID of the player.
        registration_id: String,
        /// Team name.
        team_name: String,
        /// Player username.
        username: String,
    },
    /// Player disconnected from lobby.
    PlayerDisconnected {
        /// Registration ID of the player.
        registration_id: String,
        /// Team name.
        team_name: String,
        /// Player username.
        username: String,
    },
    /// Spectator count update.
    SpectatorCount {
        /// Number of spectators.
        count: usize,
    },
    /// Error message.
    Error {
        /// Error code.
        code: String,
        /// Error message.
        message: String,
    },
    /// Coin flip result (auto-randomized when both teams connect).
    CoinFlipResult {
        /// Registration ID of the coin flip winner.
        winner_registration_id: String,
        /// Name of the winner.
        winner_name: String,
        /// Registration ID of the team with first action.
        first_action_registration_id: String,
        /// Name of the team with first action.
        first_action_name: String,
    },
    /// Pong response to ping.
    Pong,
    /// Veto action acknowledgment (sent only to the client who performed the action).
    VetoActionAck {
        /// Whether the action was successful.
        success: bool,
        /// Optional message (for errors).
        message: Option<String>,
    },
}

/// Lobby state payload for auth success.
#[derive(Debug, Clone, Serialize)]
pub struct LobbyStatePayload {
    /// Match ID.
    pub match_id: String,
    /// Current veto session if exists.
    pub session: Option<VetoSessionResponse>,
    /// Participant information.
    pub participants: ParticipantsPayload,
    /// Number of spectators.
    pub spectator_count: usize,
    /// Connected participant registration IDs.
    pub connected_participants: Vec<String>,
}

/// Participants information.
#[derive(Debug, Clone, Serialize)]
pub struct ParticipantsPayload {
    /// First participant.
    pub participant1: ParticipantPayload,
    /// Second participant.
    pub participant2: ParticipantPayload,
}

/// Single participant information.
#[derive(Debug, Clone, Serialize)]
pub struct ParticipantPayload {
    /// Registration ID.
    pub registration_id: String,
    /// Display name.
    pub name: String,
    /// Whether currently connected.
    pub is_connected: bool,
}

/// Chat author payload.
#[derive(Debug, Clone, Serialize)]
pub struct ChatAuthorPayload {
    /// User ID.
    pub user_id: String,
    /// Username.
    pub username: String,
    /// Registration ID if participant.
    pub registration_id: Option<String>,
    /// Team name if participant.
    pub team_name: Option<String>,
}

/// Chat message payload for history.
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessagePayload {
    /// Message ID.
    pub id: String,
    /// Chat type.
    pub chat_type: String,
    /// Author information.
    pub author: ChatAuthorPayload,
    /// Message content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

// =============================================================================
// Internal Broadcast Messages
// =============================================================================

/// Internal broadcast message types for lobby communication.
#[derive(Debug, Clone)]
pub enum LobbyBroadcast {
    /// Chat message broadcast.
    Chat(ChatBroadcast),
    /// Veto state update broadcast.
    VetoStateUpdate(VetoStateBroadcast),
    /// Veto action performed broadcast.
    VetoActionPerformed(VetoActionBroadcast),
    /// Veto complete broadcast.
    VetoComplete(VetoCompleteBroadcast),
    /// Timeout warning broadcast.
    TimeoutWarning(TimeoutWarningBroadcast),
    /// Coin flip result broadcast.
    CoinFlipResult(CoinFlipResultBroadcast),
    /// Participant connected broadcast.
    ParticipantConnected(ParticipantConnectionBroadcast),
    /// Participant disconnected broadcast.
    ParticipantDisconnected(ParticipantConnectionBroadcast),
    /// Spectator count update broadcast.
    SpectatorCountUpdate(usize),
}

/// Chat message broadcast.
#[derive(Debug, Clone)]
pub struct ChatBroadcast {
    /// The chat message.
    pub message: VetoLobbyMessage,
    /// Author username.
    pub author_username: String,
    /// Author team name if participant.
    pub author_team_name: Option<String>,
}

/// Veto state update broadcast.
#[derive(Debug, Clone)]
pub struct VetoStateBroadcast {
    /// Updated session response.
    pub session: VetoSessionResponse,
}

/// Veto action performed broadcast.
#[derive(Debug, Clone)]
pub struct VetoActionBroadcast {
    /// Updated session response.
    pub session: VetoSessionResponse,
    /// The action response.
    pub action: VetoActionResponse,
    /// Whether veto is complete.
    pub is_complete: bool,
}

/// Veto complete broadcast.
#[derive(Debug, Clone)]
pub struct VetoCompleteBroadcast {
    /// Final selected maps.
    pub selected_maps: Vec<String>,
    /// Final session response.
    pub session: VetoSessionResponse,
}

/// Coin flip result broadcast.
#[derive(Debug, Clone)]
pub struct CoinFlipResultBroadcast {
    /// Registration ID of the coin flip winner.
    pub winner_registration_id: TournamentRegistrationId,
    /// Name of the winner.
    pub winner_name: String,
    /// Registration ID of the team with first action.
    pub first_action_registration_id: TournamentRegistrationId,
    /// Name of the team with first action.
    pub first_action_name: String,
}

/// Timeout warning broadcast.
#[derive(Debug, Clone)]
pub struct TimeoutWarningBroadcast {
    /// Seconds remaining.
    pub seconds_remaining: u32,
    /// Team that needs to act.
    pub current_team_registration_id: TournamentRegistrationId,
    /// Team name.
    pub current_team_name: String,
}

/// Participant connection broadcast.
#[derive(Debug, Clone)]
pub struct ParticipantConnectionBroadcast {
    /// Registration ID.
    pub registration_id: TournamentRegistrationId,
    /// Team name.
    pub team_name: String,
    /// Username.
    pub username: String,
}

impl ChatBroadcast {
    /// Convert to server message for a specific connection.
    pub fn to_server_message(&self) -> ServerMessage {
        ServerMessage::Chat {
            id: self.message.id.to_string(),
            chat_type: self.message.message_type.to_string(),
            author: ChatAuthorPayload {
                user_id: self.message.author_user_id.to_string(),
                username: self.author_username.clone(),
                registration_id: self.message.author_registration_id.map(|id| id.to_string()),
                team_name: self.author_team_name.clone(),
            },
            content: self.message.content.clone(),
            timestamp: self.message.created_at,
        }
    }

    /// Check if this message is visible to a participant.
    pub fn is_visible_to_participant(&self, registration_id: TournamentRegistrationId) -> bool {
        self.message.is_visible_to_team(registration_id)
    }

    /// Check if this message is visible to spectators.
    pub fn is_visible_to_spectators(&self) -> bool {
        self.message.is_visible_to_spectators()
    }
}
