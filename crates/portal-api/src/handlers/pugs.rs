//! Pick-Up Game (PUG) handlers.
//!
//! The lobby endpoints operate on the `pugs` aggregate; `lock` and `spin`
//! delegate to `pug_flow`, which composes the tournament/veto/game-server
//! subsystems. Share-link semantics: `GET /pugs/code/{code}` is deliberately
//! unauthenticated (landing-page preview); joining requires a Steam-linked
//! account because MatchZy locks the server roster to SteamID64s.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use portal_core::types::{PugMapSelectionMode, PugStatus};
use portal_core::{GameId, MatchFormat, PlayerId, PugId, SideSelectionMode};
use portal_domain::entities::pug::{Pug, PugPlayer, PugPlayerAggregates, PugWheelEntry, PugWheelSpin};
use portal_domain::repositories::tournament::TournamentMatchRepository as _;

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, OptionalAuthenticatedUser, ValidatedJson};
use crate::state::AppState;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// DTOs
// =============================================================================

/// Create a PUG lobby.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreatePugRequest {
    /// Game ID.
    pub game_id: String,
    /// Series format: bo1, bo3 or bo5.
    pub match_format: MatchFormat,
    /// veto (pick/ban) or wheel (weighted random).
    pub map_selection_mode: PugMapSelectionMode,
    /// knife (default), coin_flip, or picker_choice (veto mode only).
    pub side_selection_mode: Option<SideSelectionMode>,
    /// Players per team; defaults to the game's configured team size.
    #[validate(range(min = 1, max = 16, message = "team_size must be 1-16"))]
    pub team_size: Option<i32>,
    /// Preferred server region (must match a registered server's region).
    #[validate(length(max = 32, message = "region must be at most 32 characters"))]
    pub region: Option<String>,
    /// Veto mode: custom map pool (subset of the game's catalog).
    pub map_pool: Option<Vec<String>>,
    /// Show this lobby in the public open-PUGs browser.
    #[serde(default)]
    pub listed: bool,
}

/// Assign a player to a team (or the bench).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SetTeamRequest {
    /// Player to move; omitted = yourself. Only the creator may move others.
    pub player_id: Option<String>,
    /// 1 or 2; null = bench.
    pub team: Option<i16>,
}

/// Toggle captain status (creator only).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SetCaptainRequest {
    pub player_id: String,
    pub is_captain: bool,
}

/// Kick a player (creator only).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct KickPlayerRequest {
    pub player_id: String,
}

/// Nominate a map for the wheel.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct WheelEntryRequest {
    #[validate(length(min = 1, max = 64, message = "map_id must be 1-64 characters"))]
    pub map_id: String,
}

/// Lock the lobby and start map selection.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct LockPugRequest {
    /// Allow uneven / short-handed teams.
    #[serde(default)]
    pub force: bool,
}

/// Viewer query for the lobby (share-link viewers pass the code).
#[derive(Debug, Default, Deserialize)]
pub struct PugViewQuery {
    pub code: Option<String>,
}

/// Query for the open-PUGs browser.
#[derive(Debug, Default, Deserialize)]
pub struct OpenPugsQuery {
    pub game_id: Option<String>,
}

/// A PUG lobby.
#[derive(Debug, Serialize, ToSchema)]
pub struct PugResponse {
    pub id: String,
    pub game_id: String,
    pub status: PugStatus,
    pub match_format: MatchFormat,
    pub map_selection_mode: PugMapSelectionMode,
    pub side_selection_mode: SideSelectionMode,
    pub team_size: i32,
    pub region: Option<String>,
    pub map_pool: Option<Vec<String>>,
    pub listed: bool,
    /// Invite code — only present for participants.
    pub join_code: Option<String>,
    pub tournament_id: Option<String>,
    pub match_id: Option<String>,
    pub winner_team: Option<i16>,
    pub team1_score: Option<i32>,
    pub team2_score: Option<i32>,
    pub completed_at: Option<String>,
    pub expires_at: String,
    pub created_at: String,
    /// creator | player — the viewer's relationship to this pug.
    pub my_role: Option<String>,
}

/// A player in a PUG lobby.
#[derive(Debug, Serialize, ToSchema)]
pub struct PugPlayerResponse {
    pub player_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    /// Players without a linked Steam ID cannot enter the game server.
    pub has_steam_id: bool,
    pub team: Option<i16>,
    pub is_captain: bool,
    pub joined_at: String,
}

