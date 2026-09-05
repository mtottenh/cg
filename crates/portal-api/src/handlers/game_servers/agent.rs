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
use portal_domain::repositories::ServerReservationRepository;
use portal_plugins::games::cs2::console;
use serde::{Deserialize, Serialize};

/// Header set by Caddy after client-cert verification.
const CLIENT_CERT_SERIAL_HEADER: &str = "x-client-cert-serial";
/// Marker only the agents vhost sets (and the main site strips): the
/// serial header is trusted ONLY when this accompanies it (§5.4 — without
/// it, a client could post a forged serial through the public site).
const AGENT_VHOST_HEADER: &str = "x-portal-agent-vhost";
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
    /// Server-scoped demo-upload token — write into the permanent MatchZy
    /// config (§6.3); shown once.
    pub demo_token: String,
    /// Absolute URL MatchZy should upload demos to.
    pub demo_upload_url: String,
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

    let base = state.public_base_url.trim_end_matches('/');
    Ok((
        StatusCode::CREATED,
        Json(EnrollResponse {
            server_id: result.server.id.to_string(),
            server_name: result.server.name.clone(),
            certificate_pem: result.cert_pem,
            ca_certificate_pem: result.ca_cert_pem,
            expires_at: result.certificate.not_after.to_rfc3339(),
            demo_token: result.demo_token,
            demo_upload_url: format!("{base}/v1/gameserver/demos"),
        }),
    ))
}

// =============================================================================
// Agent WebSocket
// =============================================================================

/// Resolve the calling agent's server identity from the proxy-verified
/// Normalize a forwarded client-cert serial to the stored format:
/// even-length lowercase hex. Caddy forwards decimal (Go big.Int); an
/// all-digit value that parses as u128 (serials are <= 16 bytes) is
/// converted; anything else is treated as hex and lowercased.
fn normalize_serial(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    if !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && let Ok(v) = s.parse::<u128>()
    {
        let hex = format!("{v:x}");
        if hex.len() % 2 == 1 {
            return format!("0{hex}");
        }
        return hex;
    }
    s
}

/// client-cert serial (or the dev header in insecure mode).
async fn authenticate(
    state: &GameServerState,
    headers: &HeaderMap,
) -> Result<GameServer, ApiError> {
    let via_agent_vhost = headers
        .get(AGENT_VHOST_HEADER)
        .and_then(|v| v.to_str().ok())
        == Some("1");
    if via_agent_vhost
        && let Some(serial) = headers
            .get(CLIENT_CERT_SERIAL_HEADER)
            .and_then(|v| v.to_str().ok())
    {
        // Caddy's {tls_client_serial} placeholder forwards the serial as a
        // DECIMAL big-integer string (Go big.Int); enrollment stores
        // even-length lowercase hex (as OpenSSL prints it). Accept both —
        // the mismatch 403'd every real agent on the first deployment.
        return state
            .registry
            .authenticate_agent(&normalize_serial(serial))
            .await
            .map_err(ApiError::from);
    }

    if state.insecure_dev_auth
        && let Some(raw) = headers
            .get(DEV_SERVER_ID_HEADER)
            .and_then(|v| v.to_str().ok())
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
                        crate::observability::record_ws_message("gameserver-agent", "out");
                    }
                    None => break,
                }
            }
            // Frames from the agent: heartbeats and command results.
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        crate::observability::record_ws_message("gameserver-agent", "in");
                        handle_agent_message(&state, server_id, &session, text.as_str()).await;
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_))) => {}
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
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
            // Portal-side agent aggregation (observability-design.md §4.6):
            // alerts about agents come from here, never from scraping them.
            crate::observability::record_agent_heartbeat(&server_id.to_string());
            let gamestate = hb.get5_status.as_ref().and_then(|s| {
                s.get("gamestate")
                    .and_then(|g| g.as_str())
                    .map(AgentGamestate::parse_lenient)
            });
            let reported_matchzy_id = hb.get5_status.as_ref().and_then(|s| {
                let m = s.get("matchid")?;
                m.as_i64().or_else(|| m.as_str()?.parse().ok())
            });
            // CS2's `status` (agent 0.2.0+): the map and who is connected.
            // Parsed here rather than in the domain crate, which must not
            // depend on the plugin crate; stored raw with addresses redacted.
            let cs2 = hb.status_output.as_deref().map(|raw| {
                let redacted = console::redact_addresses(raw);
                let parsed = console::parse_status(&redacted);
                (parsed, console::sanitise_output(&redacted, 8 * 1024))
            });
            let update = HeartbeatUpdate {
                agent_version: hb.agent_version,
                rcon_ok: hb.rcon_ok,
                gamestate,
                reported_matchzy_id,
                last_map: cs2.as_ref().and_then(|(p, _)| p.map.clone()),
                last_player_count: cs2
                    .as_ref()
                    .map(|(p, _)| i32::try_from(p.player_count()).unwrap_or(i32::MAX)),
                status_output: cs2.map(|(_, raw)| raw),
            };
            // §6.7 rule 3: a loaded match is OURS when a live reservation
            // exists and the reported matchid matches (or is unreported).
            let ours = match state
                .server_reservation_repo
                .find_live_by_server(server_id)
                .await
            {
                Ok(Some(reservation)) => update
                    .reported_matchzy_id
                    .is_none_or(|id| id == reservation.matchzy_id),
                Ok(None) => false,
                Err(e) => {
                    tracing::warn!(server_id = %server_id, error = %e,
                        "live-reservation lookup failed; assuming ours");
                    true
                }
            };
            if let Err(e) = state
                .registry
                .record_heartbeat(server_id, update, ours)
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

/// Request body for certificate renewal.
#[derive(Debug, Deserialize)]
pub struct RenewRequest {
    /// Fresh PEM-encoded CSR (a new local keypair is recommended).
    pub csr_pem: String,
}

/// Renewal response: the re-signed certificate.
#[derive(Debug, Serialize)]
pub struct RenewResponse {
    pub certificate_pem: String,
    pub expires_at: String,
}

/// Renew the calling agent's certificate (§5.3 step 4, mTLS-authenticated
/// — the agents vhost verified the CURRENT cert to get here).
pub async fn renew(
    State(state): State<GameServerState>,
    headers: HeaderMap,
    Json(body): Json<RenewRequest>,
) -> ApiResult<Json<RenewResponse>> {
    let ca = state.agent_ca.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(
            "game-server integration is not configured (PORTAL_AGENT_CA_DIR unset)",
        )
    })?;
    let server = authenticate(&state, &headers).await?;
    let (certificate, cert_pem) = state
        .registry
        .renew_certificate(server.id, &body.csr_pem, ca)
        .await
        .map_err(ApiError::from)?;
    tracing::info!(server_id = %server.id, serial = %certificate.serial,
        "agent certificate renewed");
    Ok(Json(RenewResponse {
        certificate_pem: cert_pem,
        expires_at: certificate.not_after.to_rfc3339(),
    }))
}
