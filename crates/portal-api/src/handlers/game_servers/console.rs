//! The CS2 server console (docs/server-console-design.md §5.3–§5.5).
//!
//! Everything here rides the agent's outbound socket, checks
//! `admin.servers.manage`, and writes one row to `admin_server_commands`
//! per command sent. Two entries of the design's action table are not
//! offered because agent 0.2.0 refuses the verbs they would need at its
//! end: `set_password` (`sv_password` is portal-owned) and `exec_cfg`
//! (`exec` is refused as an escape from the check).

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::game_server_flow;
use crate::state::AppState;
use crate::websocket::agent_manager::AgentCommand;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Duration, Utc};
use portal_core::ids::{GameId, GameServerId, ServerBookingId, UserId};
use portal_core::permissions;
use portal_core::types::{GameServerStatus, ReservationKind, ReservationStatus};
use portal_domain::entities::{AdminServerCommand, GameServer, ServerBooking, ServerReservation};
use portal_domain::repositories::{
    AdminServerCommandRepository, CreateAdminServerCommand, CreateServerBooking,
    ServerReservationRepository,
};
use portal_plugins::MapInfo;
use portal_plugins::games::cs2::console;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

/// A live RCON reply can be a megabyte; nothing downstream needs more.
const LIVE_OUTPUT_MAX: usize = 64 * 1024;
/// What the audit row keeps of an output.
const AUDIT_OUTPUT_MAX: usize = 4 * 1024;
/// Raw passthrough commands are one console line.
const RAW_COMMAND_MAX: u64 = 512;
/// Allocator hold placed by a map change (stock / workshop).
const MAP_HOLD_MINUTES: i64 = 5;
const WORKSHOP_HOLD_MINUTES: i64 = 10;
/// Practice holds: default and ceiling.
const PRACTICE_DEFAULT_HOURS: i64 = 2;
const PRACTICE_MAX_HOURS: i64 = 12;
/// Booking reasons the console writes, so `practice_stop` can find its hold.
const PRACTICE_REASON_PREFIX: &str = "Practice (console)";
const MAP_HOLD_REASON_PREFIX: &str = "Map change (console)";

fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

fn parse_server_id(id: &str) -> Result<GameServerId, ApiError> {
    id.parse()
        .map_err(|_| ApiError::bad_request(format!("invalid game server id: {id}")))
}

// =============================================================================
// DTOs
// =============================================================================

/// Query for the console snapshot.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ConsoleQuery {
    /// Run `status` on the server now instead of reading the last heartbeat.
    #[serde(default)]
    pub live: bool,
}

/// The agent's side of the connection.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsoleAgentInfo {
    pub connected: bool,
    pub version: Option<String>,
    pub heartbeat_at: Option<String>,
    /// Whether the agent could reach RCON at its last heartbeat; absent
    /// before the first heartbeat.
    pub rcon_ok: Option<bool>,
    /// Whether this agent reports CS2's `status` (agent 0.2.0+).
    pub reports_status: bool,
}

/// A portal player matched to a connected Steam account.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ConsolePlayerLink {
    pub id: String,
    pub display_name: String,
}

/// One row of CS2's `status`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsolePlayer {
    pub userid: u32,
    pub name: String,
    /// Steam ID64 as a string (JSON numbers lose precision above 2^53).
    pub steam_id64: Option<String>,
    pub bot: bool,
    pub connected_secs: Option<u32>,
    pub ping: Option<u32>,
    pub loss: Option<u32>,
    pub state: Option<String>,
    /// The portal account behind the Steam id, when one exists.
    pub player: Option<ConsolePlayerLink>,
}

/// What `status` said about the server.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsoleStatus {
    pub hostname: Option<String>,
    /// Engine map name, e.g. `de_mirage`.
    pub map: Option<String>,
    pub humans: Option<u32>,
    pub bots: Option<u32>,
    pub max_players: Option<u32>,
    pub players: Vec<ConsolePlayer>,
}