/// A wheel nomination.
#[derive(Debug, Serialize, ToSchema)]
pub struct PugWheelEntryResponse {
    pub player_id: String,
    pub player_name: String,
    pub map_id: String,
}

/// A recorded wheel spin (replayable animation payload).
#[derive(Debug, Serialize, ToSchema)]
pub struct PugWheelSpinResponse {
    pub game_number: i32,
    /// `[{map_id, weight, nominated_by}]` snapshot at spin time.
    pub segments: serde_json::Value,
    pub winner_map_id: String,
    pub spin_seed: i64,
    pub spun_at: String,
}

/// Full lobby state.
#[derive(Debug, Serialize, ToSchema)]
pub struct PugDetailResponse {
    pub pug: PugResponse,
    pub players: Vec<PugPlayerResponse>,
    pub wheel_entries: Vec<PugWheelEntryResponse>,
    pub spins: Vec<PugWheelSpinResponse>,
    /// The viewer's registration in the materialized match (drives the veto
    /// lobby UI), when they are on a team.
    pub my_registration_id: Option<String>,
}

/// Unauthenticated share-link preview.
#[derive(Debug, Serialize, ToSchema)]
pub struct PugPreviewResponse {
    pub id: String,
    pub game_id: String,
    pub status: PugStatus,
    pub match_format: MatchFormat,
    pub map_selection_mode: PugMapSelectionMode,
    pub team_size: i32,
    pub region: Option<String>,
    pub players_count: i64,
    pub slots_total: i64,
}

/// Result of a wheel spin.
#[derive(Debug, Serialize, ToSchema)]
pub struct SpinResponse {
    pub game_number: i32,
    pub segments: serde_json::Value,
    pub winner_map_id: String,
    pub spin_seed: i64,
    pub duration_ms: u32,
    pub is_complete: bool,
    pub selected_maps: Vec<String>,
}

/// Rotated invite code.
#[derive(Debug, Serialize, ToSchema)]
pub struct JoinCodeResponse {
    pub join_code: String,
}

/// Career PUG stats — demo-derived, separate from tournament profiles.
#[derive(Debug, Serialize, ToSchema)]
pub struct PugStatsResponse {
    pub matches_played: i64,
    pub wins: i64,
    pub losses: i64,
    pub demos_counted: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub avg_adr: f64,
    pub avg_hs_percentage: f64,
}

impl From<PugPlayerAggregates> for PugStatsResponse {
    fn from(a: PugPlayerAggregates) -> Self {
        Self {
            matches_played: a.matches_played,
            wins: a.wins,
            losses: a.losses,
            demos_counted: a.demos_counted,
            kills: a.kills,
            deaths: a.deaths,
            assists: a.assists,
            avg_adr: a.avg_adr,
            avg_hs_percentage: a.avg_hs_percentage,
        }
    }
}

fn pug_response(pug: &Pug, viewer: Option<&AuthenticatedUser>, is_participant: bool) -> PugResponse {
    let my_role = viewer.and_then(|v| {
        if pug.created_by_user_id == v.user_id {
            Some("creator".to_string())
        } else if is_participant {
            Some("player".to_string())
        } else {
            None
        }
    });
    PugResponse {
        id: pug.id.to_string(),
        game_id: pug.game_id.to_string(),
        status: pug.status,
        match_format: pug.match_format,
        map_selection_mode: pug.map_selection_mode,
        side_selection_mode: pug.side_selection_mode,
        team_size: pug.team_size,
        region: pug.region.clone(),
        map_pool: pug.map_pool.clone(),
        listed: pug.listed,
        join_code: (is_participant || my_role.as_deref() == Some("creator"))
            .then(|| pug.join_code.clone()),
        tournament_id: pug.tournament_id.map(|id| id.to_string()),
        match_id: pug.match_id.map(|id| id.to_string()),
        winner_team: pug.winner_team,
        team1_score: pug.team1_score,
        team2_score: pug.team2_score,
        completed_at: pug.completed_at.map(|t| t.to_rfc3339()),
        expires_at: pug.expires_at.to_rfc3339(),
        created_at: pug.created_at.to_rfc3339(),
        my_role,
    }
}

fn player_response(p: &PugPlayer) -> PugPlayerResponse {
    PugPlayerResponse {
        player_id: p.player_id.to_string(),
        display_name: p.display_name.clone(),
        avatar_url: p.avatar_url.clone(),
        has_steam_id: p.has_steam_id,
        team: p.team,
        is_captain: p.is_captain,
        joined_at: p.joined_at.to_rfc3339(),
    }
}

