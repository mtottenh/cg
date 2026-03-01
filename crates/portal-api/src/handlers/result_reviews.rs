//! Result review handlers.

use crate::dto::common::{DataResponse, PaginationParams};
use crate::dto::requests::AdminReviewDecisionRequest;
use crate::dto::responses::{
    AcknowledgmentResponse, ResultReviewListResponse, ResultReviewResponse,
    ResultReviewSummaryResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use portal_core::{ResultReviewId, TournamentMatchId, TournamentRegistrationId};

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

/// Get result review for a match.
///
/// Returns the result review if one exists for the match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/result-review",
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Result review found", body = DataResponse<ResultReviewResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "No review for match", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn get_result_review(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let review = state
        .result_review_service
        .get_for_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("No result review found for match {match_id}")))?;

    Ok(Json(DataResponse::new(
        ResultReviewResponse::from(review),
        request_id,
    )))
}

/// Acknowledge a result review.
///
/// Captain acknowledges the roster mismatch. When both captains acknowledge,
/// the review transitions to the acknowledged state.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/result-review/acknowledge",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("registration_id" = String, Query, description = "Registration ID of the captain")
    ),
    responses(
        (status = 200, description = "Acknowledgment recorded", body = DataResponse<AcknowledgmentResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a captain", body = ApiError),
        (status = 404, description = "No review for match", body = ApiError),
        (status = 409, description = "Already acknowledged", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn acknowledge_result_review(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Query(params): Query<AcknowledgeParams>,
) -> ApiResult<Json<DataResponse<AcknowledgmentResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let registration_id: TournamentRegistrationId = params
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    // Get the review first
    let review = state
        .result_review_service
        .get_for_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("No result review found for match {match_id}")))?;

    // Acknowledge
    let updated_review = state
        .result_review_service
        .acknowledge(review.id, registration_id, auth.user_id)
        .await?;

    let both_acknowledged = updated_review.both_captains_acknowledged();
    let message = if both_acknowledged {
        "Both captains have acknowledged the roster mismatch".to_string()
    } else {
        "Acknowledgment recorded. Waiting for the other captain.".to_string()
    };

    Ok(Json(DataResponse::new(
        AcknowledgmentResponse {
            review: ResultReviewResponse::from(updated_review),
            both_acknowledged,
            message,
        },
        request_id,
    )))
}

/// Query parameters for acknowledge endpoint.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct AcknowledgeParams {
    /// Registration ID of the captain acknowledging.
    pub registration_id: String,
}

// =============================================================================
// ADMIN ENDPOINTS
// =============================================================================

/// List pending result reviews for admin queue.
///
/// Returns all reviews pending admin action, ordered by creation date.
#[utoipa::path(
    get,
    path = "/v1/admin/result-reviews",
    params(PaginationParams),
    responses(
        (status = 200, description = "List of pending reviews", body = DataResponse<ResultReviewListResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin only", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn list_pending_reviews(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Query(params): Query<PaginationParams>,
) -> ApiResult<Json<DataResponse<ResultReviewListResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let limit = params.limit();
    let offset = params.offset();

    let (reviews, total) = state
        .result_review_service
        .list_pending_reviews(limit, offset)
        .await?;

    Ok(Json(DataResponse::new(
        ResultReviewListResponse {
            reviews: reviews.into_iter().map(ResultReviewSummaryResponse::from).collect(),
            total,
        },
        request_id,
    )))
}

/// Get a result review by ID.
///
/// Admin endpoint to get full details of a result review.
#[utoipa::path(
    get,
    path = "/v1/admin/result-reviews/{review_id}",
    params(
        ("review_id" = String, Path, description = "Review ID")
    ),
    responses(
        (status = 200, description = "Review details", body = DataResponse<ResultReviewResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin only", body = ApiError),
        (status = 404, description = "Review not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn get_result_review_by_id(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(review_id): Path<String>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let review_id: ResultReviewId = review_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid review ID format"))?;

    let review = state
        .result_review_service
        .get_by_id(review_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Result review {review_id} not found")))?;

    Ok(Json(DataResponse::new(
        ResultReviewResponse::from(review),
        request_id,
    )))
}

/// Approve a result review.
///
/// Admin approves the result despite validation mismatches.
#[utoipa::path(
    post,
    path = "/v1/admin/result-reviews/{review_id}/approve",
    request_body = AdminReviewDecisionRequest,
    params(
        ("review_id" = String, Path, description = "Review ID")
    ),
    responses(
        (status = 200, description = "Review approved", body = DataResponse<ResultReviewResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin only", body = ApiError),
        (status = 404, description = "Review not found", body = ApiError),
        (status = 409, description = "Review already resolved", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn approve_result_review(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(review_id): Path<String>,
    ValidatedJson(req): ValidatedJson<AdminReviewDecisionRequest>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let review_id: ResultReviewId = review_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid review ID format"))?;

    let review = state
        .result_review_service
        .approve(review_id, auth.user_id, req.notes)
        .await?;

    Ok(Json(DataResponse::new(
        ResultReviewResponse::from(review),
        request_id,
    )))
}

/// Reject a result review.
///
/// Admin rejects the result, requiring a new result submission.
#[utoipa::path(
    post,
    path = "/v1/admin/result-reviews/{review_id}/reject",
    request_body = AdminReviewDecisionRequest,
    params(
        ("review_id" = String, Path, description = "Review ID")
    ),
    responses(
        (status = 200, description = "Review rejected", body = DataResponse<ResultReviewResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin only", body = ApiError),
        (status = 404, description = "Review not found", body = ApiError),
        (status = 409, description = "Review already resolved", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn reject_result_review(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(review_id): Path<String>,
    ValidatedJson(req): ValidatedJson<AdminReviewDecisionRequest>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let review_id: ResultReviewId = review_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid review ID format"))?;

    let review = state
        .result_review_service
        .reject(review_id, auth.user_id, req.notes)
        .await?;

    Ok(Json(DataResponse::new(
        ResultReviewResponse::from(review),
        request_id,
    )))
}
