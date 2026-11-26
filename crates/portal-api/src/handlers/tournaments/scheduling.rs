//! Match scheduling and bracket standings handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Covers:
//!
//! * The proposal/counter-proposal workflow used for match scheduling
//!   (`propose_schedule`, `accept_schedule_proposal`,
//!   `reject_schedule_proposal`, `cancel_schedule_proposal`,
//!   `counter_propose`, `get_active_proposal`, `get_proposal_history`).
//! * The admin direct-schedule bypass (`admin_schedule_match`).
//! * The bracket-standings read (`get_bracket_standings`) and Swiss-round
//!   generation (`admin_generate_next_swiss_round`), which sit next to
//!   scheduling because they fire as the round advances.

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::{
    AcceptScheduleProposalRequest, AdminScheduleRequest, CancelScheduleProposalRequest,
    CounterProposeRequest, ProposeScheduleRequest, RejectScheduleProposalRequest,
};
use crate::dto::responses::{
    ScheduleProposalResponse, TournamentMatchResponse, TournamentStandingResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::TournamentState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::{ScheduleProposalId, TournamentId, TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::schedule_proposal::{
    AcceptProposalCommand, CancelProposalCommand, CounterProposeCommand, RejectProposalCommand,
};
use portal_domain::services::tournament::RegistrationActor;

/// The registrations in a match the caller may *schedule* for.
///
/// Agreeing a time commits the whole team to turning up, and missing the
/// check-in window forfeits the match — so scheduling is bound to the same
/// people who can check the team in: its captain, its owner, or a roster
/// member the captain delegated to (and, for an individual registration,
/// the player). Any roster member used to be able to propose and accept,
/// which let a member agree a time the captain could not make. The rule is
/// the one veto and check-in already use
/// (`VetoAuthorizationService::can_act_for_registration`), so a delegate
/// granted for one is a delegate for all three.
///
/// `403` when the caller schedules for neither side.
async fn scheduling_registrations(
    state: &TournamentState,
    auth: &AuthenticatedUser,
    match_id: TournamentMatchId,
) -> Result<Vec<TournamentRegistrationId>, ApiError> {
    let tournament_match = state
        .scheduling_service
        .get_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    let mut acting_for = Vec::with_capacity(2);
    for registration_id in [
        tournament_match.participant1_registration_id,
        tournament_match.participant2_registration_id,
    ]
    .into_iter()
    .flatten()
    {
        if state
            .veto_authorization_service
            .can_act_for_registration(registration_id, auth.user_id, auth.player_id)
            .await
            .is_ok()
        {
            acting_for.push(registration_id);
        }
    }

    if acting_for.is_empty() {
        return Err(ApiError::forbidden(
            "Only the team captain or a delegate can schedule matches",
        ));
    }
    Ok(acting_for)
}

/// Require that `proposal_id` belongs to the match addressed in the path.
///
/// Accept and reject carry only a proposal id in the body; the scheduling
/// authorization above is resolved from the path's match. Without this the
/// two could be about different matches.
async fn require_proposal_on_match(
    state: &TournamentState,
    match_id: TournamentMatchId,
    proposal_id: ScheduleProposalId,
) -> Result<(), ApiError> {
    let on_match = state
        .scheduling_service
        .get_proposal_history(match_id)
        .await?
        .iter()
        .any(|p| p.id == proposal_id);
    if on_match {
        Ok(())
    } else {
        Err(ApiError::not_found("Proposal not found on this match"))
    }
}

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
        (status = 403, description = "Only the team captain, owner or a delegate can schedule", body = ApiError),
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

    scheduling_registrations(&state, &auth, match_id).await?;

    let proposal = state
        .scheduling_service
        .propose_schedule(
            match_id,
            req.proposed_times,
            RegistrationActor::new(auth.user_id, auth.player_id),
            req.notes,
        )
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
        (status = 403, description = "Only the team captain, owner or a delegate can schedule", body = ApiError),
        (status = 404, description = "Proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn accept_schedule_proposal(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AcceptScheduleProposalRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposal_id: ScheduleProposalId = req
        .proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    scheduling_registrations(&state, &auth, match_id).await?;
    require_proposal_on_match(&state, match_id, proposal_id).await?;

    let command = AcceptProposalCommand {
        proposal_id,
        selected_time: req.selected_time,
        accepted_by: RegistrationActor::new(auth.user_id, auth.player_id),
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
        (status = 403, description = "Only the team captain, owner or a delegate can schedule", body = ApiError),
        (status = 404, description = "Proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn reject_schedule_proposal(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<RejectScheduleProposalRequest>,
) -> ApiResult<Json<DataResponse<ScheduleProposalResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposal_id: ScheduleProposalId = req
        .proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    scheduling_registrations(&state, &auth, match_id).await?;
    require_proposal_on_match(&state, match_id, proposal_id).await?;

    let command = RejectProposalCommand {
        proposal_id,
        rejected_by: RegistrationActor::new(auth.user_id, auth.player_id),
        reason: req.reason,
    };

    let proposal = state.scheduling_service.reject_proposal(command).await?;

    Ok(Json(DataResponse::new(
        ScheduleProposalResponse::from(proposal),
        request_id,
    )))
}

/// Withdraw a schedule proposal you made yourself.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/cancel",
    request_body = CancelScheduleProposalRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Proposal withdrawn", body = DataResponse<ScheduleProposalResponse>),
        (status = 400, description = "Proposal is no longer pending", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Only the proposing team's captain, owner or delegate (or the proposer) may withdraw", body = ApiError),
        (status = 404, description = "Proposal not found on this match", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn cancel_schedule_proposal(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<CancelScheduleProposalRequest>,
) -> ApiResult<Json<DataResponse<ScheduleProposalResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposal_id: ScheduleProposalId = req
        .proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    // Who may schedule for a side is resolved here; whether this proposal
    // belongs to that side needs the proposal row, so the service decides.
    // `NotAuthorized` maps to 403.
    let acting_for_registrations = scheduling_registrations(&state, &auth, match_id).await?;
    let command = CancelProposalCommand {
        proposal_id,
        cancelled_by_user_id: auth.user_id,
        acting_for_registrations,
        match_id,
    };

    let proposal = state.scheduling_service.cancel_proposal(command).await?;

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
        (status = 403, description = "Only the team captain, owner or a delegate can schedule", body = ApiError),
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

    scheduling_registrations(&state, &auth, match_id).await?;

    // The proposal being countered is the one named in the request, not
    // "whichever is newest on the match".
    let original_proposal = state
        .scheduling_service
        .get_proposal(original_proposal_id)
        .await?
        .filter(|p| p.match_id == match_id)
        .ok_or_else(|| ApiError::not_found("Proposal not found on this match"))?;

    let tournament_match = state
        .scheduling_service
        .get_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    // The counter comes from the other side of the match.
    let proposer = original_proposal.proposed_by_registration_id;
    let registration_id = match (
        tournament_match.participant1_registration_id,
        tournament_match.participant2_registration_id,
    ) {
        (Some(p1), Some(p2)) if p1 == proposer => p2,
        (Some(p1), Some(p2)) if p2 == proposer => p1,
        (Some(_), Some(_)) => {
            return Err(ApiError::bad_request(
                "The proposal was not made by a participant of this match",
            ));
        }
        _ => return Err(ApiError::bad_request("Opponent not assigned to match")),
    };

    let command = CounterProposeCommand {
        original_proposal_id,
        match_id,
        proposed_by_registration_id: registration_id,
        proposed_by: RegistrationActor::new(auth.user_id, auth.player_id),
        proposed_times: req.proposed_times,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(48),
        notes: req.notes,
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

    let response: Vec<ScheduleProposalResponse> = proposals.into_iter().map(Into::into).collect();

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
        .require_permission(
            &auth,
            portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY,
        )
        .await?;

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let match_ = state
        .scheduling_service
        .admin_schedule(
            match_id,
            req.scheduled_at,
            auth.user_id,
            req.notes,
            req.allow_past,
        )
        .await?;

    // An admin may overrule a clash (they can see the whole evening), so it
    // is reported back rather than refused.
    let schedule_warning = state
        .scheduling_service
        .schedule_clash(&match_, req.scheduled_at)
        .await
        .unwrap_or_default();

    let mut response = TournamentMatchResponse::from(match_);
    response.schedule_warning = schedule_warning;
    Ok(Json(DataResponse::new(response, request_id)))
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

    let response: Vec<TournamentStandingResponse> = standings.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Generate the next Swiss round for a tournament.
///
/// Advances every Swiss bracket of the active stage whose current round is
/// settled (every match played or cancelled) and which has rounds left: the one bracket of a Swiss
/// tournament, or each Swiss group of a groups-and-playoffs tournament
/// (groups advance independently). The response lists the new matches of
/// every bracket that advanced.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/generate-next-round",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Next round generated for every Swiss bracket/group that was ready", body = DataResponse<Vec<TournamentMatchResponse>>),
        (status = 400, description = "Not Swiss format, or no Swiss bracket could advance (a round still in progress, or every bracket has played its final round)", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
        (status = 409, description = "A concurrent request is already generating the round, or some brackets advanced and others failed (the detail names both; retrying only advances the brackets still behind)", body = ApiError),
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
        .require_permission(
            &auth,
            portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY,
        )
        .await?;

    let new_matches = state
        .tournament_service
        .generate_next_swiss_round(tournament_id)
        .await?;

    let response: Vec<TournamentMatchResponse> = new_matches.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}