fn entry_response(e: &PugWheelEntry) -> PugWheelEntryResponse {
    PugWheelEntryResponse {
        player_id: e.player_id.to_string(),
        player_name: e.player_name.clone(),
        map_id: e.map_id.clone(),
    }
}

fn spin_response(s: &PugWheelSpin) -> PugWheelSpinResponse {
    PugWheelSpinResponse {
        game_number: s.game_number,
        segments: s.entries.clone(),
        winner_map_id: s.winner_map_id.clone(),
        spin_seed: s.spin_seed,
        spun_at: s.spun_at.to_rfc3339(),
    }
}

fn parse_pug_id(raw: &str) -> ApiResult<PugId> {
    raw.parse::<uuid::Uuid>()
        .map(PugId::from_uuid)
        .map_err(|_| ApiError::bad_request("invalid pug id"))
}

fn parse_player_id(raw: &str) -> ApiResult<PlayerId> {
    raw.parse::<uuid::Uuid>()
        .map(PlayerId::from_uuid)
        .map_err(|_| ApiError::bad_request("invalid player id"))
}

/// Resolve the viewer's registration id in the materialized match: team 1
/// maps to participant 1 (the materializer creates them in that order).
async fn my_registration_id(
    state: &AppState,
    pug: &Pug,
    players: &[PugPlayer],
    viewer_player: Option<PlayerId>,
) -> Option<String> {
    let match_id = pug.match_id?;
    let viewer = viewer_player?;
    let team = players.iter().find(|p| p.player_id == viewer)?.team?;
    let match_ = state.tournament_match_repo.find_by_id(match_id).await.ok()??;
    let reg = if team == 1 {
        match_.participant1_registration_id
    } else {
        match_.participant2_registration_id
    };
    reg.map(|r| r.to_string())
}

/// Validate map ids against the game's catalog.
fn validate_maps_in_catalog(
    state: &AppState,
    game_row: &portal_db::entities::GameRow,
    maps: &[String],
) -> ApiResult<()> {
    let plugin = state.plugin_manager.get(&game_row.plugin_id);
    let catalog = crate::handlers::games::game_catalog_map_ids(game_row, &plugin);
    for map in maps {
        if !catalog.contains(map) {
            return Err(ApiError::bad_request(format!(
                "map \"{map}\" is not in this game's catalog"
            )));
        }
    }
    Ok(())
}

async fn build_detail(
    state: &AppState,
    pug: &Pug,
    viewer: Option<&AuthenticatedUser>,
) -> ApiResult<PugDetailResponse> {
    let players = state
        .pug_service
        .players(pug.id)
        .await
        .map_err(ApiError::from)?;
    let is_participant = viewer
        .is_some_and(|v| players.iter().any(|p| p.player_id == v.player_id));
    let entries = if pug.map_selection_mode == PugMapSelectionMode::Wheel {
        state
            .pug_service
            .wheel_entries(pug.id)
            .await
            .map_err(ApiError::from)?
    } else {
        Vec::new()
    };
    let spins = if pug.map_selection_mode == PugMapSelectionMode::Wheel && pug.is_materialized() {
        state
            .pug_service
            .repo()
            .list_spins(pug.id)
            .await
            .map_err(ApiError::from)?
    } else {
        Vec::new()
    };
    let my_reg =
        my_registration_id(state, pug, &players, viewer.map(|v| v.player_id)).await;

    Ok(PugDetailResponse {
        pug: pug_response(pug, viewer, is_participant),
        players: players.iter().map(player_response).collect(),
        wheel_entries: entries.iter().map(entry_response).collect(),
        spins: spins.iter().map(spin_response).collect(),
        my_registration_id: my_reg,
    })
}

// =============================================================================
// LOBBY ENDPOINTS
// =============================================================================

