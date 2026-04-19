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
use portal_domain::repositories::tournament::TournamentMatchRepository;
use portal_domain::services::tournament::MatchCompletionInput;
use tracing::warn;

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
    Path(match_id): Path<TournamentMatchId>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

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
    Path(match_id): Path<TournamentMatchId>,
    Query(params): Query<AcknowledgeParams>,
) -> ApiResult<Json<DataResponse<AcknowledgmentResponse>>> {
    let request_id = get_request_id(&headers);

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

    // If both captains acknowledged and the review only has roster issues
    // (no score/winner mismatch), resume the saga for bracket progression
    if both_acknowledged && updated_review.is_roster_only() {
        if let Err(e) = resume_saga_after_review(&state, match_id).await {
            warn!(
                match_id = %match_id,
                error = %e,
                "Failed to resume saga after both captains acknowledged"
            );
        }
    }

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
    Path(review_id): Path<ResultReviewId>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

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
    Path(review_id): Path<ResultReviewId>,
    ValidatedJson(req): ValidatedJson<AdminReviewDecisionRequest>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let review = state
        .result_review_service
        .approve(review_id, auth.user_id, req.notes)
        .await?;

    // Resume the match completion saga for bracket progression
    if let Err(e) = resume_saga_after_review(&state, review.match_id).await {
        warn!(
            review_id = %review_id,
            match_id = %review.match_id,
            error = %e,
            "Failed to resume saga after review approval"
        );
    }

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
    Path(review_id): Path<ResultReviewId>,
    ValidatedJson(req): ValidatedJson<AdminReviewDecisionRequest>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let review = state
        .result_review_service
        .reject(review_id, auth.user_id, req.notes)
        .await?;

    // Revert the match back to in_progress so a new result can be submitted.
    // This bypasses the normal state machine since Completed -> InProgress
    // is not a standard transition.
    if let Err(e) = state
        .tournament_match_repo
        .update_status(
            review.match_id,
            portal_core::types::TournamentMatchStatus::InProgress,
        )
        .await
    {
        warn!(
            review_id = %review_id,
            match_id = %review.match_id,
            error = %e,
            "Failed to revert match status after review rejection"
        );
    }

    Ok(Json(DataResponse::new(
        ResultReviewResponse::from(review),
        request_id,
    )))
}

// =============================================================================
// HELPERS
// =============================================================================

/// Resume the match completion saga after a review has been resolved.
///
/// Builds the `MatchCompletionInput` from the completed match state and calls
/// `continue_after_review` to run the remaining progression steps.
async fn resume_saga_after_review(
    state: &AppState,
    match_id: TournamentMatchId,
) -> Result<(), portal_core::DomainError> {
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await?
        .ok_or_else(|| {
            portal_core::DomainError::TournamentMatchNotFound(match_id)
        })?;

    let winner_registration_id = match_.winner_registration_id.ok_or_else(|| {
        portal_core::DomainError::InvalidState("Match has no winner set".to_string())
    })?;

    let loser_registration_id =
        if match_.participant1_registration_id == Some(winner_registration_id) {
            match_.participant2_registration_id
        } else {
            match_.participant1_registration_id
        }
        .ok_or_else(|| {
            portal_core::DomainError::InvalidState("Match has no loser participant".to_string())
        })?;

    let (winner_score, loser_score) =
        if match_.participant1_registration_id == Some(winner_registration_id) {
            (match_.participant1_score, match_.participant2_score)
        } else {
            (match_.participant2_score, match_.participant1_score)
        };

    let saga_input = MatchCompletionInput {
        match_id,
        winner_registration_id,
        loser_registration_id,
        winner_score,
        loser_score,
        is_forfeit: false,
        saga_id: None,
        result_claim_id: None,
    };

    state
        .match_completion_saga
        .continue_after_review(match_id, saga_input)
        .await?;

    Ok(())
}