/// The live reservation on the server, if any.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsoleReservation {
    pub id: String,
    pub match_id: String,
    pub kind: ReservationKind,
    pub status: ReservationStatus,
    pub matchzy_id: i64,
}

/// A booking covering right now (a practice night, a map-change hold).
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsoleHold {
    pub id: String,
    pub reason: Option<String>,
    pub tournament_id: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
}

/// Everything the console modal shows at once.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsoleSnapshotResponse {
    pub server_id: String,
    pub server_name: String,
    pub server_status: GameServerStatus,
    pub agent: ConsoleAgentInfo,
    /// MatchZy gamestate from the last heartbeat.
    pub gamestate: Option<String>,
    /// Parsed `status`; absent when nothing has been captured.
    pub status: Option<ConsoleStatus>,
    /// The `status` text the parse came from, addresses redacted.
    pub raw_status: Option<String>,
    /// When `status` was captured.
    pub status_at: Option<String>,
    /// Whether `status` was fetched just now rather than read from the store.
    pub live: bool,
    /// Why a requested live fetch fell back to stored data.
    pub live_error: Option<String>,
    pub reservation: Option<ConsoleReservation>,
    pub holds: Vec<ConsoleHold>,
}

/// One audited console command.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminServerCommandResponse {
    pub id: String,
    pub server_id: String,
    pub reservation_id: Option<String>,
    pub admin_user_id: String,
    pub admin_username: Option<String>,
    /// `raw`, `map_change`, or `action:<name>`.
    pub kind: String,
    pub command: String,
    pub output: Option<String>,
    pub ok: bool,
    pub force: bool,
    pub created_at: String,
}

/// Query for the console history.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct HistoryQuery {
    /// Rows to return, newest first (default 50, at most 200).
    pub limit: Option<i64>,
}

/// A map change: a catalogue entry or a free-form target.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct MapChangeRequest {
    /// Id of a map in the game's catalogue.
    pub map_id: Option<String>,
    /// A map name, a workshop id, or a workshop link.
    #[validate(length(max = 256, message = "custom must be at most 256 characters"))]
    pub custom: Option<String>,
    /// Cancel a live reservation first (participants are notified).
    #[serde(default)]
    pub force: bool,
}

/// What the map change resolved to.
#[derive(Debug, Serialize, ToSchema)]
pub struct MapTarget {
    pub map_id: Option<String>,
    pub display_name: Option<String>,
    /// What the server was told to load: an engine name or a workshop id.
    pub engine_name: String,
    pub workshop: bool,
}

/// Result of a map change.
#[derive(Debug, Serialize, ToSchema)]
pub struct MapChangeResponse {
    pub command: String,
    pub ok: bool,
    pub output: String,
    pub target: MapTarget,
    /// Match whose reservation `force` cancelled.
    pub cancelled_match_id: Option<String>,
    /// Until when the allocator keeps off the server.
    pub hold_until: Option<String>,
}

/// The console's action table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleAction {
    Pause,
    Unpause,
    ForceStart,
    /// Confirm-gated. Cancels the live reservation when there is one.
    EndMatch,
    RestartWarmup,
    KickBots,
    /// `say` a message; needs `args.message`.
    Broadcast,
    /// Confirm-gated; needs `args.userid`, takes `args.reason`.
    KickPlayer,
    /// Places a hard hold until `args.until` (default two hours).
    PracticeStart,
    /// Removes the practice hold.
    PracticeStop,
}

impl ConsoleAction {
    fn name(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::ForceStart => "force_start",
            Self::EndMatch => "end_match",
            Self::RestartWarmup => "restart_warmup",
            Self::KickBots => "kick_bots",
            Self::Broadcast => "broadcast",
            Self::KickPlayer => "kick_player",
            Self::PracticeStart => "practice_start",
            Self::PracticeStop => "practice_stop",
        }
    }

    fn needs_confirm(self) -> bool {
        matches!(self, Self::EndMatch | Self::KickPlayer)
    }
}