/// Create a PUG lobby.
#[utoipa::path(
    post,
    path = "/v1/pugs",
    request_body = CreatePugRequest,
    responses(
        (status = 201, description = "PUG created", body = DataResponse<PugDetailResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Too many active PUGs", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn create_pug(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<CreatePugRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<DataResponse<PugDetailResponse>>)> {
    let request_id = get_request_id(&headers).to_string();

    let game_uuid = req
        .game_id
        .parse::<uuid::Uuid>()
        .map_err(|_| ApiError::bad_request("invalid game id"))?;
    let game_row = state
        .game_repo
        .find_by_id(game_uuid)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("game not found"))?;

    if let Some(pool) = &req.map_pool {
        validate_maps_in_catalog(&state, &game_row, pool)?;
    }

    let team_size = req.team_size.unwrap_or(game_row.team_size_default);
    let side_mode = req.side_selection_mode.unwrap_or(SideSelectionMode::Knife);

    let pug = state
        .pug_service
        .create(
            GameId::from(game_uuid),
            auth.user_id,
            auth.player_id,
            req.match_format,
            req.map_selection_mode,
            side_mode,
            team_size,
            req.region.clone(),
            req.map_pool.clone(),
            req.listed,
        )
        .await
        .map_err(ApiError::from)?;

    let detail = build_detail(&state, &pug, Some(&auth)).await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(DataResponse::new(detail, &request_id)),
    ))
}

/// Get full lobby state. Participants always see it; share-link viewers
/// pass `?code=`; listed and finished pugs are public.
#[utoipa::path(
    get,
    path = "/v1/pugs/{pug_id}",
    params(
        ("pug_id" = String, Path, description = "PUG ID"),
        ("code" = Option<String>, Query, description = "Invite code (share-link viewers)")
    ),
    responses(
        (status = 200, description = "Lobby state", body = DataResponse<PugDetailResponse>),
        (status = 403, description = "Private lobby", body = ApiError),
        (status = 404, description = "Not found", body = ApiError),
    ),
    tag = "pugs"
)]
pub async fn get_pug(
    State(state): State<AppState>,
    auth: OptionalAuthenticatedUser,
    headers: HeaderMap,
    Path(pug_id): Path<String>,
    Query(query): Query<PugViewQuery>,
) -> ApiResult<Json<DataResponse<PugDetailResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let pug_id = parse_pug_id(&pug_id)?;
    let pug = state.pug_service.get(pug_id).await.map_err(ApiError::from)?;

    let viewer = auth.0.as_ref();
    state
        .pug_service
        .authorize_view(&pug, viewer.map(|v| v.player_id), query.code.as_deref())
        .await
        .map_err(ApiError::from)?;

    let detail = build_detail(&state, &pug, viewer).await?;
    Ok(Json(DataResponse::new(detail, &request_id)))
}

/// PUGs the caller created or joined (personal feed, newest first).
#[utoipa::path(
    get,
    path = "/v1/pugs/mine",
    responses(
        (status = 200, description = "My pugs", body = DataResponse<Vec<PugResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn my_pugs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<PugResponse>>>> {
    let request_id = get_request_id(&headers).to_string();
    let pugs = state
        .pug_service
        .repo()
        .list_by_participant(auth.player_id, 50)
        .await
        .map_err(ApiError::from)?;
    let items = pugs
        .iter()
        .map(|p| pug_response(p, Some(&auth), true))
        .collect();
    Ok(Json(DataResponse::new(items, &request_id)))
}

/// Publicly listed gathering lobbies (open-PUGs browser).
#[utoipa::path(
    get,
    path = "/v1/pugs/open",
    params(("game_id" = Option<String>, Query, description = "Filter by game")),
    responses(
        (status = 200, description = "Open listed pugs", body = DataResponse<Vec<PugResponse>>),
    ),
    tag = "pugs"
)]
pub async fn open_pugs(
    State(state): State<AppState>,
    auth: OptionalAuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<OpenPugsQuery>,
) -> ApiResult<Json<DataResponse<Vec<PugResponse>>>> {
    let request_id = get_request_id(&headers).to_string();
    let game_id = match &query.game_id {
        Some(raw) => Some(
            raw.parse::<uuid::Uuid>()
                .map(GameId::from_uuid)
                .map_err(|_| ApiError::bad_request("invalid game id"))?,
        ),
        None => None,
    };
    let pugs = state
        .pug_service
        .repo()
        .list_open_listed(game_id, 50)
        .await
        .map_err(ApiError::from)?;
    let items = pugs
        .iter()
        .map(|p| pug_response(p, auth.0.as_ref(), false))
        .collect();
    Ok(Json(DataResponse::new(items, &request_id)))
}

