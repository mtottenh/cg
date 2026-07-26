//! PUG lobby WebSocket handler.
//!
//! `GET /v1/ws/pug/{pug_id}` — real-time gathering (and whole-lifecycle)
//! updates for a PUG lobby. Auth is in-band like the veto socket: the first
//! frame must be `{"type":"auth","token":"<jwt>","code":"<invite>"?}` within
//! the timeout. Authorization mirrors the REST detail endpoint
//! (`PugService::authorize_view`): participants always; share-link viewers
//! with the code; listed or finished lobbies for anyone signed in.
//!
//! The server only ever pushes doorbell frames (`pug_changed`,
//! `rematch_created`) — mutations stay on REST, and clients refetch the
//! viewer-specific detail on each ring.

use std::time::Duration;

use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use portal_core::PugId;
use portal_domain::validate_token;
use tokio::time::timeout;
use tracing::{debug, info};

use crate::state::AppState;

const AUTH_TIMEOUT_SECS: u64 = 10;
const PING_INTERVAL_SECS: u64 = 30;

/// Upgrade to a PUG lobby WebSocket connection.
pub async fn ws_upgrade(
    State(state): State<AppState>,
    Path(pug_id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, pug_id, state))
}

#[derive(serde::Deserialize)]
struct AuthFrame {
    #[serde(rename = "type")]
    kind: String,
    token: String,
    /// Invite code for share-link viewers who haven't joined yet.
    code: Option<String>,
}

async fn handle_socket(socket: WebSocket, raw_pug_id: String, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    let Ok(pug_uuid) = raw_pug_id.parse::<uuid::Uuid>() else {
        let _ = sender.send(auth_error("invalid pug id")).await;
        return;
    };
    let pug_id = PugId::from_uuid(pug_uuid);

    // First frame must be auth, within the timeout.
    let first = timeout(Duration::from_secs(AUTH_TIMEOUT_SECS), receiver.next()).await;
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        let _ = sender.send(auth_error("authentication timeout")).await;
        return;
    };

    let Ok(auth) = serde_json::from_str::<AuthFrame>(&text) else {
        let _ = sender.send(auth_error("first message must be auth")).await;
        return;
    };
    if auth.kind != "auth" {
        let _ = sender.send(auth_error("first message must be auth")).await;
        return;
    }

    let authorized = async {
        let claims = validate_token(&auth.token, &state.jwt_secret)
            .map_err(|e| format!("invalid token: {e}"))?;
        let player_id = portal_core::PlayerId::from(claims.player_id);
        let pug = state
            .pug_service
            .get(pug_id)
            .await
            .map_err(|e| e.to_string())?;
        state
            .pug_service
            .authorize_view(&pug, Some(player_id), auth.code.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    }
    .await;

    if let Err(reason) = authorized {
        debug!(%pug_id, %reason, "pug ws auth refused");
        let _ = sender.send(auth_error(&reason)).await;
        return;
    }

    let _ = sender
        .send(Message::Text(
            serde_json::json!({ "type": "auth_success", "pug_id": raw_pug_id })
                .to_string()
                .into(),
        ))
        .await;

    info!(%pug_id, "pug lobby ws connected");
    let mut broadcasts = state.pug_lobby_manager.subscribe(pug_id);
    let mut ping = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));
    ping.tick().await; // first tick fires immediately; skip it

    loop {
        tokio::select! {
            frame = broadcasts.recv() => {
                match frame {
                    Ok(message) => {
                        let Ok(text) = serde_json::to_string(&message) else { continue };
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    // Lagged: the client missed frames — one doorbell catches
                    // it up, since frames carry no state.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let catch_up = serde_json::json!({
                            "type": "pug_changed",
                            "reason": "resync",
                        });
                        if sender.send(Message::Text(catch_up.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if text.contains("\"ping\"") {
                            let pong = serde_json::json!({ "type": "pong" });
                            if sender.send(Message::Text(pong.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        // Everything else is ignored: mutations are REST-only.
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = ping.tick() => {
                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }

    debug!(%pug_id, "pug lobby ws disconnected");
}

fn auth_error(message: &str) -> Message {
    Message::Text(
        serde_json::json!({ "type": "auth_error", "error": message })
            .to_string()
            .into(),
    )
}
