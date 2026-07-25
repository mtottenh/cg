//! Admin game-server registry handlers.

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::GameServerState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Utc};
use portal_core::ids::{GameId, GameServerId, ServerBookingId, TournamentId};
use portal_core::permissions;
use portal_core::types::GameServerStatus;
use portal_domain::entities::{GameServer, ServerBooking};
use portal_domain::repositories::{CreateGameServer, CreateServerBooking, UpdateGameServer};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// DTOs
// =============================================================================

/// Request to register a game server.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateGameServerRequest {
    /// Display name (e.g. "London #1").
    #[validate(length(min = 1, max = 128, message = "name must be 1-128 characters"))]
    pub name: String,
    /// Game UUID this server hosts.
    pub game_id: String,
    /// Public IPv4/IPv6 address players connect to.
    pub ip_address: String,
    /// Game port (also the RCON port on the server host).
    #[validate(range(min = 1, max = 65535, message = "port must be 1-65535"))]
    pub port: u16,
    /// GOTV port, if GOTV is enabled.
    #[validate(range(min = 1, max = 65535, message = "gotv_port must be 1-65535"))]
    pub gotv_port: Option<u16>,
    /// Region label used by allocation (e.g. "eu-west").
    #[validate(length(min = 1, max = 32, message = "region must be 1-32 characters"))]
    pub region: String,
}

/// Partial update of a game server. Absent fields are unchanged.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct UpdateGameServerRequest {
    #[validate(length(min = 1, max = 128, message = "name must be 1-128 characters"))]
    pub name: Option<String>,
    pub ip_address: Option<String>,
    #[validate(range(min = 1, max = 65535, message = "port must be 1-65535"))]
    pub port: Option<u16>,
    #[validate(range(min = 1, max = 65535, message = "gotv_port must be 1-65535"))]
    pub gotv_port: Option<u16>,
    #[validate(length(min = 1, max = 32, message = "region must be 1-32 characters"))]
    pub region: Option<String>,
    /// Admin kill-switch; a disabled server is never allocated.
    pub enabled: Option<bool>,
}