/// Recently completed PUGs (public results feed).
#[utoipa::path(
    get,
    path = "/v1/pugs/recent",
    responses(
        (status = 200, description = "Recent results", body = DataResponse<Vec<PugResponse>>),
    ),
    tag = "pugs"
)]
pub async fn recent_pugs(
    State(state): State<AppState>,
    auth: OptionalAuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<PugResponse>>>> {
    let request_id = get_request_id(&headers).to_string();
    let pugs = state
        .pug_service
        .repo()
        .list_recent_completed(50)
        .await
        .map_err(ApiError::from)?;
    let items = pugs
        .iter()
        .map(|p| pug_response(p, auth.0.as_ref(), false))
        .collect();
    Ok(Json(DataResponse::new(items, &request_id)))
}

// =============================================================================
// SHARE LINK
// =============================================================================

/// Unauthenticated share-link preview ("you've been invited...").
#[utoipa::path(
    get,
    path = "/v1/pugs/code/{code}",
    params(("code" = String, Path, description = "Invite code")),
    responses(
        (status = 200, description = "Lobby preview", body = DataResponse<PugPreviewResponse>),
        (status = 404, description = "Unknown code", body = ApiError),
    ),
    tag = "pugs"
)]
pub async fn preview_by_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> ApiResult<Json<DataResponse<PugPreviewResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let pug = state
        .pug_service
        .get_by_code(&code)
        .await
        .map_err(ApiError::from)?;
    let players_count = state
        .pug_service
        .repo()
        .count_players(pug.id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(DataResponse::new(
        PugPreviewResponse {
            id: pug.id.to_string(),
            game_id: pug.game_id.to_string(),
            status: pug.status,
            match_format: pug.match_format,
            map_selection_mode: pug.map_selection_mode,
            team_size: pug.team_size,
            region: pug.region.clone(),
            players_count,
            slots_total: i64::from(pug.team_size) * 2,
        },
        &request_id,
    )))
}

/// Join via invite code.
#[utoipa::path(
    post,
    path = "/v1/pugs/code/{code}/join",
    params(("code" = String, Path, description = "Invite code")),
    responses(
        (status = 200, description = "Joined", body = DataResponse<PugDetailResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Lobby full", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn join_by_code(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> ApiResult<Json<DataResponse<PugDetailResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let pug = state
        .pug_service
        .join_by_code(&code, auth.player_id)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug.id, "player_joined");
    let detail = build_detail(&state, &pug, Some(&auth)).await?;
    Ok(Json(DataResponse::new(detail, &request_id)))
}

/// Rotate the invite code (creator only) — old links die immediately.
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/code/rotate",
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses(
        (status = 200, description = "New code", body = DataResponse<JoinCodeResponse>),
        (status = 403, description = "Not the creator", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn rotate_code(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(pug_id): Path<String>,
) -> ApiResult<Json<DataResponse<JoinCodeResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let pug_id = parse_pug_id(&pug_id)?;
    let join_code = state
        .pug_service
        .rotate_code(pug_id, auth.user_id)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "code_rotated");
    Ok(Json(DataResponse::new(
        JoinCodeResponse { join_code },
        &request_id,
    )))
}

// =============================================================================
// MEMBERSHIP + TEAMS
// =============================================================================

/// Leave the lobby (gathering only; the creator cancels instead).
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/leave",
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Left")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn leave_pug(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    state
        .pug_service
        .leave(pug_id, auth.player_id, auth.user_id)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "player_left");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Kick a player (creator only).
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/kick",
    request_body = KickPlayerRequest,
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Kicked")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn kick_player(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
    ValidatedJson(req): ValidatedJson<KickPlayerRequest>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    let target = parse_player_id(&req.player_id)?;
    state
        .pug_service
        .kick(pug_id, auth.user_id, target, Some(auth.player_id))
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "player_kicked");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Join a team / move to the bench. Players move themselves; the creator
/// may move anyone.
#[utoipa::path(
    put,
    path = "/v1/pugs/{pug_id}/team",
    request_body = SetTeamRequest,
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Assigned")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn set_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
    ValidatedJson(req): ValidatedJson<SetTeamRequest>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    let target = match &req.player_id {
        Some(raw) => parse_player_id(raw)?,
        None => auth.player_id,
    };
    state
        .pug_service
        .set_team(pug_id, auth.user_id, auth.player_id, target, req.team)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "teams_changed");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Toggle a player's captain flag (creator only).
