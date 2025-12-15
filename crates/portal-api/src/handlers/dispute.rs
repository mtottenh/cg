//! Dispute handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    AddDisputeMessageRequest, AdminDisputeMessageRequest, ListDisputesQuery, RaiseDisputeRequest,
    ResolveAdjustedRequest, ResolveDoubleDqRequest, ResolveOverturnRequest, ResolveRematchRequest,
    ResolveUpholdRequest,
};
use crate::dto::responses::{
    DisputeListResponse, DisputeMessageResponse, DisputeResolutionResultResponse, DisputeResponse,
    DisputeWithThreadResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{DisputeId, EvidenceId, ResultClaimId, TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::dispute::{AuthorType, DisputePriority, DisputeReason};

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

/// Raise a dispute against a match result.
///
/// Creates a new dispute that will require admin resolution.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/dispute",
    request_body = RaiseDisputeRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Dispute created", body = DataResponse<DisputeResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a match participant", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "Match already has active dispute", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn raise_dispute(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<RaiseDisputeRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<DisputeResponse>>)> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let registration_id: TournamentRegistrationId = req
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let reason: DisputeReason = req
        .reason
        .parse()
        .map_err(|e| ApiError::bad_request(format!("Invalid dispute reason: {e}")))?;

    let result_claim_id: Option<ResultClaimId> = req
        .result_claim_id
        .map(|id| id.parse())
        .transpose()
        .map_err(|_| ApiError::bad_request("Invalid result claim ID format"))?;

    let evidence_ids: Vec<EvidenceId> = req
        .evidence_ids
        .into_iter()
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))
        })
        .collect::<ApiResult<Vec<_>>>()?;

    let dispute = state
        .dispute_service
        .raise_dispute(
            match_id,
            result_claim_id,
            reason,
            req.description,
            evidence_ids,
            registration_id,
            auth.user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(DisputeResponse::from(dispute), request_id)),
    ))
}

/// Add a message to a dispute.
///
/// Adds a message to the dispute thread as a participant.
#[utoipa::path(
    post,
    path = "/v1/disputes/{dispute_id}/messages",
    request_body = AddDisputeMessageRequest,
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 201, description = "Message added", body = DataResponse<DisputeMessageResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized to add message", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn add_dispute_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
    ValidatedJson(req): ValidatedJson<AddDisputeMessageRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<DisputeMessageResponse>>)> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    let evidence_ids: Vec<EvidenceId> = req
        .evidence_ids
        .into_iter()
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))
        })
        .collect::<ApiResult<Vec<_>>>()?;

    let message = state
        .dispute_service
        .add_message(
            dispute_id,
            req.message,
            evidence_ids,
            auth.user_id,
            AuthorType::Participant,
            false, // Not internal
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(DisputeMessageResponse::from(message), request_id)),
    ))
}

/// Get a dispute with its message thread.
#[utoipa::path(
    get,
    path = "/v1/disputes/{dispute_id}",
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 200, description = "Dispute with thread", body = DataResponse<DisputeWithThreadResponse>),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    tag = "disputes"
)]
pub async fn get_dispute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
) -> ApiResult<Json<DataResponse<DisputeWithThreadResponse>>> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    // Regular users don't see internal messages
    let dispute_with_thread = state
        .dispute_service
        .get_dispute_with_thread(dispute_id, false)
        .await?;

    Ok(Json(DataResponse::new(
        DisputeWithThreadResponse::from(dispute_with_thread),
        request_id,
    )))
}

// =============================================================================
// ADMIN ENDPOINTS
// =============================================================================

/// Admin: List disputes.
///
/// Lists all disputes with optional filtering.
#[utoipa::path(
    get,
    path = "/v1/admin/disputes",
    params(ListDisputesQuery),
    responses(
        (status = 200, description = "List of disputes", body = DataResponse<DisputeListResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_list_disputes(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<ListDisputesQuery>,
) -> ApiResult<Json<DataResponse<DisputeListResponse>>> {
    let request_id = get_request_id(&headers);

    // TODO: Add admin permission check

    let priority = query
        .priority
        .as_deref()
        .map(|p| p.parse::<DisputePriority>())
        .transpose()
        .map_err(|_| ApiError::bad_request("Invalid priority value"))?;

    let limit = i64::from(query.page_size);
    let offset = i64::from((query.page - 1) * query.page_size);

    let (disputes, total) = state
        .dispute_service
        .get_pending_disputes(None, priority, limit, offset)
        .await?;

    let response = DisputeListResponse {
        disputes: disputes.into_iter().map(Into::into).collect(),
        total: total as u64,
        page: query.page,
        page_size: query.page_size,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Admin: Add a message to a dispute (can be internal).
#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{dispute_id}/messages",
    request_body = AdminDisputeMessageRequest,
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 201, description = "Message added", body = DataResponse<DisputeMessageResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_add_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
    ValidatedJson(req): ValidatedJson<AdminDisputeMessageRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<DisputeMessageResponse>>)> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    // TODO: Add admin permission check

    let evidence_ids: Vec<EvidenceId> = req
        .evidence_ids
        .into_iter()
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))
        })
        .collect::<ApiResult<Vec<_>>>()?;

    let message = state
        .dispute_service
        .add_message(
            dispute_id,
            req.message,
            evidence_ids,
            auth.user_id,
            AuthorType::Admin,
            req.is_internal,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(DisputeMessageResponse::from(message), request_id)),
    ))
}

