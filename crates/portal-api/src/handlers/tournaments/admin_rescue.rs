//! Admin match-rescue handlers.
//!
//! The endpoints an organiser uses to get a stuck match moving again from
//! the admin UI: reset it to `ready`, open or extend its check-in window,
//! restart its veto, re-run its completion, and read a diagnosis of what is
//! wrong with it. The logic lives in [`crate::admin_rescue`]; the game-server
//! side of the same toolbox is in `handlers::game_servers::match_server`.
//!
//! Every mutation requires `admin.tournaments.manage_any` and a reason, and
//! leaves a `match_status_log` row naming the admin.

use super::get_request_id;
use crate::admin_rescue::{self, CompletionRetryResponse, MatchDiagnosticsResponse};
use crate::dto::common::DataResponse;
use crate::dto::responses::TournamentMatchResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use portal_core::TournamentMatchId;
use portal_domain::repositories::match_rescue::RescueAudit;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Body of a rescue action that needs only a reason.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminRescueRequest {
    /// Why the admin is doing this. Recorded on the match's status log.
    #[validate(length(min = 1, max = 256))]
    pub reason: String,
}

/// Body for opening or extending a match's check-in window.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminCheckInWindowRequest {
    /// When the window closes. Must be in the future.
    pub deadline: DateTime<Utc>,
    /// Why the admin is doing this. Recorded on the match's status log.
    #[validate(length(min = 1, max = 256))]
    pub reason: String,
}

fn parse_match_id(raw: &str) -> Result<TournamentMatchId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))
}

fn audit(auth: &AuthenticatedUser, reason: &str) -> Result<RescueAudit, ApiError> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(ApiError::bad_request("A reason is required"));
    }
    Ok(RescueAudit {
        admin_id: auth.user_id,
        reason: reason.to_string(),
    })
}

async fn require_admin(
    perm_checker: &PermissionChecker,
    auth: &AuthenticatedUser,
) -> Result<(), ApiError> {
    perm_checker
        .require_permission(
            auth,
            portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY,
        )
        .await
}

/// Reset a match to `ready` (admin).
///
/// The universal way out of a stuck match: whatever state it reached —
/// scheduled with no time, an expired check-in, a no-show forfeit, a double
/// no-show cancellation, a veto or series that has to be replayed — it goes
/// back to `ready` with its time, check-ins, veto, result, pending claims,
/// open disputes and server reservation cleared, and standings recomputed.
/// A completed match is refused: correct its score or revert its progression
/// instead.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/reset",
    request_body = AdminRescueRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match reset to ready", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "The match cannot be reset from its current state", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "The match changed while the reset was running", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn admin_reset_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminRescueRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);
    require_admin(&perm_checker, &auth).await?;
    let match_id = parse_match_id(&match_id)?;
    let audit = audit(&auth, &req.reason)?;

    let match_ = admin_rescue::reset_match(&state, match_id, &audit).await?;
    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Open, extend or shorten a match's check-in window (admin).
///
/// On a match that is checking in, moves the deadline and keeps the
/// check-ins already made. On a `ready` or `scheduled` match, opens the
/// window now. A window that has already expired into a forfeit is reopened
/// by resetting the match first.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/check-in-window",
    request_body = AdminCheckInWindowRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Check-in window set", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Deadline not in the future, or the match is past check-in", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "The match changed while the request was running", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn admin_set_check_in_window(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminCheckInWindowRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);
    require_admin(&perm_checker, &auth).await?;
    let match_id = parse_match_id(&match_id)?;
    let audit = audit(&auth, &req.reason)?;

    let match_ = admin_rescue::set_check_in_window(&state, match_id, req.deadline, &audit).await?;
    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Restart a match's map veto (admin).
///
/// Discards the veto session and its picks, releases the game server (its
/// config was built from that veto), and starts a fresh veto with the match
/// in `pick_ban`.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/veto/reset",
    request_body = AdminRescueRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Veto restarted", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "No veto format, map pool too small, a result already reported, or wrong match status", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "The match changed while the request was running", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn admin_reset_veto(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminRescueRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);
    require_admin(&perm_checker, &auth).await?;
    let match_id = parse_match_id(&match_id)?;
    let audit = audit(&auth, &req.reason)?;

    let match_ = admin_rescue::restart_veto(&state, match_id, &audit).await?;
    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Re-run a decided match's completion (admin).
///
/// For a completed or forfeited match whose completion saga failed, paused
/// or never ran: advances the winner if that has not happened, recomputes
/// the standings and moves the stage on if it is finished. Idempotent.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/completion/retry",
    request_body = AdminRescueRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Completion re-run", body = DataResponse<CompletionRetryResponse>),
        (status = 400, description = "The match is not decided", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn admin_retry_completion(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminRescueRequest>,
) -> ApiResult<Json<DataResponse<CompletionRetryResponse>>> {
    let request_id = get_request_id(&headers);
    require_admin(&perm_checker, &auth).await?;
    let match_id = parse_match_id(&match_id)?;
    let audit = audit(&auth, &req.reason)?;

    let outcome = admin_rescue::retry_completion(&state, match_id, &audit).await?;
    Ok(Json(DataResponse::new(outcome, request_id)))
}

/// Diagnose a match (admin, read-only).
///
/// One consistent read of the match and everything hanging off it — veto,
/// server reservation, pending proposal and claim, open dispute, completion
/// saga — plus a list of what is wrong and which rescue action fixes it.
///
/// `problems[].code` is a closed list: `scheduled_without_time`,
/// `checkin_expired`, `both_checked_in_not_advanced`, `veto_missing`,
/// `veto_stalled`, `cancelled_by_no_show`, `forfeit_by_no_show`,
/// `pick_ban_veto_complete_no_server`, `reservation_failed`,
/// `reservation_stuck`, `reservation_live_match_terminal`, `roster_invalid`,
/// `saga_failed`, `disputed_flag_stale`.
#[utoipa::path(
    get,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/diagnostics",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match diagnostics", body = DataResponse<MatchDiagnosticsResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn admin_match_diagnostics(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<MatchDiagnosticsResponse>>> {
    let request_id = get_request_id(&headers);
    require_admin(&perm_checker, &auth).await?;
    let match_id = parse_match_id(&match_id)?;

    let diagnostics = admin_rescue::diagnostics(&state, match_id).await?;
    Ok(Json(DataResponse::new(diagnostics, request_id)))
}