#[utoipa::path(
    put,
    path = "/v1/pugs/{pug_id}/captain",
    request_body = SetCaptainRequest,
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Updated")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn set_captain(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
    ValidatedJson(req): ValidatedJson<SetCaptainRequest>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    let target = parse_player_id(&req.player_id)?;
    state
        .pug_service
        .set_captain(pug_id, auth.user_id, target, req.is_captain)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "captain_changed");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Randomize balanced teams (creator only).
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/shuffle",
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Shuffled")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn shuffle_teams(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    state
        .pug_service
        .shuffle(pug_id, auth.user_id)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "teams_shuffled");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Mirror the rosters — team 1 ↔ team 2 (creator only).
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/swap-teams",
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Swapped")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn swap_teams(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    state
        .pug_service
        .swap_teams(pug_id, auth.user_id)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "teams_swapped");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// =============================================================================
// WHEEL + LOCK + SPIN + CANCEL
// =============================================================================

/// Draft the next bench player (captains draft). The team with fewer
/// players picks; only that team's captain (or the creator) may draft.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct DraftPickRequest {
    pub player_id: String,
}

/// Which team a draft pick landed on.
#[derive(Debug, Serialize, ToSchema)]
pub struct DraftPickResponse {
    pub player_id: String,
    pub team: i16,
}

/// Captains draft: assign the next bench player to the picking team.
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/draft",
    request_body = DraftPickRequest,
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses(
        (status = 200, description = "Drafted", body = DataResponse<DraftPickResponse>),
        (status = 403, description = "Not the picking captain", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn draft_pick(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(pug_id): Path<String>,
    ValidatedJson(req): ValidatedJson<DraftPickRequest>,
) -> ApiResult<Json<DataResponse<DraftPickResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let pug_id = parse_pug_id(&pug_id)?;
    let target = parse_player_id(&req.player_id)?;
    let team = state
        .pug_service
        .draft_pick(pug_id, auth.user_id, auth.player_id, target)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "player_drafted");
    Ok(Json(DataResponse::new(
        DraftPickResponse {
            player_id: req.player_id,
            team,
        },
        &request_id,
    )))
}

/// Nominate a map for the wheel (one per player, upserted).
#[utoipa::path(
    put,
    path = "/v1/pugs/{pug_id}/wheel-entry",
    request_body = WheelEntryRequest,
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Nominated")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn nominate_map(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
    ValidatedJson(req): ValidatedJson<WheelEntryRequest>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    let pug = state.pug_service.get(pug_id).await.map_err(ApiError::from)?;
    let game_row = state
        .game_repo
        .find_by_id(pug.game_id.as_uuid())
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("game not found"))?;
    validate_maps_in_catalog(&state, &game_row, std::slice::from_ref(&req.map_id))?;

    state
        .pug_service
        .nominate_map(pug_id, auth.player_id, &req.map_id)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "nomination_changed");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Lock the lobby: materialize the match and start map selection.
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/lock",
    request_body = LockPugRequest,
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses(
        (status = 200, description = "Locked", body = DataResponse<PugDetailResponse>),
        (status = 400, description = "Teams invalid / missing Steam links", body = ApiError),
        (status = 403, description = "Not the creator or a captain", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn lock_pug(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(pug_id): Path<String>,
    Json(req): Json<LockPugRequest>,
) -> ApiResult<Json<DataResponse<PugDetailResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let pug_id = parse_pug_id(&pug_id)?;
    let pug = crate::pug_flow::lock_pug(&state, pug_id, auth.user_id, auth.player_id, req.force)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "locked");
    let detail = build_detail(&state, &pug, Some(&auth)).await?;
    Ok(Json(DataResponse::new(detail, &request_id)))
}

/// Spin the wheel for the current map slot (creator or captain).
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/spin",
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses(
        (status = 200, description = "Spin result", body = DataResponse<SpinResponse>),
        (status = 400, description = "Not spin time", body = ApiError),
        (status = 403, description = "Not the creator or a captain", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn spin_wheel(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(pug_id): Path<String>,
) -> ApiResult<Json<DataResponse<SpinResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let pug_id = parse_pug_id(&pug_id)?;
    let outcome = crate::pug_flow::spin_wheel(&state, pug_id, auth.user_id, auth.player_id)
        .await
        .map_err(ApiError::from)?;
    let segments = serde_json::to_value(&outcome.draw.segments)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(DataResponse::new(
        SpinResponse {
            game_number: outcome.game_number,
            segments,
            winner_map_id: outcome.draw.winner_map_id,
            spin_seed: outcome.draw.spin_seed,
            duration_ms: crate::pug_flow::WHEEL_SPIN_DURATION_MS,
            is_complete: outcome.result.veto_complete,
            selected_maps: outcome.result.session.selected_maps.clone(),
        },
        &request_id,
    )))
}