/// Admin: Assign a dispute for review.
#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{dispute_id}/assign",
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 200, description = "Dispute assigned", body = DataResponse<DisputeResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_assign_dispute(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
) -> ApiResult<Json<DataResponse<DisputeResponse>>> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    // TODO: Add admin permission check

    let dispute = state
        .dispute_service
        .assign_for_review(dispute_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(DisputeResponse::from(dispute), request_id)))
}

/// Admin: Resolve dispute by upholding the original result.
#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{dispute_id}/resolve/uphold",
    request_body = ResolveUpholdRequest,
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 200, description = "Dispute resolved", body = DataResponse<DisputeResolutionResultResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_resolve_uphold(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
    ValidatedJson(req): ValidatedJson<ResolveUpholdRequest>,
) -> ApiResult<Json<DataResponse<DisputeResolutionResultResponse>>> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    // TODO: Add admin permission check

    let result = state
        .dispute_service
        .resolve_uphold(dispute_id, req.notes, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        DisputeResolutionResultResponse::from(result),
        request_id,
    )))
}

/// Admin: Resolve dispute by overturning the result.
#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{dispute_id}/resolve/overturn",
    request_body = ResolveOverturnRequest,
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 200, description = "Dispute resolved", body = DataResponse<DisputeResolutionResultResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_resolve_overturn(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
    ValidatedJson(req): ValidatedJson<ResolveOverturnRequest>,
) -> ApiResult<Json<DataResponse<DisputeResolutionResultResponse>>> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    let new_winner_registration_id: TournamentRegistrationId = req
        .new_winner_registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    // TODO: Add admin permission check

    let result = state
        .dispute_service
        .resolve_overturn(
            dispute_id,
            new_winner_registration_id,
            req.new_participant1_score,
            req.new_participant2_score,
            req.notes,
            auth.user_id,
        )
        .await?;

    Ok(Json(DataResponse::new(
        DisputeResolutionResultResponse::from(result),
        request_id,
    )))
}

/// Admin: Resolve dispute by ordering a rematch.
#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{dispute_id}/resolve/rematch",
    request_body = ResolveRematchRequest,
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 200, description = "Dispute resolved - rematch ordered", body = DataResponse<DisputeResolutionResultResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_resolve_rematch(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
    ValidatedJson(req): ValidatedJson<ResolveRematchRequest>,
) -> ApiResult<Json<DataResponse<DisputeResolutionResultResponse>>> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    // TODO: Add admin permission check

    let result = state
        .dispute_service
        .resolve_rematch(dispute_id, req.notes, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        DisputeResolutionResultResponse::from(result),
        request_id,
    )))
}

/// Admin: Resolve dispute by adjusting scores.
#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{dispute_id}/resolve/adjusted",
    request_body = ResolveAdjustedRequest,
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 200, description = "Dispute resolved - scores adjusted", body = DataResponse<DisputeResolutionResultResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_resolve_adjusted(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
    ValidatedJson(req): ValidatedJson<ResolveAdjustedRequest>,
) -> ApiResult<Json<DataResponse<DisputeResolutionResultResponse>>> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    // TODO: Add admin permission check

    let result = state
        .dispute_service
        .resolve_adjusted(
            dispute_id,
            req.new_participant1_score,
            req.new_participant2_score,
            req.notes,
            auth.user_id,
        )
        .await?;

    Ok(Json(DataResponse::new(
        DisputeResolutionResultResponse::from(result),
        request_id,
    )))
}

/// Admin: Resolve dispute by disqualifying both teams.
#[utoipa::path(
    post,
    path = "/v1/admin/disputes/{dispute_id}/resolve/double-dq",
    request_body = ResolveDoubleDqRequest,
    params(
        ("dispute_id" = String, Path, description = "Dispute ID")
    ),
    responses(
        (status = 200, description = "Dispute resolved - both teams disqualified", body = DataResponse<DisputeResolutionResultResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized", body = ApiError),
        (status = 404, description = "Dispute not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "disputes"
)]
pub async fn admin_resolve_double_dq(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(dispute_id): Path<String>,
    ValidatedJson(req): ValidatedJson<ResolveDoubleDqRequest>,
) -> ApiResult<Json<DataResponse<DisputeResolutionResultResponse>>> {
    let request_id = get_request_id(&headers);

    let dispute_id: DisputeId = dispute_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid dispute ID format"))?;

    // TODO: Add admin permission check

    let result = state
        .dispute_service
        .resolve_double_dq(dispute_id, req.notes, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        DisputeResolutionResultResponse::from(result),
        request_id,
    )))
}