/// A registered game server.
#[derive(Debug, Serialize, ToSchema)]
pub struct GameServerResponse {
    pub id: String,
    pub name: String,
    pub game_id: String,
    pub ip_address: String,
    pub port: u16,
    pub gotv_port: Option<u16>,
    pub region: String,
    pub enabled: bool,
    pub status: GameServerStatus,
    pub current_match_id: Option<String>,
    /// Whether the agent's WebSocket is connected right now.
    pub agent_connected: bool,
    pub agent_version: Option<String>,
    pub agent_cert_expires_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
    /// MatchZy gamestate from the last heartbeat.
    pub last_gamestate: Option<String>,
    /// Whether an unexpired enrollment token is outstanding.
    pub enrollment_open: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl GameServerResponse {
    fn from_entity(s: &GameServer, agent_connected: bool) -> Self {
        Self {
            id: s.id.to_string(),
            name: s.name.clone(),
            game_id: s.game_id.to_string(),
            ip_address: s.ip_address.to_string(),
            port: s.port,
            gotv_port: s.gotv_port,
            region: s.region.clone(),
            enabled: s.enabled,
            status: s.status,
            current_match_id: s.current_match_id.map(|id| id.to_string()),
            agent_connected,
            agent_version: s.agent_version.clone(),
            agent_cert_expires_at: s.agent_cert_expires_at.map(|dt| dt.to_rfc3339()),
            last_heartbeat_at: s.last_heartbeat_at.map(|dt| dt.to_rfc3339()),
            last_gamestate: s.last_gamestate.map(|g| g.to_string()),
            enrollment_open: s.enrollment_open(Utc::now()),
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

/// A freshly minted enrollment token — shown exactly once.
#[derive(Debug, Serialize, ToSchema)]
pub struct EnrollmentTokenResponse {
    /// The raw one-time token. Never retrievable again.
    pub token: String,
    pub expires_at: String,
}

/// Result of revoking a server's agent certificates.
#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeAgentResponse {
    /// Number of certificates revoked.
    pub revoked_count: u64,
}

/// Request to create a booking (scheduled event hold).
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateServerBookingRequest {
    /// Tournament the hold is for; omit for a hard hold (maintenance).
    pub tournament_id: Option<String>,
    #[validate(length(max = 255, message = "reason must be at most 255 characters"))]
    pub reason: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// A server booking.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServerBookingResponse {
    pub id: String,
    pub server_id: String,
    pub tournament_id: Option<String>,
    pub reason: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
    pub created_by: String,
    pub created_at: String,
}

impl ServerBookingResponse {
    fn from_entity(b: &ServerBooking) -> Self {
        Self {
            id: b.id.to_string(),
            server_id: b.server_id.to_string(),
            tournament_id: b.tournament_id.map(|t| t.to_string()),
            reason: b.reason.clone(),
            starts_at: b.starts_at.to_rfc3339(),
            ends_at: b.ends_at.to_rfc3339(),
            created_by: b.created_by.to_string(),
            created_at: b.created_at.to_rfc3339(),
        }
    }
}

fn parse_server_id(id: &str) -> Result<GameServerId, ApiError> {
    id.parse()
        .map_err(|_| ApiError::bad_request(format!("invalid game server id: {id}")))
}

fn parse_ip(ip: &str) -> Result<std::net::IpAddr, ApiError> {
    ip.parse()
        .map_err(|_| ApiError::bad_request(format!("invalid IP address: {ip}")))
}

// =============================================================================
// Handlers
// =============================================================================

/// List all registered game servers.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers",
    responses(
        (status = 200, description = "Registered servers", body = DataResponse<Vec<GameServerResponse>>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn list_game_servers(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<GameServerResponse>>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);

    let servers = state.registry.list().await.map_err(ApiError::from)?;
    let response = servers
        .iter()
        .map(|s| GameServerResponse::from_entity(s, state.agent_manager.is_connected(s.id)))
        .collect();
    Ok(Json(DataResponse::new(response, request_id)))
}

/// Register a new game server.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers",
    request_body = CreateGameServerRequest,
    responses(
        (status = 201, description = "Server registered", body = DataResponse<GameServerResponse>),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn create_game_server(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Json(body): Json<CreateGameServerRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<GameServerResponse>>)> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let game_id: GameId = body
        .game_id
        .parse()
        .map_err(|_| ApiError::bad_request(format!("invalid game id: {}", body.game_id)))?;

    let server = state
        .registry
        .register(CreateGameServer {
            id: GameServerId::new(),
            name: body.name,
            game_id,
            ip_address: parse_ip(&body.ip_address)?,
            port: body.port,
            gotv_port: body.gotv_port,
            region: body.region,
        })
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            GameServerResponse::from_entity(&server, false),
            request_id,
        )),
    ))
}

/// Get a game server.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers/{server_id}",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 200, description = "Server detail", body = DataResponse<GameServerResponse>),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn get_game_server(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<Json<DataResponse<GameServerResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;

    let server = state.registry.get(id).await.map_err(ApiError::from)?;
    Ok(Json(DataResponse::new(
        GameServerResponse::from_entity(&server, state.agent_manager.is_connected(id)),
        request_id,
    )))
}

/// Update a game server's registration fields.
#[utoipa::path(
    patch,
    path = "/v1/admin/game-servers/{server_id}",
    params(("server_id" = String, Path, description = "Game server ID")),
    request_body = UpdateGameServerRequest,
    responses(
        (status = 200, description = "Server updated", body = DataResponse<GameServerResponse>),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn update_game_server(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<UpdateGameServerRequest>,
) -> ApiResult<Json<DataResponse<GameServerResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let ip_address = body.ip_address.as_deref().map(parse_ip).transpose()?;
    let server = state
        .registry
        .update(
            id,
            UpdateGameServer {
                name: body.name,
                ip_address,
                port: body.port,
                gotv_port: body.gotv_port.map(Some),
                region: body.region,
                enabled: body.enabled,
            },
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(DataResponse::new(
        GameServerResponse::from_entity(&server, state.agent_manager.is_connected(id)),
        request_id,
    )))
}

