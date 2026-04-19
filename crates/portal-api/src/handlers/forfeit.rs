//! Forfeit handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{AdminDisqualifyRequest, AdminForfeitMatchRequest, WithdrawFromTournamentRequest};
use crate::dto::responses::{DisqualificationResponse, ForfeitResponse, WithdrawalResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::ForfeitState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use portal_core::{TournamentId, TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::forfeit::{ForfeitTrigger, ForfeitType};

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// PARTICIPANT ENDPOINTS
// =============================================================================

/// Withdraw from a tournament.
///
/// Forfeits all remaining matches and marks the registration as withdrawn.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/withdraw",
    request_body = WithdrawFromTournamentRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID")
    ),
    responses(
        (status = 200, description = "Successfully withdrawn", body = DataResponse<WithdrawalResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized to withdraw this registration", body = ApiError),
        (status = 404, description = "Tournament or registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "forfeits"
)]
pub async fn withdraw_from_tournament(
    State(state): State<ForfeitState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((tournament_id, registration_id)): Path<(TournamentId, TournamentRegistrationId)>,
    ValidatedJson(req): ValidatedJson<WithdrawFromTournamentRequest>,
) -> ApiResult<Json<DataResponse<WithdrawalResponse>>> {
    let request_id = get_request_id(&headers);

    // Authorization: the forfeit service validates that user_id is associated with
    // this registration (owns it or is team captain). If not the owner, require
    // tournament participant management permission as a fallback.
    let results = state
        .forfeit_service
        .withdraw_from_tournament(tournament_id, registration_id, req.reason, auth.user_id)
        .await?;

    let response = WithdrawalResponse {
        registration_id: registration_id.to_string(),
        matches_forfeited: results.len(),
        forfeits: results.into_iter().map(Into::into).collect(),
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

// =============================================================================
// ADMIN ENDPOINTS
// =============================================================================

/// Admin: Forfeit a match.
///
/// Forces a forfeit for a specific match. Only accessible by tournament admins.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/forfeit",
    request_body = AdminForfeitMatchRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match forfeited", body = DataResponse<ForfeitResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "forfeits"
)]
pub async fn admin_forfeit_match(
    State(state): State<ForfeitState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminForfeitMatchRequest>,
) -> ApiResult<Json<DataResponse<ForfeitResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let forfeiting_registration_id: TournamentRegistrationId = req
        .forfeiting_registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let forfeit_type: ForfeitType = req
        .forfeit_type
        .parse()
        .map_err(|e| ApiError::bad_request(format!("Invalid forfeit type: {e}")))?;

    let result = state
        .forfeit_service
        .process_forfeit(
            match_id,
            forfeiting_registration_id,
            forfeit_type,
            Some(req.reason.clone()),
            ForfeitTrigger::Admin {
                user_id: auth.user_id,
                reason: req.reason,
            },
        )
        .await?;

    Ok(Json(DataResponse::new(
        ForfeitResponse::from(result),
        request_id,
    )))
}

/// Admin: Disqualify a registration.
///
/// Disqualifies a team/player and forfeits all their remaining matches.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/registrations/{registration_id}/disqualify",
    request_body = AdminDisqualifyRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID")
    ),
    responses(
        (status = 200, description = "Registration disqualified", body = DataResponse<DisqualificationResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "forfeits"
)]
pub async fn admin_disqualify(
    State(state): State<ForfeitState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((tournament_id, registration_id)): Path<(TournamentId, TournamentRegistrationId)>,
    ValidatedJson(req): ValidatedJson<AdminDisqualifyRequest>,
) -> ApiResult<Json<DataResponse<DisqualificationResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let results = state
        .forfeit_service
        .disqualify(tournament_id, registration_id, req.reason.clone(), auth.user_id)
        .await?;

    let response = DisqualificationResponse {
        registration_id: registration_id.to_string(),
        reason: req.reason,
        matches_forfeited: results.len(),
        forfeits: results.into_iter().map(Into::into).collect(),
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Admin: Process a double forfeit.
///
/// Both teams forfeit and the match is cancelled with no winner.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/double-forfeit",
    request_body = crate::dto::requests::AdminDoubleForfeitRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Double forfeit processed", body = DataResponse<ForfeitResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "forfeits"
)]
pub async fn admin_double_forfeit(
    State(state): State<ForfeitState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::AdminDoubleForfeitRequest>,
) -> ApiResult<Json<DataResponse<ForfeitResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let result = state
        .forfeit_service
        .process_double_forfeit(
            match_id,
            Some(req.reason.clone()),
            ForfeitTrigger::Admin {
                user_id: auth.user_id,
                reason: req.reason,
            },
        )
        .await?;

    Ok(Json(DataResponse::new(
        ForfeitResponse::from(result),
        request_id,
    )))
}
