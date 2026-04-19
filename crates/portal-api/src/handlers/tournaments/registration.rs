//! Tournament registration handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Covers
//! both sides of the registration flow:
//!
//! * Participant-facing: register team, register player,
//!   list registrations, check in, withdraw.
//! * Admin-facing: approve, reject, disqualify, admin check-in, process
//!   no-shows, and the check-in status summary.
//!
//! The participant-vs-admin split is enforced at the service layer
//! (different services for different operations) rather than by module
//! boundary, so everything registration-shaped lives together here.

use super::{check_eligibility_for_players, get_request_id};
use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    DisqualifyRequest, RegisterPlayerRequest, RegisterTeamRequest, RejectRegistrationRequest,
};
use crate::dto::responses::{CheckInStatusResponse, TournamentRegistrationResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{PlayerId, TournamentId};

/// Query parameter for filtering registrations by status.
#[derive(Debug, serde::Deserialize)]
pub struct RegistrationStatusQuery {
    /// Optional registration-status filter (parsed to
    /// `TournamentRegistrationStatus` at the handler boundary).
    #[serde(default)]
    pub status: Option<String>,
}

/// Path parameters for check-in.
#[derive(Debug, serde::Deserialize)]
pub struct CheckInPath {
    #[allow(dead_code)]
    tournament_id: String,
    registration_id: String,
}

/// Path parameters for registration operations.
#[derive(Debug, serde::Deserialize)]
pub struct RegistrationPath {
    #[allow(dead_code)]
    tournament_id: String,
    registration_id: String,
}

// =============================================================================

/// Register a team for a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/team",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = RegisterTeamRequest,
    responses(
        (status = 201, description = "Team registered", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Registration closed or validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
        (status = 409, description = "Already registered", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn register_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<RegisterTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentRegistrationResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_season_id = req.parse_team_season_id()?;

    // Eligibility check: fetch tournament and team members, run restrictions
    let tournament = state.tournament_service.get_tournament(tournament_id).await?;
    let members = state.league_team_service.get_members(team_season_id).await?;
    let player_ids: Vec<PlayerId> = members.iter().map(|m| m.player_id).collect();
    check_eligibility_for_players(&state, &tournament, &player_ids).await?;

    let registration = state
        .tournament_service
        .register_team(
            tournament_id,
            team_season_id,
            req.participant_name,
            req.participant_logo_url,
            auth.user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TournamentRegistrationResponse::from(registration),
            request_id,
        )),
    ))
}

/// Register a player for an individual tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/player",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = RegisterPlayerRequest,
    responses(
        (status = 201, description = "Player registered", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Registration closed or validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
        (status = 409, description = "Already registered", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn register_player(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<RegisterPlayerRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentRegistrationResponse>>)> {
    let request_id = get_request_id(&headers);

    let player_id = auth.player_id;

    // Eligibility check: fetch tournament and run restrictions for this player
    let tournament = state.tournament_service.get_tournament(tournament_id).await?;
    check_eligibility_for_players(&state, &tournament, &[player_id]).await?;

    let registration = state
        .tournament_service
        .register_player(tournament_id, player_id, req.participant_name, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TournamentRegistrationResponse::from(registration),
            request_id,
        )),
    ))
}

/// Get registrations for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/registrations",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("status" = Option<String>, Query, description = "Filter by registration status"),
        ("page" = Option<u32>, Query, description = "Page number"),
        ("per_page" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of registrations", body = PaginatedResponse<TournamentRegistrationResponse>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_registrations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    Query(status_filter): Query<RegistrationStatusQuery>,
    Query(pagination): Query<PaginationParams>,
) -> ApiResult<Json<PaginatedResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let status = status_filter
        .status
        .map(|s| {
            s.parse()
                .map_err(|_| ApiError::bad_request("Invalid registration status"))
        })
        .transpose()?;

    let (registrations, total) = state
        .tournament_service
        .get_registrations(tournament_id, status, pagination.limit(), pagination.offset())
        .await?;

    let data: Vec<TournamentRegistrationResponse> =
        registrations.into_iter().map(Into::into).collect();

    Ok(Json(PaginatedResponse::new(
        data,
        &pagination,
        total as u64,
        request_id,
    )))
}

/// Check in for a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/check-in",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Checked in", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Check-in not open or already checked in", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn check_in(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<CheckInPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .tournament_service
        .check_in(registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Withdraw from a tournament.
#[utoipa::path(
    delete,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Withdrawn successfully", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot withdraw", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .withdraw(registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Approve a pending registration (admin only).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Registration approved", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot approve", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn approve_registration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .approve_registration(registration_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Reject a pending registration (admin only).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/reject",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    request_body = RejectRegistrationRequest,
    responses(
        (status = 200, description = "Registration rejected", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot reject", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn reject_registration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::RejectRegistrationRequest>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .reject_registration(registration_id, req.reason)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Disqualify a participant (admin only).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/disqualify",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    request_body = DisqualifyRequest,
    responses(
        (status = 200, description = "Participant disqualified", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot disqualify", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn disqualify(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::DisqualifyRequest>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .disqualify(registration_id, req.reason)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Get check-in status for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/check-in-status",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Check-in status", body = DataResponse<CheckInStatusResponse>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_check_in_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<crate::dto::responses::CheckInStatusResponse>>> {
    let request_id = get_request_id(&headers);

    let status = state
        .checkin_service
        .get_check_in_status(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        crate::dto::responses::CheckInStatusResponse {
            tournament_id: status.tournament_id.to_string(),
            check_in_required: status.check_in_required,
            check_in_open: status.check_in_open,
            checked_in_count: status.checked_in_count,
            total_eligible: status.total_eligible,
        },
        request_id,
    )))
}

/// Admin check-in a participant (bypasses check-in window).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/admin-check-in",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Participant checked in", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot check in", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn admin_check_in(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .checkin_service
        .admin_check_in(registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Process no-shows (mark unchecked-in participants as no-show).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/process-no-shows",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "No-shows processed", body = DataResponse<Vec<TournamentRegistrationResponse>>),
        (status = 400, description = "Cannot process no-shows", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn process_no_shows(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentRegistrationResponse>>>> {
    let request_id = get_request_id(&headers);

    let no_shows = state
        .checkin_service
        .process_no_shows(tournament_id)
        .await?;

    let data: Vec<TournamentRegistrationResponse> =
        no_shows.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}