/// Remove a game server. Refused while it is busy with a match.
#[utoipa::path(
    delete,
    path = "/v1/admin/game-servers/{server_id}",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 204, description = "Server removed"),
        (status = 400, description = "Server is busy", body = ApiError),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn delete_game_server(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path(server_id): Path<String>,
) -> ApiResult<StatusCode> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let id = parse_server_id(&server_id)?;
    state.registry.delete(id).await.map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Mint a one-time enrollment token for a server's agent.
///
/// The raw token is returned exactly once; minting again invalidates any
/// previous token.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/enrollment-token",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 201, description = "Token minted", body = DataResponse<EnrollmentTokenResponse>),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn mint_enrollment_token(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<(StatusCode, Json<DataResponse<EnrollmentTokenResponse>>)> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;

    let (token, expires_at) = state
        .registry
        .mint_enrollment_token(id)
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            EnrollmentTokenResponse {
                token,
                expires_at: expires_at.to_rfc3339(),
            },
            request_id,
        )),
    ))
}

/// Revoke a server's agent certificates and drop its connection.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/revoke",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 200, description = "Certificates revoked", body = DataResponse<RevokeAgentResponse>),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn revoke_agent(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<Json<DataResponse<RevokeAgentResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;

    let revoked_count = state
        .registry
        .revoke_agent(id)
        .await
        .map_err(ApiError::from)?;
    // Sever the live connection, if any: its cert is no longer valid.
    state.agent_manager.disconnect(id);

    Ok(Json(DataResponse::new(
        RevokeAgentResponse { revoked_count },
        request_id,
    )))
}

/// List current and upcoming bookings for a server.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers/{server_id}/bookings",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 200, description = "Bookings", body = DataResponse<Vec<ServerBookingResponse>>),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn list_bookings(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<ServerBookingResponse>>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;

    let bookings = state
        .registry
        .list_bookings(id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(DataResponse::new(
        bookings
            .iter()
            .map(ServerBookingResponse::from_entity)
            .collect(),
        request_id,
    )))
}

/// Create a booking (scheduled event hold) on a server.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/bookings",
    params(("server_id" = String, Path, description = "Game server ID")),
    request_body = CreateServerBookingRequest,
    responses(
        (status = 201, description = "Booking created", body = DataResponse<ServerBookingResponse>),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn create_booking(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<CreateServerBookingRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<ServerBookingResponse>>)> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let tournament_id: Option<TournamentId> = body
        .tournament_id
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|_| ApiError::bad_request("invalid tournament id"))?;

    let booking = state
        .registry
        .create_booking(CreateServerBooking {
            id: ServerBookingId::new(),
            server_id: id,
            tournament_id,
            reason: body.reason,
            starts_at: body.starts_at,
            ends_at: body.ends_at,
            created_by: auth.user_id,
        })
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            ServerBookingResponse::from_entity(&booking),
            request_id,
        )),
    ))
}

/// Delete a booking.
#[utoipa::path(
    delete,
    path = "/v1/admin/game-servers/{server_id}/bookings/{booking_id}",
    params(
        ("server_id" = String, Path, description = "Game server ID"),
        ("booking_id" = String, Path, description = "Booking ID"),
    ),
    responses(
        (status = 204, description = "Booking deleted"),
        (status = 404, description = "Not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn delete_booking(
    State(state): State<GameServerState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((_server_id, booking_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let id: ServerBookingId = booking_id
        .parse()
        .map_err(|_| ApiError::bad_request(format!("invalid booking id: {booking_id}")))?;
    state
        .registry
        .delete_booking(id)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