/// Arguments for the actions that take any.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ConsoleActionArgs {
    pub message: Option<String>,
    pub userid: Option<u32>,
    pub reason: Option<String>,
    /// End of a practice hold.
    pub until: Option<DateTime<Utc>>,
}

/// Run one action from the table.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ConsoleActionRequest {
    pub action: ConsoleAction,
    #[serde(default)]
    pub args: ConsoleActionArgs,
    /// Required for `end_match` and `kick_player`.
    #[serde(default)]
    pub confirm: bool,
}

/// Result of an action.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConsoleActionResponse {
    pub action: ConsoleAction,
    /// Console lines sent, in order.
    pub commands: Vec<String>,
    pub ok: bool,
    pub output: String,
    pub cancelled_match_id: Option<String>,
    pub hold_until: Option<String>,
}

/// Request to run a raw console command.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct SendCommandRequest {
    /// One console line, e.g. `css_pause`. No `;`, no control characters.
    #[validate(length(min = 1, max = 512, message = "command must be 1-512 characters"))]
    pub command: String,
}

/// Output of a console command.
#[derive(Debug, Serialize, ToSchema)]
pub struct SendCommandResponse {
    /// Whether the agent reported success.
    pub ok: bool,
    /// The server's reply (or the agent's error).
    pub output: String,
}

// =============================================================================
// Shared pieces
// =============================================================================

/// The live reservation on a server; a lookup failure reads as "none" and
/// is logged, since it only gates a warning here.
async fn live_reservation(state: &AppState, server_id: GameServerId) -> Option<ServerReservation> {
    match state
        .server_reservation_repo
        .find_live_by_server(server_id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(server_id = %server_id, error = %e, "live-reservation lookup failed");
            None
        }
    }
}

fn require_connected(state: &AppState, server_id: GameServerId) -> Result<(), ApiError> {
    if state.agent_manager.is_connected(server_id) {
        Ok(())
    } else {
        Err(ApiError::conflict("no agent connected for this server"))
    }
}

/// Send one console line and return `(ok, bounded output)`. An agent that
/// drops or times out mid-command answers `ok: false` with the reason, so
/// the caller still audits what may have run.
async fn exec(state: &AppState, server_id: GameServerId, command: &str) -> (bool, String) {
    match state
        .agent_manager
        .send_command(
            server_id,
            AgentCommand::Exec {
                command: command.to_string(),
            },
        )
        .await
    {
        Ok(outcome) => {
            let text = outcome.output.or(outcome.error).unwrap_or_default();
            (outcome.ok, console::sanitise_output(&text, LIVE_OUTPUT_MAX))
        }
        Err(e) => (false, e.to_string()),
    }
}

/// Run several lines in order; stops at the first failure.
async fn exec_all(
    state: &AppState,
    server_id: GameServerId,
    commands: &[String],
) -> (bool, String) {
    let mut output = String::new();
    for (i, command) in commands.iter().enumerate() {
        let (ok, text) = exec(state, server_id, command).await;
        if commands.len() > 1 {
            if i > 0 {
                output.push('\n');
            }
            output.push_str("> ");
            output.push_str(command);
            output.push('\n');
        }
        output.push_str(&text);
        if !ok {
            return (false, output);
        }
    }
    (true, output)
}

/// The audit row. A failed insert is logged, not surfaced: the command has
/// already run and the admin needs its output.
#[allow(clippy::too_many_arguments)]
async fn audit(
    state: &AppState,
    server_id: GameServerId,
    reservation: Option<&ServerReservation>,
    auth: &AuthenticatedUser,
    kind: String,
    command: String,
    output: &str,
    ok: bool,
    force: bool,
) {
    let row = CreateAdminServerCommand {
        server_id,
        reservation_id: reservation.map(|r| r.id),
        admin_user_id: auth.user_id,
        kind,
        command,
        output: Some(console::sanitise_output(output, AUDIT_OUTPUT_MAX)),
        ok,
        force,
    };
    if let Err(e) = state.admin_server_command_repo.insert(row).await {
        tracing::error!(server_id = %server_id, error = %e, "console audit row not written");
    }
}

