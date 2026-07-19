//! Progression API handlers.
//!
//! Handlers for viewing and managing bracket progression.

use crate::dto::common::DataResponse;
use crate::dto::requests::{ProcessProgressionRequest, ReapplyProgressionRequest};
use crate::dto::responses::ProgressionResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::ProgressionState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use portal_core::{TournamentMatchId, TournamentRegistrationId};

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// PROGRESSION QUERY ENDPOINTS
// =============================================================================

/// Get progression details for a match.
///
/// Returns information about winner advancement and loser routing.
/// Note: This returns progression info only if the match has been processed.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/progression",
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Progression details", body = DataResponse<ProgressionResponse>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "progression"
)]
pub async fn get_progression(
    State(_state): State<ProgressionState>,
    _auth: AuthenticatedUser,
    _headers: HeaderMap,
    Path(_match_id): Path<TournamentMatchId>,
) -> ApiResult<Json<DataResponse<ProgressionResponse>>> {
    // Progression info is typically computed/stored during match completion.
    // This endpoint would need match progression history tracking to show past progression.
    // For now, we return not implemented as progression is typically processed automatically.
    Err(ApiError::not_implemented(
        "Viewing progression history requires progression log storage (coming soon)",
    ))
}

// =============================================================================
// PROGRESSION ADMIN ENDPOINTS
// =============================================================================

/// Revert progression for a match.
///
/// Undoes winner advancement and loser routing. Used when a result is overturned.
/// Requires admin permissions.
#[utoipa::path(
    post,
    path = "/v1/admin/matches/{match_id}/progression/revert",
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Progression reverted"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin permission required", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "Cannot revert - subsequent matches affected", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "progression"
)]
pub async fn revert_progression(
    State(state): State<ProgressionState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
) -> ApiResult<Json<DataResponse<()>>> {
    let request_id = get_request_id(&headers);

    // Require admin permission
    perm_checker
        .require_permission(&auth, "tournament.admin")
        .await?;

    state
        .progression_service
        .revert_progression(match_id)
        .await?;

    Ok(Json(DataResponse::new((), request_id)))
}

/// Reapply progression with a different winner.
///
/// Reverts existing progression and reapplies with the new winner.
/// Requires admin permissions.
#[utoipa::path(
    post,
    path = "/v1/admin/matches/{match_id}/progression/reapply",
    request_body = ReapplyProgressionRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Progression reapplied", body = DataResponse<ProgressionResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin permission required", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "Cannot reapply - subsequent matches affected", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "progression"
)]
pub async fn reapply_progression(
    State(state): State<ProgressionState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<ReapplyProgressionRequest>,
) -> ApiResult<Json<DataResponse<ProgressionResponse>>> {
    let request_id = get_request_id(&headers);

    // Require admin permission
    perm_checker
        .require_permission(&auth, "tournament.admin")
        .await?;

    let new_winner: TournamentRegistrationId = req
        .new_winner_registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid winner registration ID format"))?;

    let result = state
        .progression_service
        .reapply_progression(match_id, new_winner)
        .await?;

    Ok(Json(DataResponse::new(
        ProgressionResponse::from(result),
        request_id,
    )))
}

/// Process match completion and advance bracket.
///
/// Manually triggers bracket progression with explicit winner and loser.
/// Used when automatic progression needs manual intervention.
/// Requires admin permissions.
#[utoipa::path(
    post,
    path = "/v1/admin/matches/{match_id}/progression/process",
    request_body = ProcessProgressionRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Progression processed", body = DataResponse<ProgressionResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin permission required", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "progression"
)]
pub async fn process_progression(
    State(state): State<ProgressionState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<ProcessProgressionRequest>,
) -> ApiResult<Json<DataResponse<ProgressionResponse>>> {
    let request_id = get_request_id(&headers);

    // Require admin permission
    perm_checker
        .require_permission(&auth, "tournament.admin")
        .await?;

    let winner_registration_id: TournamentRegistrationId = req
        .winner_registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid winner registration ID format"))?;

    let loser_registration_id: TournamentRegistrationId = req
        .loser_registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid loser registration ID format"))?;

    let result = state
        .progression_service
        .process_match_completion(match_id, winner_registration_id, loser_registration_id)
        .await?;

    Ok(Json(DataResponse::new(
        ProgressionResponse::from(result),
        request_id,
    )))
}
