//! Veto (map pick/ban) handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    CreateVetoSessionRequest, PerformVetoActionRequest, RecordCoinFlipRequest, SelectSideRequest,
};
use crate::dto::responses::{
    MapStatusResponse, VetoActionResponse, VetoActionResultResponse, VetoSessionResponse,
    VetoSessionStateResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::VetoState;
use crate::websocket::{
    LobbyBroadcast, VetoActionBroadcast, VetoCompleteBroadcast, VetoStateBroadcast,
};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::{TournamentMatchId, TournamentRegistrationId, VetoFormatConfig};
use portal_domain::repositories::TournamentMatchRepository;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Resolve a veto format ID to a format config.
///
/// Tries the plugin manager first (game-specific formats), then falls back
/// to the built-in standard formats.
pub(crate) fn resolve_veto_format(
    format_id: &str,
    state: &VetoState,
) -> ApiResult<VetoFormatConfig> {
    // Try plugin-provided formats
    for plugin in state.plugin_manager.list_plugins() {
        if let Some(tp) = plugin.as_tournament_plugin()
            && let Some(f) = tp.veto_formats().into_iter().find(|f| f.id == format_id)
        {
            return Ok(f);
        }
    }

    // Fall back to built-in formats
    match format_id {
        "bo1_veto" | "bo1_standard" => Ok(VetoFormatConfig::bo1()),
        "bo3_veto" | "bo3_standard" => Ok(VetoFormatConfig::bo3()),
        "bo5_veto" | "bo5_standard" => Ok(VetoFormatConfig::bo5()),
        _ => Err(ApiError::bad_request(format!(
            "Unknown veto format: {format_id}. Valid formats: bo1_standard, bo3_standard, bo5_standard"
        ))),
    }
}

// =============================================================================
// VETO SESSION ENDPOINTS
// =============================================================================

/// Create a veto session for a match.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/veto",
    request_body = CreateVetoSessionRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Veto session created", body = DataResponse<VetoSessionResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "Veto session already exists", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto"
)]
pub async fn create_veto_session(
    State(state): State<VetoState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<CreateVetoSessionRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<VetoSessionResponse>>)> {
    let request_id = get_request_id(&headers);

    // Verify user is a match participant (via veto authorization) or tournament admin
    let match_ = require_veto_participant_or_admin(&state, &perm_checker, &auth, match_id).await?;

    let veto_format = resolve_veto_format(&req.veto_format_id, &state)?;

    // Resolve side selection mode: request → tournament settings → plugin default
    let side_selection_mode = if let Some(ref mode_str) = req.side_selection_mode {
        mode_str.parse::<portal_core::SideSelectionMode>().map_err(|_| {
            ApiError::bad_request(format!(
                "Invalid side selection mode: {mode_str}. Valid modes: picker_choice, coin_flip, knife"
            ))
        })?
    } else {
        let tournament = state
            .tournament_service
            .get_tournament(match_.tournament_id)
            .await?;
        resolve_side_selection_mode(&tournament, &state.plugin_manager)
    };

    // Resolve map pool: explicit request → tournament pool → game default
    let map_pool = if let Some(pool) = req.map_pool {
        pool
    } else {
        use portal_domain::repositories::tournament::TournamentMapPoolRepository as _;
        // Try tournament-specific pool, then game default
        let tournament = state
            .tournament_service
            .get_tournament(match_.tournament_id)
            .await?;

        if let Ok(Some(pool)) = state
            .tournament_map_pool_repo
            .get_effective(match_.tournament_id, Some(match_.stage_id))
            .await
        {
            pool.maps
        } else if let Ok(Some(game)) = state
            .game_repo
            .find_by_id(tournament.game_id.as_uuid())
            .await
        {
            crate::handlers::games::extract_map_pool(&game)
        } else {
            vec![]
        }
    };

    let session = state
        .veto_service
        .create_session(
            match_id,
            &veto_format,
            map_pool,
            Some(req.timeout_seconds),
            side_selection_mode,
        )
        .await?;

    // Broadcast session creation to WebSocket lobby (if any connections exist)
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        let () = lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
            session: VetoSessionResponse::from(session.clone()),
        }));
    }

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            VetoSessionResponse::from(session),
            request_id,
        )),
    ))
}

