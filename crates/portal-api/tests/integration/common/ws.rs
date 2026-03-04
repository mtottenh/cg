//! WebSocket test helpers.

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::Message,
    MaybeTlsStream, WebSocketStream,
};
use tokio::net::TcpStream;

/// WebSocket connection type for tests.
pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Server message types for parsing WebSocket responses.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ServerMessage {
    /// Authentication success.
    AuthSuccess {
        role: String,
        registration_id: Option<String>,
        team_name: Option<String>,
        lobby_state: serde_json::Value,
    },
    /// Authentication error.
    AuthError {
        error: String,
    },
    /// Chat message.
    Chat {
        id: String,
        chat_type: String,
        author: serde_json::Value,
        content: String,
        timestamp: String,
    },
    /// Chat history.
    ChatHistory {
        messages: Vec<serde_json::Value>,
    },
    /// Veto state update.
    VetoStateUpdate {
        session: serde_json::Value,
    },
    /// Veto action performed.
    VetoActionPerformed {
        session: serde_json::Value,
        action: serde_json::Value,
        is_complete: bool,
    },
    /// Veto complete.
    VetoComplete {
        selected_maps: Vec<String>,
        session: serde_json::Value,
    },
    /// Player connected.
    PlayerConnected {
        registration_id: String,
        team_name: String,
        username: String,
    },
    /// Player disconnected.
    PlayerDisconnected {
        registration_id: String,
        team_name: String,
        username: String,
    },
    /// Spectator count update.
    SpectatorCount {
        count: usize,
    },
    /// Error message.
    Error {
        code: String,
        message: String,
    },
    /// Pong response.
    Pong,
    /// Veto action acknowledgment.
    VetoActionAck {
        success: bool,
        message: Option<String>,
    },
    /// Timeout warning.
    TimeoutWarning {
        seconds_remaining: u32,
        current_team: String,
        current_team_registration_id: String,
    },
}

/// Connect to the veto WebSocket endpoint.
pub async fn connect_veto_ws(addr: SocketAddr, match_id: &str) -> WsStream {
    let url = format!("ws://{}/v1/ws/veto/{}", addr, match_id);
    let (ws_stream, _) = connect_async(&url)
        .await
        .expect("Failed to connect to WebSocket");
    ws_stream
}

/// Send authentication message and wait for response.
pub async fn ws_authenticate(ws: &mut WsStream, token: &str) -> ServerMessage {
    let auth_msg = json!({
        "type": "auth",
        "token": token
    });
    ws.send(Message::Text(auth_msg.to_string().into()))
        .await
        .expect("Failed to send auth message");

    // Wait for auth response (with timeout)
    let response = timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("Timeout waiting for auth response")
        .expect("Connection closed")
        .expect("WebSocket error");

    parse_server_message(&response)
}

/// Send a veto ban action.
pub async fn ws_ban_map(ws: &mut WsStream, map_id: &str) {
    let msg = json!({
        "type": "veto_action",
        "action": {
            "action": "ban",
            "map_id": map_id
        }
    });
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .expect("Failed to send ban action");
}

/// Send a veto pick action.
#[allow(dead_code)]
pub async fn ws_pick_map(ws: &mut WsStream, map_id: &str) {
    let msg = json!({
        "type": "veto_action",
        "action": {
            "action": "pick",
            "map_id": map_id
        }
    });
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .expect("Failed to send pick action");
}

/// Send a side select action.
#[allow(dead_code)]
pub async fn ws_select_side(ws: &mut WsStream, action_number: u32, side: &str) {
    let msg = json!({
        "type": "veto_action",
        "action": {
            "action": "select_side",
            "action_number": action_number,
            "side": side
        }
    });
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .expect("Failed to send side select action");
}

/// Send a chat message.
#[allow(dead_code)]
pub async fn ws_send_chat(ws: &mut WsStream, chat_type: &str, content: &str) {
    let msg = json!({
        "type": "chat",
        "chat_type": chat_type,
        "content": content
    });
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .expect("Failed to send chat message");
}

/// Wait for the next server message (with timeout).
pub async fn ws_next_message(ws: &mut WsStream) -> Option<ServerMessage> {
    let result = timeout(Duration::from_secs(5), ws.next())
        .await
        .ok()?
        .and_then(|r| r.ok())?;

    Some(parse_server_message(&result))
}

/// Wait for a specific message type (with timeout).
pub async fn ws_wait_for<F>(ws: &mut WsStream, predicate: F) -> Option<ServerMessage>
where
    F: Fn(&ServerMessage) -> bool,
{
    let start = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(5);

    while start.elapsed() < timeout_duration {
        if let Some(msg) = ws_next_message(ws).await {
            if predicate(&msg) {
                return Some(msg);
            }
        }
    }
    None
}

fn parse_server_message(msg: &Message) -> ServerMessage {
    match msg {
        Message::Text(text) => serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("Failed to parse server message: {}\nRaw: {}", e, text)),
        Message::Ping(_) => ServerMessage::Pong, // Treat ping as pong for simplicity
        other => panic!("Unexpected message type: {:?}", other),
    }
}
