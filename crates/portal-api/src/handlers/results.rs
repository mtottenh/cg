//! Result submission handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{DisputeResultClaimRequest, SubmitResultClaimRequest};
use crate::dto::responses::{
    ResultClaimResponse, ResultClaimSubmissionResponse, ResultConfirmationResponse,
    ResultDisputeResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::ResultState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{DemoMatchLinkId, EvidenceId, ResultClaimId, TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::result_claim::GameResultInput;
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
// RESULT CLAIM ENDPOINTS
// =============================================================================

/// Submit a result claim for a match.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/result",
    request_body = SubmitResultClaimRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Result claim submitted", body = DataResponse<ResultClaimSubmissionResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a match participant", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "results"
)]
pub async fn submit_result(
    State(state): State<ResultState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<SubmitResultClaimRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<ResultClaimSubmissionResponse>>)> {
    let request_id = get_request_id(&headers);

    let claimed_winner_registration_id: TournamentRegistrationId = req
        .claimed_winner_registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid winner registration ID format"))?;

    // Convert game results
    let game_results: Vec<GameResultInput> = req
        .game_results
        .into_iter()
        .map(|g| {
            let evidence_ids: Result<Vec<EvidenceId>, _> = g
                .evidence_ids
                .into_iter()
                .map(|id| {
                    id.parse::<EvidenceId>()
                        .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))
                })
                .collect();
            let demo_link_id: Option<DemoMatchLinkId> = g
                .demo_link_id
                .map(|id| {
                    id.parse::<DemoMatchLinkId>()
                        .map_err(|_| ApiError::bad_request("Invalid demo link ID format"))
                })
                .transpose()?;
            Ok(GameResultInput {
                game_number: g.game_number,
                map_id: g.map_id,
                participant1_score: g.participant1_score,
                participant2_score: g.participant2_score,
                duration_seconds: g.duration_seconds,
                evidence_ids: evidence_ids?,
                demo_link_id,
            })
        })
        .collect::<ApiResult<Vec<_>>>()?;

    // Parse evidence IDs
    let evidence_ids: Vec<EvidenceId> = req
        .evidence_ids
        .into_iter()
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))
        })
        .collect::<ApiResult<Vec<_>>>()?;

    // Parse demo link IDs
    let demo_link_ids: Vec<DemoMatchLinkId> = req
        .demo_link_ids
        .into_iter()
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid demo link ID format"))
        })
        .collect::<ApiResult<Vec<_>>>()?;

    let claim = state
        .result_service
        .submit_claim(
            match_id,
            claimed_winner_registration_id,
            req.participant1_score,
            req.participant2_score,
            game_results,
            evidence_ids,
            demo_link_ids,
            req.notes,
            auth.user_id,
        )
        .await?;

    let auto_confirm_at = claim.auto_confirm_at.unwrap_or_else(chrono::Utc::now);

    let response = ResultClaimSubmissionResponse {
        claim: ResultClaimResponse::from(claim),
        superseded_previous: true, // Assume any previous claims were superseded
        auto_confirm_at,
    };

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(response, request_id)),
    ))
}

/// Get the current result claim for a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/result",
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Current result claim", body = DataResponse<ResultClaimResponse>),
        (status = 404, description = "No pending claim found", body = ApiError),
    ),
    tag = "results"
)]
pub async fn get_result_claim(
    State(state): State<ResultState>,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
) -> ApiResult<Json<DataResponse<ResultClaimResponse>>> {
    let request_id = get_request_id(&headers);

    let claim = state.result_service.get_pending_claim(match_id).await?;

    let claim = claim.ok_or_else(|| ApiError::not_found("No pending result claim for this match"))?;

    Ok(Json(DataResponse::new(
        ResultClaimResponse::from(claim),
        request_id,
    )))
}

