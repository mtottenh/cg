//! Veto (map pick/ban) handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    CreateVetoSessionRequest, PerformVetoActionRequest, RecordCoinFlipRequest, SelectSideRequest,
};
use crate::dto::responses::{
    VetoActionResponse, VetoActionResultResponse, VetoSessionResponse, VetoSessionStateResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use crate::websocket::{LobbyBroadcast, VetoActionBroadcast, VetoCompleteBroadcast, VetoStateBroadcast};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::veto::VetoFormat;
use portal_domain::repositories::TournamentMatchRepository;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Parse a veto format ID string into a VetoFormat.
fn parse_veto_format(format_id: &str) -> ApiResult<VetoFormat> {
    match format_id {
        "bo1_veto" | "bo1_standard" => Ok(VetoFormat::bo1()),
        "bo3_veto" | "bo3_standard" => Ok(VetoFormat::bo3()),
        "bo5_veto" | "bo5_standard" => Ok(VetoFormat::bo5()),
        _ => Err(ApiError::bad_request(format!(
            "Unknown veto format: {format_id}. Valid formats: bo1_veto, bo3_veto, bo5_veto"
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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    ValidatedJson(req): ValidatedJson<CreateVetoSessionRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<VetoSessionResponse>>)> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    // Verify user is a match participant (via veto authorization) or tournament admin
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Match {match_id} not found")))?;

    let is_participant = if let Some(p1) = match_.participant1_registration_id {
        state
            .veto_authorization_service
            .can_perform_veto_action(p1, auth.user_id, auth.player_id)
            .await
            .is_ok()
    } else {
        false
    } || if let Some(p2) = match_.participant2_registration_id {
        state
            .veto_authorization_service
            .can_perform_veto_action(p2, auth.user_id, auth.player_id)
            .await
            .is_ok()
    } else {
        false
    };

    if !is_participant {
        perm_checker
            .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
            .await?;
    }

    let veto_format = parse_veto_format(&req.veto_format_id)?;

    let session = state
        .veto_service
        .create_session(
            match_id,
            &veto_format,
            req.map_pool.unwrap_or_default(),
            Some(req.timeout_seconds),
        )
        .await?;

    // Broadcast session creation to WebSocket lobby (if any connections exist)
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        let _ = lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
            session: VetoSessionResponse::from(session.clone()),
        }));
    }

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(VetoSessionResponse::from(session), request_id)),
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
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<VetoSessionStateResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let session_state = state.veto_service.get_session_state(match_id).await?;

    Ok(Json(DataResponse::new(
        VetoSessionStateResponse::from(session_state),
        request_id,
    )))
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
        (status = 404, description = "Veto session not found", body = ApiError),
        (status = 409, description = "Coin flip already recorded", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto"
)]
pub async fn record_coin_flip(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Json(req): Json<RecordCoinFlipRequest>,
) -> ApiResult<Json<DataResponse<VetoSessionResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

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
        let _ = lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
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
        (status = 404, description = "Veto session not found", body = ApiError),
        (status = 409, description = "Session already started", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto"
)]
pub async fn start_veto_session(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<VetoSessionResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    // Get session by match
    let session_state = state.veto_service.get_session_state(match_id).await?;

    let updated = state
        .veto_service
        .start_session(session_state.session.id)
        .await?;

    // Broadcast session start to WebSocket lobby
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        let _ = lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    ValidatedJson(req): ValidatedJson<PerformVetoActionRequest>,
) -> ApiResult<Json<DataResponse<VetoActionResultResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

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
        .perform_action(session_state.session.id, &req.map_id, current_team, auth.user_id)
        .await?;

    // Broadcast action to WebSocket lobby
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        if result.veto_complete {
            // Broadcast veto completion
            let _ = lobby.broadcast(LobbyBroadcast::VetoComplete(VetoCompleteBroadcast {
                session: VetoSessionResponse::from(result.session.clone()),
                selected_maps: result.session.selected_maps.clone(),
            }));
        } else {
            // Broadcast the action performed
            let _ = lobby.broadcast(LobbyBroadcast::VetoActionPerformed(VetoActionBroadcast {
                session: VetoSessionResponse::from(result.session.clone()),
                action: VetoActionResponse::from(result.action.clone()),
                is_complete: false,
            }));
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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    ValidatedJson(req): ValidatedJson<SelectSideRequest>,
) -> ApiResult<Json<DataResponse<VetoActionResponse>>> {
    use portal_domain::repositories::TournamentMatchRepository;

    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

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
        .ok_or_else(|| ApiError::not_found(format!("Match {} not found", match_id)))?;

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
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        if let Ok(new_session_state) = state.veto_service.get_session_state(match_id).await {
            let _ = lobby.broadcast(LobbyBroadcast::VetoActionPerformed(VetoActionBroadcast {
                session: VetoSessionResponse::from(new_session_state.session),
                action: VetoActionResponse::from(updated.clone()),
                is_complete: false,
            }));
        }
    }

    Ok(Json(DataResponse::new(
        VetoActionResponse::from(updated),
        request_id,
    )))
}
