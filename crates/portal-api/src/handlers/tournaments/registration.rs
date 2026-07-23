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

use super::{check_eligibility_for_players, get_request_id, require_registration_actor};
use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    CreateTournamentInvitationRequest, DisqualifyRequest, RegisterPlayerRequest,
    RegisterTeamRequest, RejectRegistrationRequest,
};
use crate::dto::responses::{
    CheckInStatusResponse, TournamentInvitationResponse, TournamentRegistrationResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::TournamentState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
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

/// Require `tournament.participants.manage` on the tournament that owns
/// the given registration.
///
/// The check deliberately resolves the tournament from the registration
/// row rather than trusting the `tournament_id` path segment — otherwise
/// an admin of tournament A could act on a registration belonging to
/// tournament B by crafting the URL.
async fn require_registration_manage(
    state: &TournamentState,
    auth: &AuthenticatedUser,
    perm_checker: &PermissionChecker,
    registration_id: portal_core::TournamentRegistrationId,
) -> ApiResult<()> {
    let registration = state
        .registration_service
        .get_registration(registration_id)
        .await?;

    perm_checker
        .require_tournament_permission(
            auth,
            registration.tournament_id.as_uuid(),
            portal_core::permissions::tournament::PARTICIPANTS_MANAGE,
        )
        .await?;

    Ok(())
}

/// Path parameters for invitation operations.
#[derive(Debug, serde::Deserialize)]
pub struct InvitationPath {
    tournament_id: String,
    invitation_id: String,
}

/// Invite a user or team to an invite-only tournament.
///
/// The invite list is what makes `registration_type = "invite_only"` mean
/// anything: before audit P-27 no invite concept existed and an invite-only
/// tournament accepted registrations from anybody.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/invitations",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = CreateTournamentInvitationRequest,
    responses(
        (status = 201, description = "Invitation created", body = DataResponse<TournamentInvitationResponse>),
        (status = 400, description = "Invalid invite target", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
        (status = 409, description = "Target already invited", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn create_invitation(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<CreateTournamentInvitationRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentInvitationResponse>>)> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            portal_core::permissions::tournament::PARTICIPANTS_MANAGE,
        )
        .await?;

    let (user_id, team_season_id) = req.parse_target()?;

    let invitation = state
        .tournament_service
        .invite_to_tournament(
            tournament_id,
            user_id,
            team_season_id,
            req.message,
            auth.user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TournamentInvitationResponse::from(invitation),
            request_id,
        )),
    ))
}

/// List a tournament's invitations.
#[utoipa::path(
    get,
    // `operationId` defaults to the handler name, and `leagues::list_invitations`
    // already claims `list_invitations`. Two operations sharing an ID make the
    // document ambiguous and break generated clients — `openapi-typescript`
    // emits one `operations` member per ID, so the collision produced a
    // TypeScript file that would not compile (duplicate identifier).
    operation_id = "list_tournament_invitations",
    path = "/v1/tournaments/{tournament_id}/invitations",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Invitations", body = DataResponse<Vec<TournamentInvitationResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn list_invitations(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentInvitationResponse>>>> {
    let request_id = get_request_id(&headers);

    // The invite list names who an organiser considered; it is not public.
    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            portal_core::permissions::tournament::PARTICIPANTS_MANAGE,
        )
        .await?;

    let invitations = state
        .tournament_service
        .list_invitations(tournament_id)
        .await?;

    let data: Vec<TournamentInvitationResponse> = invitations.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Revoke a tournament invitation.
#[utoipa::path(
    delete,
    path = "/v1/tournaments/{tournament_id}/invitations/{invitation_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("invitation_id" = String, Path, description = "Invitation ID"),
    ),
    responses(
        (status = 200, description = "Invitation revoked", body = DataResponse<TournamentInvitationResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn revoke_invitation(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(path): Path<InvitationPath>,
) -> ApiResult<Json<DataResponse<TournamentInvitationResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = path
        .tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;
    let invitation_id: portal_core::TournamentInvitationId = path
        .invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            portal_core::permissions::tournament::PARTICIPANTS_MANAGE,
        )
        .await?;

    // The service re-checks that the invitation belongs to this tournament,
    // so the permission above cannot be satisfied against tournament A to
    // revoke an invitation owned by tournament B.
    let invitation = state
        .tournament_service
        .revoke_invitation(tournament_id, invitation_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentInvitationResponse::from(invitation),
        request_id,
    )))
}

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
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<RegisterTeamRequest>,
) -> ApiResult<(
    StatusCode,
    Json<DataResponse<TournamentRegistrationResponse>>,
)> {
    let request_id = get_request_id(&headers);

    let team_season_id = req.parse_team_season_id()?;

    // Eligibility check: fetch tournament and team members, run restrictions
    let tournament = state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;
    let members = state
        .league_team_service
        .get_members(team_season_id)
        .await?;
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
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<RegisterPlayerRequest>,
) -> ApiResult<(
    StatusCode,
    Json<DataResponse<TournamentRegistrationResponse>>,
)> {
    let request_id = get_request_id(&headers);

    let player_id = auth.player_id;

    // Eligibility check: fetch tournament and run restrictions for this player
    let tournament = state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;
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
    State(state): State<TournamentState>,
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
        .get_registrations(
            tournament_id,
            status,
            pagination.limit(),
            pagination.offset(),
        )
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
///
/// Carried the same hole as match check-in (P-24): the caller must now
/// be able to act for the registration — see
/// [`require_registration_actor`]. Staff wanting to check someone in
/// out-of-band still have the dedicated `admin-check-in` endpoint,
/// which additionally bypasses the check-in window.
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
        (status = 403, description = "Not authorized to check in this participant", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn check_in(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(path): Path<CheckInPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    require_registration_actor(&state, &auth, &perm_checker, registration_id).await?;

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
    State(state): State<TournamentState>,
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
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn approve_registration(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    require_registration_manage(&state, &auth, &perm_checker, registration_id).await?;

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
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn reject_registration(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::RejectRegistrationRequest>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    require_registration_manage(&state, &auth, &perm_checker, registration_id).await?;

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
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn disqualify(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::DisqualifyRequest>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    require_registration_manage(&state, &auth, &perm_checker, registration_id).await?;

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
    State(state): State<TournamentState>,
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
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn admin_check_in(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    require_registration_manage(&state, &auth, &perm_checker, registration_id).await?;

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
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn process_no_shows(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentRegistrationResponse>>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            portal_core::permissions::tournament::PARTICIPANTS_MANAGE,
        )
        .await?;

    let no_shows = state
        .checkin_service
        .process_no_shows(tournament_id)
        .await?;

    let data: Vec<TournamentRegistrationResponse> = no_shows.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}