/// List all result claims for a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/result/history",
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "List of result claims", body = DataResponse<Vec<ResultClaimResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "results"
)]
pub async fn list_result_claims(
    State(state): State<ResultState>,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
) -> ApiResult<Json<DataResponse<Vec<ResultClaimResponse>>>> {
    let request_id = get_request_id(&headers);

    let claims = state.result_service.get_claim_history(match_id).await?;

    let responses: Vec<ResultClaimResponse> = claims.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Confirm a result claim.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/result/{claim_id}/confirm",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("claim_id" = String, Path, description = "Claim ID")
    ),
    responses(
        (status = 200, description = "Result confirmed", body = DataResponse<ResultConfirmationResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Cannot confirm own claim", body = ApiError),
        (status = 404, description = "Claim not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "results"
)]
pub async fn confirm_result(
    State(state): State<ResultState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((match_id, claim_id)): Path<(TournamentMatchId, ResultClaimId)>,
) -> ApiResult<Json<DataResponse<ResultConfirmationResponse>>> {
    let request_id = get_request_id(&headers);

    // Confirm the claim (marks match completed with scores)
    let claim = state
        .result_service
        .confirm_claim(claim_id, auth.user_id)
        .await?;

    // Determine loser registration ID from match participants
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    let loser_registration_id = if match_.participant1_registration_id
        == Some(claim.claimed_winner_registration_id)
    {
        match_.participant2_registration_id
    } else {
        match_.participant1_registration_id
    }
    .ok_or_else(|| ApiError::internal("Loser participant not found on match"))?;

    // Determine winner/loser scores
    let (winner_score, loser_score) = if match_.participant1_registration_id
        == Some(claim.claimed_winner_registration_id)
    {
        (
            claim.claimed_participant1_score,
            claim.claimed_participant2_score,
        )
    } else {
        (
            claim.claimed_participant2_score,
            claim.claimed_participant1_score,
        )
    };

    // Build saga input
    let saga_input = MatchCompletionInput {
        match_id,
        winner_registration_id: claim.claimed_winner_registration_id,
        loser_registration_id,
        winner_score,
        loser_score,
        is_forfeit: false,
        saga_id: None,
        result_claim_id: Some(claim.id),
    };

    // Execute the match completion saga (may pause if review needed)
    let saga_result = state
        .match_completion_saga
        .execute_completion(saga_input)
        .await;

    match saga_result {
        Ok(result) if result.is_paused() => {
            // Review pending — return response indicating progression is paused
            let output = result.output.as_ref();
            let response = ResultConfirmationResponse {
                claim: ResultClaimResponse::from(claim),
                match_status: "completed".to_string(),
                bracket_advanced: false,
                review_pending: Some(true),
                review_id: output.and_then(|o| o.review_id.map(|id| id.to_string())),
            };
            Ok(Json(DataResponse::new(response, request_id)))
        }
        Ok(result) => {
            // Full completion with bracket progression
            let advanced = result
                .output
                .as_ref()
                .is_some_and(|o| o.winner_next_match_id.is_some());
            let response = ResultConfirmationResponse {
                claim: ResultClaimResponse::from(claim),
                match_status: "completed".to_string(),
                bracket_advanced: advanced,
                review_pending: None,
                review_id: None,
            };
            Ok(Json(DataResponse::new(response, request_id)))
        }
        Err(e) => {
            // Saga failed — log but still return success since the claim was confirmed
            warn!(
                match_id = %match_id,
                error = %e,
                "Match completion saga failed after result confirmation"
            );
            let response = ResultConfirmationResponse {
                claim: ResultClaimResponse::from(claim),
                match_status: "completed".to_string(),
                bracket_advanced: false,
                review_pending: None,
                review_id: None,
            };
            Ok(Json(DataResponse::new(response, request_id)))
        }
    }
}

/// Dispute a result claim.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/result/{claim_id}/dispute",
    request_body = DisputeResultClaimRequest,
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("claim_id" = String, Path, description = "Claim ID")
    ),
    responses(
        (status = 200, description = "Result disputed", body = DataResponse<ResultDisputeResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Cannot dispute own claim", body = ApiError),
        (status = 404, description = "Claim not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "results"
)]
pub async fn dispute_result(
    State(state): State<ResultState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((match_id, claim_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<DisputeResultClaimRequest>,
) -> ApiResult<Json<DataResponse<ResultDisputeResponse>>> {
    let request_id = get_request_id(&headers);

    let _match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let claim_id: ResultClaimId = claim_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid claim ID format"))?;

    let claim = state
        .result_service
        .dispute_claim(claim_id, auth.user_id, &req.reason)
        .await?;

    let response = ResultDisputeResponse {
        claim: ResultClaimResponse::from(claim),
        match_status: "disputed".to_string(),
        requires_admin: true,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}