/// Cancel the PUG (creator only; admins via tournament.manage).
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/cancel",
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses((status = 204, description = "Cancelled")),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn cancel_pug(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(pug_id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    let pug_id = parse_pug_id(&pug_id)?;
    let is_admin = state
        .permission_repo
        .user_has_permission(auth.user_id, "tournament.manage")
        .await
        .unwrap_or(false);
    crate::pug_flow::cancel_pug(&state, pug_id, auth.user_id, is_admin)
        .await
        .map_err(ApiError::from)?;
    state.pug_lobby_manager.notify_changed(pug_id, "cancelled");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Rematch: clone a finished PUG (settings + roster + teams) into a fresh
/// lobby with a new invite code (creator only).
#[utoipa::path(
    post,
    path = "/v1/pugs/{pug_id}/rematch",
    params(("pug_id" = String, Path, description = "PUG ID")),
    responses(
        (status = 201, description = "New lobby", body = DataResponse<PugDetailResponse>),
        (status = 400, description = "PUG not finished", body = ApiError),
        (status = 403, description = "Not the creator", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "pugs"
)]
pub async fn rematch_pug(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(pug_id): Path<String>,
) -> ApiResult<(axum::http::StatusCode, Json<DataResponse<PugDetailResponse>>)> {
    let request_id = get_request_id(&headers).to_string();
    let pug_id = parse_pug_id(&pug_id)?;
    let old = state.pug_service.get(pug_id).await.map_err(ApiError::from)?;

    if old.created_by_user_id != auth.user_id {
        return Err(ApiError::forbidden("Only the PUG creator can rematch"));
    }
    if !old.status.is_terminal() {
        return Err(ApiError::bad_request("Finish or cancel this PUG first"));
    }

    let players = state
        .pug_service
        .players(pug_id)
        .await
        .map_err(ApiError::from)?;

    let new_pug = state
        .pug_service
        .create(
            old.game_id,
            auth.user_id,
            auth.player_id,
            old.match_format,
            old.map_selection_mode,
            old.side_selection_mode,
            old.team_size,
            old.region.clone(),
            old.map_pool.clone(),
            old.listed,
        )
        .await
        .map_err(ApiError::from)?;

    // Same roster, same teams, same captains; anyone can leave.
    for p in players.iter().filter(|p| p.player_id != auth.player_id) {
        let repo = state.pug_service.repo();
        if let Err(e) = repo.add_player(new_pug.id, p.player_id).await {
            tracing::warn!(error = %e, "rematch: failed to re-add player");
            continue;
        }
        let _ = repo.set_player_team(new_pug.id, p.player_id, p.team).await;
        if p.is_captain {
            let _ = repo.set_player_captain(new_pug.id, p.player_id, true).await;
        }
    }
    // Creator keeps their old team side too.
    if let Some(me) = players.iter().find(|p| p.player_id == auth.player_id) {
        let _ = state
            .pug_service
            .repo()
            .set_player_team(new_pug.id, auth.player_id, me.team)
            .await;
    }

    state.pug_lobby_manager.broadcast(
        pug_id,
        crate::websocket::pug_lobby::PugLobbyBroadcast::RematchCreated {
            pug_id: new_pug.id.to_string(),
        },
    );

    let detail = build_detail(&state, &new_pug, Some(&auth)).await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(DataResponse::new(detail, &request_id)),
    ))
}

// =============================================================================
// STATS (separate PUG feed)
// =============================================================================

/// Career PUG stats for a player — demo-derived (`demos.category = 'pug'`),
/// entirely separate from tournament profiles.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/pug-stats",
    params(("player_id" = String, Path, description = "Player ID")),
    responses(
        (status = 200, description = "PUG aggregates", body = DataResponse<PugStatsResponse>),
    ),
    tag = "pugs"
)]
pub async fn player_pug_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(player_id): Path<String>,
) -> ApiResult<Json<DataResponse<PugStatsResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    let player_id = parse_player_id(&player_id)?;
    let aggregates = state
        .pug_service
        .repo()
        .player_aggregates(player_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(DataResponse::new(
        PugStatsResponse::from(aggregates),
        &request_id,
    )))
}