/// Get veto session for a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/veto",
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Veto session state", body = DataResponse<VetoSessionStateResponse>),
        (status = 404, description = "Match or veto session not found", body = ApiError),
    ),
    tag = "veto"
)]
pub async fn get_veto_session(
    State(state): State<VetoState>,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
) -> ApiResult<Json<DataResponse<VetoSessionStateResponse>>> {
    let request_id = get_request_id(&headers);

    let session_state = state.veto_service.get_session_state(match_id).await?;

    let mut response = VetoSessionStateResponse::from(session_state);
    enrich_map_metadata(&state, match_id, &mut response.maps).await;

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Record coin flip result to determine first action.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/veto/coin-flip",
    request_body = RecordCoinFlipRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Coin flip recorded", body = DataResponse<VetoSessionResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a match participant or admin", body = ApiError),
        (status = 404, description = "Match or veto session not found", body = ApiError),
        (status = 409, description = "Coin flip already recorded", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto"
)]
pub async fn record_coin_flip(
    State(state): State<VetoState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    Json(req): Json<RecordCoinFlipRequest>,
) -> ApiResult<Json<DataResponse<VetoSessionResponse>>> {
    let request_id = get_request_id(&headers);

    // Same gate as `create_veto_session`: match participant or tournament admin.
    require_veto_participant_or_admin(&state, &perm_checker, &auth, match_id).await?;

    let winner_registration_id: TournamentRegistrationId = req
        .winner_registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    // Get session by match
    let session_state = state.veto_service.get_session_state(match_id).await?;

    let updated = state
        .veto_service
        .record_coin_flip(
            session_state.session.id,
            winner_registration_id,
            req.winner_goes_first,
        )
        .await?;

    // Broadcast coin flip result to WebSocket lobby
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        let () = lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
            session: VetoSessionResponse::from(updated.clone()),
        }));
    }

    Ok(Json(DataResponse::new(
        VetoSessionResponse::from(updated),
        request_id,
    )))
}

/// Start veto session (begin coin flip phase).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/veto/start",
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Veto session started", body = DataResponse<VetoSessionResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a match participant or admin", body = ApiError),
        (status = 404, description = "Match or veto session not found", body = ApiError),
        (status = 409, description = "Session already started", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto"
)]
pub async fn start_veto_session(
    State(state): State<VetoState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
) -> ApiResult<Json<DataResponse<VetoSessionResponse>>> {
    let request_id = get_request_id(&headers);

    // Same gate as `create_veto_session`: match participant or tournament admin.
    require_veto_participant_or_admin(&state, &perm_checker, &auth, match_id).await?;

    // Get session by match
    let session_state = state.veto_service.get_session_state(match_id).await?;

    let updated = state
        .veto_service
        .start_session(session_state.session.id)
        .await?;

    // Broadcast session start to WebSocket lobby
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        let () = lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
            session: VetoSessionResponse::from(updated.clone()),
        }));
    }

    Ok(Json(DataResponse::new(
        VetoSessionResponse::from(updated),
        request_id,
    )))
}

/// Perform a veto action (ban or pick a map).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/veto/action",
    request_body = PerformVetoActionRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Action performed", body = DataResponse<VetoActionResultResponse>),
        (status = 400, description = "Invalid action", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not your turn", body = ApiError),
        (status = 404, description = "Veto session not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto"
)]
pub async fn perform_veto_action(
    State(state): State<VetoState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<PerformVetoActionRequest>,
) -> ApiResult<Json<DataResponse<VetoActionResultResponse>>> {
    let request_id = get_request_id(&headers);

    // Get session by match
    let session_state = state.veto_service.get_session_state(match_id).await?;

    // Get current team turn
    let current_team = session_state
        .session
        .current_team_turn
        .ok_or_else(|| ApiError::bad_request("No team turn set - veto may not be in progress"))?;

    // Verify user is authorized to act for this team
    state
        .veto_authorization_service
        .can_perform_veto_action(current_team, auth.user_id, auth.player_id)
        .await?;

    let result = state
        .veto_service
        .perform_action(
            session_state.session.id,
            &req.map_id,
            current_team,
            auth.user_id,
        )
        .await?;

    // Broadcast action to WebSocket lobby
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        if result.veto_complete {
            // Broadcast veto completion
            let () = lobby.broadcast(LobbyBroadcast::VetoComplete(VetoCompleteBroadcast {
                session: VetoSessionResponse::from(result.session.clone()),
                selected_maps: result.session.selected_maps.clone(),
            }));
        } else {
            // Broadcast the action performed
            let () = lobby.broadcast(LobbyBroadcast::VetoActionPerformed(Box::new(
                VetoActionBroadcast {
                    session: VetoSessionResponse::from(result.session.clone()),
                    action: VetoActionResponse::from(result.action.clone()),
                    is_complete: false,
                },
            )));
        }
    }

    // Build response
    let response = VetoActionResultResponse {
        action: VetoActionResponse::from(result.action),
        session: VetoSessionResponse::from(result.session.clone()),
        is_complete: result.veto_complete,
        selected_maps: if result.veto_complete {
            Some(result.session.selected_maps)
        } else {
            None
        },
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Select side for a picked map.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/veto/side",
    request_body = SelectSideRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Side selected", body = DataResponse<VetoActionResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Cannot select side for this action", body = ApiError),
        (status = 404, description = "Veto action not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto"
)]
pub async fn select_side(
    State(state): State<VetoState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<SelectSideRequest>,
) -> ApiResult<Json<DataResponse<VetoActionResponse>>> {
    use portal_domain::repositories::TournamentMatchRepository;

    let request_id = get_request_id(&headers);

    // Get session by match
    let session_state = state.veto_service.get_session_state(match_id).await?;

    // Find the action to get the picker
    let action = session_state
        .actions
        .iter()
        .find(|a| a.action_number == req.action_number)
        .ok_or_else(|| ApiError::not_found(format!("Action {} not found", req.action_number)))?;

    let picker = action
        .performed_by_registration_id
        .ok_or_else(|| ApiError::bad_request("Action has no performer"))?;

    // Get the match to find both participants
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Match {match_id} not found")))?;

    // Determine opponent (who should select side)
    let opponent = if match_.participant1_registration_id == Some(picker) {
        match_.participant2_registration_id
    } else {
        match_.participant1_registration_id
    }
    .ok_or_else(|| ApiError::internal("Opponent not found in match"))?;

    // Verify user is authorized to act for the opponent
    state
        .veto_authorization_service
        .can_perform_veto_action(opponent, auth.user_id, auth.player_id)
        .await?;

    let updated = state
        .veto_service
        .select_side(
            session_state.session.id,
            req.action_number,
            &req.side,
            opponent,
            auth.user_id,
        )
        .await?;

    // Broadcast side selection to WebSocket lobby
    // Get the updated session state for the broadcast
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id)
        && let Ok(new_session_state) = state.veto_service.get_session_state(match_id).await
    {
        let () = lobby.broadcast(LobbyBroadcast::VetoActionPerformed(Box::new(
            VetoActionBroadcast {
                session: VetoSessionResponse::from(new_session_state.session),
                action: VetoActionResponse::from(updated.clone()),
                is_complete: false,
            },
        )));
    }

    Ok(Json(DataResponse::new(
        VetoActionResponse::from(updated),
        request_id,
    )))
}

