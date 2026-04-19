//! Match-level status transition handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Owns the
//! six endpoints that move a single match through its status state
//! machine: status read / history, player check-in, schedule, forfeit,
//! and the admin override transition. The two mutation paths
//! (`match_check_in`, `admin_match_transition`) auto-bootstrap the veto
//! session when the match enters PickBan via [`super::auto_create_veto_session`].

use super::{auto_create_veto_session, get_request_id};
use crate::dto::common::DataResponse;
use crate::dto::requests::{
    AdminMatchTransitionRequest, ForfeitMatchRequest, MatchCheckInRequest, ScheduleMatchRequest,
};
use crate::dto::responses::{
    MatchStatusDetailsResponse, MatchStatusLogResponse, TournamentMatchResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use portal_core::types::TournamentMatchStatus;
use portal_core::{TournamentMatchId, TournamentRegistrationId};

/// Get match status details.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/status",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match status details", body = DataResponse<MatchStatusDetailsResponse>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_lifecycle"
)]
pub async fn get_match_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<MatchStatusDetailsResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let details = state
        .match_lifecycle_service
        .get_match_status(match_id)
        .await?;

    Ok(Json(DataResponse::new(
        MatchStatusDetailsResponse::from(details),
        request_id,
    )))
}

/// Get match status history (transition log).
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/status-history",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match status history", body = DataResponse<Vec<MatchStatusLogResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_lifecycle"
)]
pub async fn get_match_status_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<MatchStatusLogResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let history = state
        .match_lifecycle_service
        .get_status_history(match_id)
        .await?;

    let response: Vec<MatchStatusLogResponse> = history.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Check in for a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/check-in",
    request_body = MatchCheckInRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Check-in successful", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn match_check_in(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<MatchCheckInRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let registration_id: TournamentRegistrationId = req
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let match_ = state
        .match_lifecycle_service
        .check_in(match_id, registration_id, auth.user_id)
        .await?;

    // Auto-create veto session when match transitions to PickBan
    if match_.status == TournamentMatchStatus::PickBan && match_.veto_required {
        if let Err(e) = auto_create_veto_session(&state, &match_).await {
            tracing::warn!(
                match_id = %match_id,
                error = ?e,
                "Failed to auto-create veto session on pick_ban transition"
            );
        }
    }

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Schedule a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule",
    request_body = ScheduleMatchRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match scheduled", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn schedule_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<ScheduleMatchRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let match_ = state
        .match_lifecycle_service
        .schedule(match_id, req.scheduled_at, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Forfeit a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/forfeit",
    request_body = ForfeitMatchRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Forfeit recorded", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn forfeit_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<ForfeitMatchRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let registration_id: TournamentRegistrationId = req
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let match_ = state
        .match_lifecycle_service
        .forfeit(match_id, registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Admin force match status transition.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition",
    request_body = AdminMatchTransitionRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Transition successful", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn admin_match_transition(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminMatchTransitionRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let to_status: TournamentMatchStatus = req
        .to_status
        .parse()
        .map_err(|e| ApiError::bad_request(format!("Invalid status: {e}")))?;

    let match_ = state
        .match_lifecycle_service
        .admin_transition(match_id, to_status, auth.user_id, req.override_reason)
        .await?;

    // Auto-create veto session when admin transitions to PickBan
    if match_.status == TournamentMatchStatus::PickBan && match_.veto_required {
        if let Err(e) = auto_create_veto_session(&state, &match_).await {
            tracing::warn!(
                match_id = %match_id,
                error = ?e,
                "Failed to auto-create veto session on admin pick_ban transition"
            );
        }
    }

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}
