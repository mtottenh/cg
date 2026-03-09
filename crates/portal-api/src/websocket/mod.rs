//! WebSocket infrastructure for real-time features.
//!
//! This module provides the foundation for WebSocket-based real-time communication,
//! starting with the veto lobby system.
//!
//! ## Architecture
//!
//! - `lobby_manager`: Manages all active lobbies, keyed by match ID
//! - `lobby`: Individual lobby instance with broadcast channel
//! - `connection`: Represents a single WebSocket connection with role/permissions
//! - `messages`: Client/server message types and broadcast payloads

pub mod connection;
pub mod lobby;
pub mod lobby_manager;
pub mod messages;
pub mod timeout_task;

pub use connection::{ConnectionRole, VetoConnection};
pub use lobby::{ConnectionId, VetoLobby};
pub use lobby_manager::VetoLobbyManager;
pub use messages::{
    ChatBroadcast, ClientChatType, ClientMessage, ClientVetoAction, CoinFlipResultBroadcast,
    LobbyBroadcast, ParticipantConnectionBroadcast, ServerMessage, TimeoutWarningBroadcast,
    VetoActionBroadcast, VetoCompleteBroadcast, VetoStateBroadcast,
};
pub use timeout_task::spawn_timeout_warning_task;