// =============================================================================
// HELPERS
// =============================================================================

/// Require that the caller may manage the veto lifecycle for `match_id`.
///
/// Allowed when the caller can act for either participant registration
/// (captain / owner / delegate / individual player, via the veto
/// authorization service) or holds the tournament admin permission.
/// Returns the loaded match for further use.
async fn require_veto_participant_or_admin(
    state: &VetoState,
    perm_checker: &PermissionChecker,
    auth: &AuthenticatedUser,
    match_id: TournamentMatchId,
) -> ApiResult<portal_domain::entities::TournamentMatch> {
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Match {match_id} not found")))?;

    let mut is_participant = false;
    for reg_id in [
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ]
    .into_iter()
    .flatten()
    {
        if state
            .veto_authorization_service
            .can_perform_veto_action(reg_id, auth.user_id, auth.player_id)
            .await
            .is_ok()
        {
            is_participant = true;
            break;
        }
    }

    if !is_participant {
        perm_checker
            .require_permission(
                auth,
                portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY,
            )
            .await?;
    }

    Ok(match_)
}

/// Best-effort enrichment of map display names and image URLs from the game's map catalog.
async fn enrich_map_metadata(
    state: &VetoState,
    match_id: TournamentMatchId,
    maps: &mut [MapStatusResponse],
) {
    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        // match -> tournament -> game
        let match_ = state
            .tournament_match_repo
            .find_by_id(match_id)
            .await?
            .ok_or("match not found")?;

        let tournament = state
            .tournament_service
            .get_tournament(match_.tournament_id)
            .await?;

        let game = state
            .game_repo
            .find_by_id(tournament.game_id.as_uuid())
            .await?
            .ok_or("game not found")?;

        let plugin = state.plugin_manager.get(&game.plugin_id);
        let catalog = crate::handlers::games::load_available_maps(&game, &plugin);

        // Build lookup by map ID
        let lookup: std::collections::HashMap<&str, _> =
            catalog.iter().map(|m| (m.id.as_str(), m)).collect();

        for map in maps.iter_mut() {
            if let Some(info) = lookup.get(map.map_id.as_str()) {
                map.map_name = info.display_name.clone();
                map.image_url = info.image_url.clone();
            }
        }

        Ok(())
    }
    .await;

    // Best-effort: silently ignore errors — maps keep raw IDs
    if let Err(e) = result {
        tracing::warn!("Failed to enrich map metadata for match {match_id}: {e}");
    }
}

/// Resolve side selection mode from tournament settings or plugin default.
///
/// Now that both plugin and domain share the same `SideSelectionMode` from portal-core,
/// no manual conversion is needed.
pub(crate) fn resolve_side_selection_mode(
    tournament: &portal_domain::entities::tournament::Tournament,
    plugin_manager: &portal_plugins::PluginManager,
) -> portal_core::SideSelectionMode {
    use portal_core::SideSelectionMode;

    // Try tournament settings first
    if let Some(mode_str) = tournament
        .settings
        .get("side_selection_mode")
        .and_then(|v| v.as_str())
        && let Ok(mode) = mode_str.parse::<SideSelectionMode>()
    {
        return mode;
    }

    // Fall back to plugin default — no conversion needed, same type
    if let Some(plugin) = plugin_manager.get(&tournament.game_id.to_string())
        && let Some(tp) = plugin.as_tournament_plugin()
    {
        return tp.default_side_selection_mode();
    }
    SideSelectionMode::Knife
}