/// Place an allocator hold. Failure is logged: the command still runs.
async fn place_hold(
    state: &AppState,
    server_id: GameServerId,
    created_by: UserId,
    reason: String,
    until: DateTime<Utc>,
) -> Option<ServerBooking> {
    match state
        .game_server_registry
        .create_booking(CreateServerBooking {
            id: ServerBookingId::new(),
            server_id,
            tournament_id: None,
            reason: Some(reason),
            starts_at: Utc::now(),
            ends_at: until,
            created_by,
        })
        .await
    {
        Ok(b) => Some(b),
        Err(e) => {
            tracing::warn!(server_id = %server_id, error = %e, "console hold not placed");
            None
        }
    }
}

/// Bookings covering now, plus the console's own future holds.
async fn current_holds(state: &AppState, server_id: GameServerId) -> Vec<ServerBooking> {
    let now = Utc::now();
    state
        .game_server_registry
        .list_bookings(server_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b.covers(now))
        .collect()
}

/// The game's map catalogue: the stored list, else the plugin's defaults —
/// the same rule the games endpoint applies.
async fn maps_for_game(state: &AppState, game_id: GameId) -> Result<Vec<MapInfo>, ApiError> {
    let game = state
        .game_repo
        .find_by_id(game_id.as_uuid())
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;
    let stored: Vec<MapInfo> = match game.available_maps.as_array() {
        Some(arr) if !arr.is_empty() => {
            serde_json::from_value(game.available_maps.clone()).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    if !stored.is_empty() {
        return Ok(stored);
    }
    Ok(state
        .plugin_manager
        .get(&game.plugin_id)
        .map(|p| p.available_maps())
        .unwrap_or_default())
}

/// The portal player behind a Steam ID64, memoised per snapshot.
async fn link_for(
    state: &AppState,
    cache: &mut HashMap<u64, Option<ConsolePlayerLink>>,
    id: u64,
) -> Option<ConsolePlayerLink> {
    if let Some(cached) = cache.get(&id) {
        return cached.clone();
    }
    let found = match i64::try_from(id) {
        Ok(signed) => state
            .player_service
            .find_by_steam_id_64(signed)
            .await
            .ok()
            .flatten(),
        Err(_) => None,
    }
    .map(|pl| ConsolePlayerLink {
        id: pl.id.to_string(),
        display_name: pl.display_name,
    });
    cache.insert(id, found.clone());
    found
}

/// Join `status` rows to portal players by Steam ID64.
async fn link_players(state: &AppState, status: &console::ServerStatus) -> Vec<ConsolePlayer> {
    let mut cache: HashMap<u64, Option<ConsolePlayerLink>> = HashMap::new();
    let mut out = Vec::with_capacity(status.players.len());
    for p in &status.players {
        let link = match p.steam_id64 {
            Some(id) => link_for(state, &mut cache, id).await,
            None => None,
        };
        out.push(ConsolePlayer {
            userid: p.userid,
            name: p.name.clone(),
            steam_id64: p.steam_id64.map(|id| id.to_string()),
            bot: p.bot,
            connected_secs: p.connected_secs,
            ping: p.ping,
            loss: p.loss,
            state: p.state.clone(),
            player: link,
        });
    }
    out
}

fn reservation_dto(r: &ServerReservation) -> ConsoleReservation {
    ConsoleReservation {
        id: r.id.to_string(),
        match_id: r.match_id.to_string(),
        kind: r.kind,
        status: r.status,
        matchzy_id: r.matchzy_id,
    }
}

fn hold_dto(b: &ServerBooking) -> ConsoleHold {
    ConsoleHold {
        id: b.id.to_string(),
        reason: b.reason.clone(),
        tournament_id: b.tournament_id.map(|t| t.to_string()),
        starts_at: b.starts_at.to_rfc3339(),
        ends_at: b.ends_at.to_rfc3339(),
    }
}

fn agent_info(server: &GameServer, connected: bool) -> ConsoleAgentInfo {
    ConsoleAgentInfo {
        connected,
        version: server.agent_version.clone(),
        heartbeat_at: server.last_heartbeat_at.map(|dt| dt.to_rfc3339()),
        rcon_ok: server
            .last_heartbeat_at
            .map(|_| server.status != GameServerStatus::Error),
        reports_status: server.last_status_at.is_some(),
    }
}

// =============================================================================
// Handlers
// =============================================================================

/// The console snapshot: agent, gamestate, `status` (stored or live), the
/// live reservation and any holds.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers/{server_id}/console",
    params(
        ("server_id" = String, Path, description = "Game server ID"),
        ConsoleQuery,
    ),
    responses(
        (status = 200, description = "Console snapshot", body = DataResponse<ConsoleSnapshotResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn get_console(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Query(query): Query<ConsoleQuery>,
) -> ApiResult<Json<DataResponse<ConsoleSnapshotResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let server = state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;
    let connected = state.agent_manager.is_connected(id);

    // Stored first; a live fetch replaces it only when it succeeds.
    let mut raw = server.last_status_output.clone();
    let mut status_at = server.last_status_at;
    let mut live = false;
    let mut live_error = None;
    if query.live {
        if connected {
            let (ok, output) = exec(&state, id, "status").await;
            if ok {
                raw = Some(console::redact_addresses(&output));
                status_at = Some(Utc::now());
                live = true;
            } else {
                live_error = Some(output);
            }
        } else {
            live_error = Some("no agent connected for this server".to_string());
        }
    }

    let status = match raw.as_deref() {
        Some(text) => {
            let parsed = console::parse_status(text);
            Some(ConsoleStatus {
                hostname: parsed.hostname.clone(),
                map: parsed.map.clone(),
                humans: parsed.humans,
                bots: parsed.bots,
                max_players: parsed.max_players,
                players: link_players(&state, &parsed).await,
            })
        }
        None => None,
    };

    let reservation = live_reservation(&state, id).await;
    let holds = current_holds(&state, id).await;

    Ok(Json(DataResponse::new(
        ConsoleSnapshotResponse {
            server_id: server.id.to_string(),
            server_name: server.name.clone(),
            server_status: server.status,
            agent: agent_info(&server, connected),
            gamestate: server.last_gamestate.map(|g| g.to_string()),
            status,
            raw_status: raw,
            status_at: status_at.map(|dt| dt.to_rfc3339()),
            live,
            live_error,
            reservation: reservation.as_ref().map(reservation_dto),
            holds: holds.iter().map(hold_dto).collect(),
        },
        request_id,
    )))
}

/// Console commands sent to this server, newest first.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers/{server_id}/console/history",
    params(
        ("server_id" = String, Path, description = "Game server ID"),
        HistoryQuery,
    ),
    responses(
        (status = 200, description = "Audit rows", body = DataResponse<Vec<AdminServerCommandResponse>>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn get_console_history(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<DataResponse<Vec<AdminServerCommandResponse>>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let rows = state
        .admin_server_command_repo
        .list_by_server(id, limit)
        .await
        .map_err(ApiError::from)?;

    let mut names: HashMap<UserId, Option<String>> = HashMap::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        let admin_username = if let Some(name) = names.get(&row.admin_user_id) {
            name.clone()
        } else {
            let name = state
                .user_service
                .get_user(row.admin_user_id)
                .await
                .ok()
                .map(|u| u.username);
            names.insert(row.admin_user_id, name.clone());
            name
        };
        out.push(command_dto(row, admin_username));
    }
    Ok(Json(DataResponse::new(out, request_id)))
}

fn command_dto(
    row: &AdminServerCommand,
    admin_username: Option<String>,
) -> AdminServerCommandResponse {
    AdminServerCommandResponse {
        id: row.id.to_string(),
        server_id: row.server_id.to_string(),
        reservation_id: row.reservation_id.map(|r| r.to_string()),
        admin_user_id: row.admin_user_id.to_string(),
        admin_username,
        kind: row.kind.clone(),
        command: row.command.clone(),
        output: row.output.clone(),
        ok: row.ok,
        force: row.force,
        created_at: row.created_at.to_rfc3339(),
    }
}

/// Change the map. Catalogue maps with a workshop id load through
/// `host_workshop_map`, everything else through `changelevel`. Refused
/// with 409 under a live reservation unless `force`, which cancels the
/// reservation first. Places a short allocator hold either way.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/console/map",
    params(("server_id" = String, Path, description = "Game server ID")),
    request_body = MapChangeRequest,
    responses(
        (status = 200, description = "Map change sent", body = DataResponse<MapChangeResponse>),
        (status = 400, description = "No such map or bad target", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 409, description = "Agent not connected, or a live reservation without force", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn change_map(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<MapChangeRequest>,
) -> ApiResult<Json<DataResponse<MapChangeResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let server = state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;

    // Resolve the target before touching anything.
    let (command, target) = match (body.map_id.as_deref(), body.custom.as_deref()) {
        (Some(map_id), _) => {
            let catalogue = maps_for_game(&state, server.game_id).await?;
            let map = catalogue
                .into_iter()
                .find(|m| m.id == map_id)
                .ok_or_else(|| {
                    ApiError::bad_request(format!("no map '{map_id}' in the catalogue"))
                })?;
            let command = console::map_change_command(&map).map_err(ApiError::bad_request)?;
            let workshop = map.external_id.is_some();
            let engine_name = if workshop {
                command
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string()
            } else {
                map.resolved_engine_name().to_string()
            };
            (
                command,
                MapTarget {
                    map_id: Some(map.id),
                    display_name: Some(map.display_name),
                    engine_name,
                    workshop,
                },
            )
        }
        (None, Some(custom)) => {
            let command = console::custom_map_command(custom).map_err(ApiError::bad_request)?;
            let workshop = command.starts_with("host_workshop_map");
            let engine_name = command
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            (
                command,
                MapTarget {
                    map_id: None,
                    display_name: None,
                    engine_name,
                    workshop,
                },
            )
        }
        (None, None) => {
            return Err(ApiError::bad_request("map_id or custom is required"));
        }
    };

    require_connected(&state, id)?;

    let reservation = live_reservation(&state, id).await;
    let cancelled_match_id = match &reservation {
        Some(r) if !body.force => {
            return Err(ApiError::conflict(format!(
                "server has a live reservation for match {}; pass force to cancel it",
                r.match_id
            )));
        }
        Some(r) => {
            game_server_flow::cancel_assignment(
                &state,
                r.match_id,
                &format!("map change by {}", auth.username),
            )
            .await
            .map_err(ApiError::from)?;
            Some(r.match_id.to_string())
        }
        None => None,
    };

    let hold_minutes = if target.workshop {
        WORKSHOP_HOLD_MINUTES
    } else {
        MAP_HOLD_MINUTES
    };
    let hold = place_hold(
        &state,
        id,
        auth.user_id,
        format!(
            "{MAP_HOLD_REASON_PREFIX} to {} by {}",
            target.engine_name, auth.username
        ),
        Utc::now() + Duration::minutes(hold_minutes),
    )
    .await;

    tracing::info!(
        admin = %auth.username, admin_user_id = %auth.user_id,
        server = %server.name, server_id = %id, command = %command, force = body.force,
        "console map change"
    );
    let (ok, output) = exec(&state, id, &command).await;
    audit(
        &state,
        id,
        reservation.as_ref(),
        &auth,
        "map_change".to_string(),
        command.clone(),
        &output,
        ok,
        body.force,
    )
    .await;

    Ok(Json(DataResponse::new(
        MapChangeResponse {
            command,
            ok,
            output,
            target,
            cancelled_match_id,
            hold_until: hold.map(|b| b.ends_at.to_rfc3339()),
        },
        request_id,
    )))
}

/// Run one action from the console's table.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/console/action",
    params(("server_id" = String, Path, description = "Game server ID")),
    request_body = ConsoleActionRequest,
    responses(
        (status = 200, description = "Action sent", body = DataResponse<ConsoleActionResponse>),
        (status = 400, description = "Missing argument or confirmation", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 409, description = "Agent not connected", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn run_action(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<ConsoleActionRequest>,
) -> ApiResult<Json<DataResponse<ConsoleActionResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let action = body.action;
    if action.needs_confirm() && !body.confirm {
        return Err(ApiError::bad_request(format!(
            "{} needs confirm: true",
            action.name()
        )));
    }
    let server = state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;

    // Build the plan before touching anything, so a bad argument costs
    // nothing.
    let mut commands: Vec<String> = Vec::new();
    let mut hold_until: Option<DateTime<Utc>> = None;
    match action {
        ConsoleAction::Pause => commands.push("css_forcepause".into()),
        ConsoleAction::Unpause => commands.push("css_forceunpause".into()),
        ConsoleAction::ForceStart => commands.push("css_start".into()),
        ConsoleAction::KickBots => commands.push("bot_kick".into()),
        ConsoleAction::EndMatch => {}
        ConsoleAction::RestartWarmup => {
            let match_loaded = server.last_gamestate.is_some_and(|g| !g.is_idle());
            if match_loaded {
                commands.push("css_restart".into());
            } else {
                commands.push("mp_warmup_start".into());
                commands.push("mp_restartgame 1".into());
            }
        }
        ConsoleAction::Broadcast => {
            let message = body
                .args
                .message
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .ok_or_else(|| ApiError::bad_request("broadcast needs args.message"))?;
            if message.chars().count() > 200 {
                return Err(ApiError::bad_request(
                    "message must be at most 200 characters",
                ));
            }
            let quoted = console::console_quote(message).map_err(ApiError::bad_request)?;
            commands.push(format!("say {quoted}"));
        }
        ConsoleAction::KickPlayer => {
            let userid = body
                .args
                .userid
                .ok_or_else(|| ApiError::bad_request("kick_player needs args.userid"))?;
            let reason = body
                .args
                .reason
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .unwrap_or("Kicked by an admin");
            let quoted = console::console_quote(reason).map_err(ApiError::bad_request)?;
            commands.push(format!("kickid {userid} {quoted}"));
        }
        ConsoleAction::PracticeStart => {
            let now = Utc::now();
            let until = body
                .args
                .until
                .unwrap_or(now + Duration::hours(PRACTICE_DEFAULT_HOURS));
            if until <= now {
                return Err(ApiError::bad_request("args.until must be in the future"));
            }
            if until > now + Duration::hours(PRACTICE_MAX_HOURS) {
                return Err(ApiError::bad_request(format!(
                    "a practice hold can last at most {PRACTICE_MAX_HOURS} hours"
                )));
            }
            hold_until = Some(until);
            commands.push("matchzy_kick_when_no_match_loaded false".into());
            commands.push("css_prac".into());
        }
        ConsoleAction::PracticeStop => {
            commands.push("css_exitprac".into());
            commands.push("matchzy_kick_when_no_match_loaded true".into());
        }
    }

    require_connected(&state, id)?;

    let reservation = live_reservation(&state, id).await;
    let mut cancelled_match_id = None;

    // Side effects that must land before the console lines.
    match action {
        ConsoleAction::EndMatch => match &reservation {
            Some(r) => {
                game_server_flow::cancel_assignment(
                    &state,
                    r.match_id,
                    &format!("ended from the console by {}", auth.username),
                )
                .await
                .map_err(ApiError::from)?;
                cancelled_match_id = Some(r.match_id.to_string());
            }
            None => commands.push("css_endmatch".into()),
        },
        ConsoleAction::PracticeStart => {
            if let Some(r) = &reservation {
                return Err(ApiError::conflict(format!(
                    "server has a live reservation for match {}; end it first",
                    r.match_id
                )));
            }
            if let Some(until) = hold_until {
                place_hold(
                    &state,
                    id,
                    auth.user_id,
                    format!("{PRACTICE_REASON_PREFIX} by {}", auth.username),
                    until,
                )
                .await;
            }
        }
        _ => {}
    }

    tracing::info!(
        admin = %auth.username, admin_user_id = %auth.user_id,
        server = %server.name, server_id = %id, action = action.name(),
        "console action"
    );
    let (ok, output) = if commands.is_empty() {
        (true, "reservation cancelled; match ended".to_string())
    } else {
        exec_all(&state, id, &commands).await
    };

    // Holds are released after the lines ran, so a refused `css_exitprac`
    // still leaves the server held for an admin to look at.
    if action == ConsoleAction::PracticeStop {
        for b in current_holds(&state, id).await {
            if b.reason
                .as_deref()
                .is_some_and(|r| r.starts_with(PRACTICE_REASON_PREFIX))
                && let Err(e) = state.game_server_registry.delete_booking(b.id).await
            {
                tracing::warn!(server_id = %id, booking_id = %b.id, error = %e,
                    "practice hold not released");
            }
        }
    }

    let audited = if commands.is_empty() {
        "cancel_assignment".to_string()
    } else {
        commands.join("\n")
    };
    audit(
        &state,
        id,
        reservation.as_ref(),
        &auth,
        format!("action:{}", action.name()),
        audited,
        &output,
        ok,
        body.confirm,
    )
    .await;

    Ok(Json(DataResponse::new(
        ConsoleActionResponse {
            action,
            commands,
            ok,
            output,
            cancelled_match_id,
            hold_until: hold_until.map(|dt| dt.to_rfc3339()),
        },
        request_id,
    )))
}

/// Run a raw console command on a server via its agent (the escape hatch).
///
/// One line per request; the portal's own controls (RCON password,
/// webhook and demo settings, process lifetime) and `exec`/`alias` are
/// refused here and again at the agent. Every invocation is audited with
/// the acting admin.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/command",
    params(("server_id" = String, Path, description = "Game server ID")),
    request_body = SendCommandRequest,
    responses(
        (status = 200, description = "Command output", body = DataResponse<SendCommandResponse>),
        (status = 400, description = "Command refused", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 409, description = "Agent not connected", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn send_command(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<SendCommandRequest>,
) -> ApiResult<Json<DataResponse<SendCommandResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    debug_assert!(u64::try_from(body.command.len()).is_ok_and(|n| n <= RAW_COMMAND_MAX));
    let command = body.command.trim().to_string();
    if let Some(reason) = console::exec_refusal(&command) {
        // The verb is enough for the log; the rest may be a secret.
        let verb = command.split_whitespace().next().unwrap_or_default();
        tracing::info!(
            admin = %auth.username, admin_user_id = %auth.user_id,
            server_id = %id, verb = %verb, reason = %reason,
            "console command refused"
        );
        return Err(ApiError::bad_request(reason));
    }
    let server = state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;
    require_connected(&state, id)?;

    tracing::info!(
        admin = %auth.username, admin_user_id = %auth.user_id,
        server = %server.name, server_id = %id, command = %command,
        "console raw command"
    );
    let reservation = live_reservation(&state, id).await;
    let (ok, output) = exec(&state, id, &command).await;
    audit(
        &state,
        id,
        reservation.as_ref(),
        &auth,
        "raw".to_string(),
        command,
        &output,
        ok,
        false,
    )
    .await;

    Ok(Json(DataResponse::new(
        SendCommandResponse { ok, output },
        request_id,
    )))
}
