//! Match scheduling and bracket standings handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Covers:
//!
//! * The proposal/counter-proposal workflow used for match scheduling
//!   (`propose_schedule`, `accept_schedule_proposal`,
//!   `reject_schedule_proposal`, `counter_propose`, `get_active_proposal`,
//!   `get_proposal_history`).
//! * The admin direct-schedule bypass (`admin_schedule_match`).
//! * The bracket-standings read (`get_bracket_standings`) and Swiss-round
//!   generation (`admin_generate_next_swiss_round`), which sit next to
//!   scheduling because they fire as the round advances.

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::{
    AcceptScheduleProposalRequest, AdminScheduleRequest, CounterProposeRequest,
    ProposeScheduleRequest, RejectScheduleProposalRequest,
};
use crate::dto::responses::{
    ScheduleProposalResponse, TournamentMatchResponse, TournamentStandingResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::TournamentState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{ScheduleProposalId, TournamentId, TournamentMatchId};
use portal_domain::entities::schedule_proposal::{
    AcceptProposalCommand, CounterProposeCommand, RejectProposalCommand,
};

/// Propose schedule times for a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose",
    request_body = ProposeScheduleRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Schedule proposal created", body = DataResponse<ScheduleProposalResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "Pending proposal already exists", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn propose_schedule(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<ProposeScheduleRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<ScheduleProposalResponse>>)> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposal = state
        .scheduling_service
        .propose_schedule(match_id, req.proposed_times, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            ScheduleProposalResponse::from(proposal),
            request_id,
        )),
    ))
}

/// Accept a schedule proposal.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept",
    request_body = AcceptScheduleProposalRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Proposal accepted, match scheduled", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn accept_schedule_proposal(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, _match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AcceptScheduleProposalRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let proposal_id: ScheduleProposalId = req
        .proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    let command = AcceptProposalCommand {
        proposal_id,
        selected_time: req.selected_time,
        accepted_by_user_id: auth.user_id,
    };

    let (_proposal, match_) = state.scheduling_service.accept_proposal(command).await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Reject a schedule proposal.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/reject",
    request_body = RejectScheduleProposalRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Proposal rejected", body = DataResponse<ScheduleProposalResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn reject_schedule_proposal(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, _match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<RejectScheduleProposalRequest>,
) -> ApiResult<Json<DataResponse<ScheduleProposalResponse>>> {
    let request_id = get_request_id(&headers);

    let proposal_id: ScheduleProposalId = req
        .proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    let command = RejectProposalCommand {
        proposal_id,
        rejected_by_user_id: auth.user_id,
    };

    let proposal = state.scheduling_service.reject_proposal(command).await?;

    Ok(Json(DataResponse::new(
        ScheduleProposalResponse::from(proposal),
        request_id,
    )))
}

/// Counter-propose with new schedule times.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/counter",
    request_body = CounterProposeRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Counter-proposal created", body = DataResponse<ScheduleProposalResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Original proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn counter_propose(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<CounterProposeRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<ScheduleProposalResponse>>)> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let original_proposal_id: ScheduleProposalId = req
        .original_proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    // Get the original proposal to find the user's registration
    let original_proposal = state
        .scheduling_service
        .get_active_proposal(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("No active proposal found"))?;

    // The counter-proposer must be the opponent, find their registration
    // For now, we'll need to look up the registration from the match
    let tournament_match = state
        .scheduling_service
        .get_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    // Determine which registration belongs to the counter-proposer
    let registration_id = if original_proposal.proposed_by_registration_id
        == tournament_match
            .participant1_registration_id
            .unwrap_or_default()
    {
        tournament_match
            .participant2_registration_id
            .ok_or_else(|| ApiError::bad_request("Opponent not assigned to match"))?
    } else {
        tournament_match
            .participant1_registration_id
            .ok_or_else(|| ApiError::bad_request("Opponent not assigned to match"))?
    };

    let command = CounterProposeCommand {
        original_proposal_id,
        match_id,
        proposed_by_registration_id: registration_id,
        proposed_by_user_id: auth.user_id,
        proposed_times: req.proposed_times,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(48),
    };

    let proposal = state.scheduling_service.counter_propose(command).await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            ScheduleProposalResponse::from(proposal),
            request_id,
        )),
    ))
}

/// Get active schedule proposal for a match.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Active proposal (or null if none)", body = DataResponse<Option<ScheduleProposalResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_scheduling"
)]
pub async fn get_active_proposal(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Option<ScheduleProposalResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposal = state
        .scheduling_service
        .get_active_proposal(match_id)
        .await?;

    Ok(Json(DataResponse::new(
        proposal.map(ScheduleProposalResponse::from),
        request_id,
    )))
}

/// Get schedule proposal history for a match.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/history",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Proposal history", body = DataResponse<Vec<ScheduleProposalResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_scheduling"
)]
pub async fn get_proposal_history(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<ScheduleProposalResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposals = state
        .scheduling_service
        .get_proposal_history(match_id)
        .await?;

    let response: Vec<ScheduleProposalResponse> =
        proposals.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Admin directly schedule a match (bypasses proposal workflow).
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule",
    request_body = AdminScheduleRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match scheduled", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn admin_schedule_match(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminScheduleRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let match_ = state
        .scheduling_service
        .admin_schedule(match_id, req.scheduled_at, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Get standings for a tournament bracket.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/brackets/{bracket_id}/standings",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("bracket_id" = String, Path, description = "Bracket ID")
    ),
    responses(
        (status = 200, description = "Bracket standings", body = DataResponse<Vec<TournamentStandingResponse>>),
        (status = 404, description = "Bracket not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_bracket_standings(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path((_tournament_id, bracket_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<TournamentStandingResponse>>>> {
    let request_id = get_request_id(&headers);

    let bracket_id: portal_core::TournamentBracketId = bracket_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid bracket ID format"))?;

    let standings = state.standings_service.get_standings(bracket_id).await?;

    let response: Vec<TournamentStandingResponse> =
        standings.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Generate the next Swiss round for a tournament.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/generate-next-round",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Next round generated", body = DataResponse<Vec<TournamentMatchResponse>>),
        (status = 400, description = "Not Swiss format or current round not complete", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn admin_generate_next_swiss_round(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentMatchResponse>>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let new_matches = state
        .tournament_service
        .generate_next_swiss_round(tournament_id)
        .await?;

    let response: Vec<TournamentMatchResponse> =
        new_matches.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}
