//! Server-facing enrollment endpoint and the agent WebSocket channel.
//!
//! Deliberately NOT annotated with `#[utoipa::path]`: these endpoints are
//! machine-facing (the portal-server-agent), authenticated by one-time
//! enrollment tokens and mTLS client certificates rather than user JWTs,
//! and are excluded from the public OpenAPI spec — same policy as
//! `handlers/internal.rs`.
//!
//! mTLS terminates at Caddy (§5.4): the `agents.<domain>` site verifies the
//! client certificate against the portal CA and forwards its serial in
//! `X-Client-Cert-Serial`. The API trusts that header ONLY on these routes
//! and only because it listens on loopback behind the proxy. For dev/e2e,
//! `PORTAL_AGENT_INSECURE=true` accepts `X-Dev-Server-Id` instead.

use crate::error::{ApiError, ApiResult};
use crate::state::GameServerState;
use crate::websocket::agent_manager::{AgentMessage, AgentSession};
use axum::Json;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use portal_core::ids::GameServerId;
use portal_core::types::AgentGamestate;
use portal_domain::entities::{GameServer, HeartbeatUpdate};
use serde::{Deserialize, Serialize};

/// Header set by Caddy after client-cert verification.
const CLIENT_CERT_SERIAL_HEADER: &str = "x-client-cert-serial";
/// Dev-mode identity header (only honored with `PORTAL_AGENT_INSECURE`).
const DEV_SERVER_ID_HEADER: &str = "x-dev-server-id";

// =============================================================================
// Enrollment
// =============================================================================

/// Request body for agent enrollment.
#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    /// One-time token minted by an admin.
    pub enrollment_token: String,
    /// PEM-encoded PKCS#10 certificate signing request.
    pub csr_pem: String,
}

/// Successful enrollment: the signed client certificate and trust anchor.
#[derive(Debug, Serialize)]
pub struct EnrollResponse {
    pub server_id: String,
    pub server_name: String,
    /// Signed client certificate (PEM).
    pub certificate_pem: String,
    /// The portal CA certificate to pin (PEM).
    pub ca_certificate_pem: String,
    pub expires_at: String,
}

/// Exchange a one-time enrollment token + CSR for a signed agent certificate.
pub async fn enroll(
    State(state): State<GameServerState>,
    Json(body): Json<EnrollRequest>,
) -> ApiResult<(StatusCode, Json<EnrollResponse>)> {
    let ca = state.agent_ca.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(
            "game-server integration is not configured (PORTAL_AGENT_CA_DIR unset)",
        )
    })?;

    let result = state
        .registry
        .enroll(&body.enrollment_token, &body.csr_pem, ca)
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(EnrollResponse {
            server_id: result.server.id.to_string(),
            server_name: result.server.name.clone(),
            certificate_pem: result.cert_pem,
            ca_certificate_pem: result.ca_cert_pem,
            expires_at: result.certificate.not_after.to_rfc3339(),
        }),
    ))
}

// =============================================================================
// Agent WebSocket
// =============================================================================

/// Resolve the calling agent's server identity from the proxy-verified
/// client-cert serial (or the dev header in insecure mode).
async fn authenticate(
    state: &GameServerState,
    headers: &HeaderMap,
) -> Result<GameServer, ApiError> {
    if let Some(serial) = headers
        .get(CLIENT_CERT_SERIAL_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        // Caddy normalizes serials to lowercase hex; ours are stored likewise.
        return state
            .registry
            .authenticate_agent(&serial.to_ascii_lowercase())
            .await
            .map_err(ApiError::from);
    }

    if state.insecure_dev_auth
        && let Some(raw) = headers.get(DEV_SERVER_ID_HEADER).and_then(|v| v.to_str().ok())
    {
        let id: GameServerId = raw
            .parse()
            .map_err(|_| ApiError::bad_request("invalid dev server id"))?;
        return state.registry.get(id).await.map_err(ApiError::from);
    }

    Err(ApiError::unauthorized(
        "agent authentication requires a verified client certificate",
    ))
}

/// Upgrade an authenticated agent connection to a WebSocket.
pub async fn ws_upgrade(
    State(state): State<GameServerState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    let server = authenticate(&state, &headers).await?;
    tracing::info!(server_id = %server.id, name = %server.name, "agent connected");
    Ok(ws.on_upgrade(move |socket| handle_socket(state, server, socket)))
}

async fn handle_socket(state: GameServerState, server: GameServer, socket: WebSocket) {
    let server_id = server.id;
    let mut session: AgentSession = state.agent_manager.register(server_id);
    let connection_id = session.connection_id;
    let (mut sink, mut stream) = socket.split();

    loop {
        tokio::select! {
            // Frames the portal wants to push to the agent. `None` means the
            // manager replaced or dropped this connection (reconnect/revoke).
            frame = session.outbound_rx.recv() => {
                match frame {
                    Some(text) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            // Frames from the agent: heartbeats and command results.
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_agent_message(&state, server_id, &session, text.as_str()).await;
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                }
            }
        }
    }

    state.agent_manager.remove(server_id, connection_id);
    // Only stamp offline if we are still the current connection — a
    // reconnecting agent must not be marked offline by its old socket.
    if !state.agent_manager.is_connected(server_id) {
        if let Err(e) = state.registry.mark_disconnected(server_id).await {
            tracing::warn!(server_id = %server_id, error = %e, "failed to mark server offline");
        }
        tracing::info!(server_id = %server_id, "agent disconnected");
    }
}

async fn handle_agent_message(
    state: &GameServerState,
    server_id: GameServerId,
    session: &AgentSession,
    text: &str,
) {
    match serde_json::from_str::<AgentMessage>(text) {
        Ok(AgentMessage::Heartbeat(hb)) => {
            if hb.frame_type != "heartbeat" {
                tracing::debug!(server_id = %server_id, frame_type = %hb.frame_type,
                    "ignoring unknown agent frame type");
                return;
            }
            let gamestate = hb.get5_status.as_ref().and_then(|s| {
                s.get("gamestate")
                    .and_then(|g| g.as_str())
                    .map(AgentGamestate::parse_lenient)
            });
            let reported_matchzy_id = hb.get5_status.as_ref().and_then(|s| {
                let m = s.get("matchid")?;
                m.as_i64().or_else(|| m.as_str()?.parse().ok())
            });
            let update = HeartbeatUpdate {
                agent_version: hb.agent_version,
                rcon_ok: hb.rcon_ok,
                gamestate,
                reported_matchzy_id,
            };
            // Phase 1: no reservations exist yet, so a loaded match is
            // always out-of-band (`busy_external`, §6.7).
            if let Err(e) = state
                .registry
                .record_heartbeat(server_id, update, false)
                .await
            {
                tracing::warn!(server_id = %server_id, error = %e, "heartbeat processing failed");
            }
        }
        Ok(AgentMessage::CommandResult(frame)) => session.resolve(frame),
        Err(e) => {
            tracing::debug!(server_id = %server_id, error = %e, "unparseable agent frame");
        }
    }
}
