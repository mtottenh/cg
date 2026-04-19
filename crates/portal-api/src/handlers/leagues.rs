//! League handlers.

use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    ApplyToLeagueRequest, CreateLeagueRequest, InviteToLeagueRequest, UpdateLeagueMemberRoleRequest,
    UpdateLeagueRequest,
};
use crate::dto::responses::{
    LeagueInvitationResponse, LeagueMemberBasicResponse, LeagueMemberResponse, LeagueResponse,
    UserLeagueMembershipResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::LeaguesState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{permissions, GameId, LeagueId, ScopeType, UserId};
use portal_domain::entities::league::LeagueMembershipType;

/// Check league entry requirements using the eligibility service.
///
/// Delegates to `EligibilityService::check_players_from_settings` which
/// parses restrictions from the league's settings JSONB, fetches the
/// player's game profile and rating stats for the correct game, and
/// runs the standard eligibility check.
async fn check_league_entry_requirements(
    state: &LeaguesState,
    league: &portal_domain::entities::league::League,
    player_id: portal_core::PlayerId,
) -> Result<(), ApiError> {
    let violations = state
        .eligibility_service
        .check_players_from_settings(&league.settings, league.game_id, &[player_id])
        .await?;

    if violations.is_empty() {
        return Ok(());
    }

    let messages: Vec<String> = violations.iter().map(|v| v.message.clone()).collect();
    Err(ApiError::bad_request(&format!(
        "You do not meet the entry requirements: {}",
        messages.join("; ")
    )))
}

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Query parameters for listing leagues.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ListLeaguesParams {
    /// Filter by game ID.
    #[serde(default)]
    pub game_id: Option<String>,
    /// Page number (1-based).
    #[serde(default = "default_page")]
    pub page: i64,
    /// Items per page.
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

const fn default_page() -> i64 {
    1
}

const fn default_per_page() -> i64 {
    20
}

/// Create a new league.
#[utoipa::path(
    post,
    path = "/v1/leagues",
    request_body = CreateLeagueRequest,
    responses(
        (status = 201, description = "League created", body = DataResponse<LeagueResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "League slug already taken", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn create_league(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<CreateLeagueRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueResponse>>)> {
    let request_id = get_request_id(&headers);

    // Convert request to domain command
    let cmd = req.try_into()?;

    // Create the league - user becomes admin
    let league = state
        .league_service
        .create_league(auth.user_id, cmd)
        .await?;

    // Assign RBAC scoped role: league_admin for the founder
    if let Err(e) = state
        .role_repo
        .assign_scoped_role(
            auth.user_id.as_uuid(),
            "league_admin",
            ScopeType::League,
            league.id.as_uuid(),
            None,
        )
        .await
    {
        tracing::error!(
            league_id = %league.id,
            user_id = %auth.user_id,
            error = %e,
            "Failed to assign league_admin RBAC role to founder"
        );
    }

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(LeagueResponse::from(league), request_id)),
    ))
}

/// Get a league by ID.
#[utoipa::path(
    get,
    path = "/v1/leagues/{league_id}",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    responses(
        (status = 200, description = "League found", body = DataResponse<LeagueResponse>),
        (status = 404, description = "League not found", body = ApiError),
    ),
    tag = "leagues"
)]
pub async fn get_league(
    State(state): State<LeaguesState>,
    headers: HeaderMap,
    Path(league_id): Path<LeagueId>,
) -> ApiResult<Json<DataResponse<LeagueResponse>>> {
    let request_id = get_request_id(&headers);

    let league = state.league_service.get_league(league_id).await?;

    Ok(Json(DataResponse::new(
        LeagueResponse::from(league),
        request_id,
    )))
}

/// Get a league by slug.
#[utoipa::path(
    get,
    path = "/v1/leagues/by-slug/{slug}",
    params(
        ("slug" = String, Path, description = "League slug")
    ),
    responses(
        (status = 200, description = "League found", body = DataResponse<LeagueResponse>),
        (status = 404, description = "League not found", body = ApiError),
    ),
    tag = "leagues"
)]
pub async fn get_league_by_slug(
    State(state): State<LeaguesState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> ApiResult<Json<DataResponse<LeagueResponse>>> {
    let request_id = get_request_id(&headers);

    let league = state.league_service.get_league_by_slug(&slug).await?;

    Ok(Json(DataResponse::new(
        LeagueResponse::from(league),
        request_id,
    )))
}

/// List leagues with optional game filter.
#[utoipa::path(
    get,
    path = "/v1/leagues",
    params(ListLeaguesParams),
    responses(
        (status = 200, description = "List of leagues", body = PaginatedResponse<LeagueResponse>),
    ),
    tag = "leagues"
)]
pub async fn list_leagues(
    State(state): State<LeaguesState>,
    headers: HeaderMap,
    Query(params): Query<ListLeaguesParams>,
) -> ApiResult<Json<PaginatedResponse<LeagueResponse>>> {
    let request_id = get_request_id(&headers);

    let page = params.page.max(1) as u32;
    let per_page = (params.per_page.clamp(1, 100)) as u32;
    let offset = i64::from((page - 1) * per_page);
    let limit = i64::from(per_page);

    let (leagues, total) = if let Some(game_id_str) = params.game_id {
        let game_id: GameId = game_id_str
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid game ID format"))?;
        state
            .league_service
            .list_leagues_by_game(&game_id, limit, offset)
            .await?
    } else {
        // Search all leagues with empty query
        state
            .league_service
            .search_leagues("", None, limit, offset)
            .await?
    };

    let pagination = PaginationParams { page, per_page };

    Ok(Json(PaginatedResponse::new(
        leagues.into_iter().map(LeagueResponse::from).collect(),
        &pagination,
        total as u64,
        request_id,
    )))
}

/// Update a league.
#[utoipa::path(
    patch,
    path = "/v1/leagues/{league_id}",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    request_body = UpdateLeagueRequest,
    responses(
        (status = 200, description = "League updated", body = DataResponse<LeagueResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing required permission", body = ApiError),
        (status = 404, description = "League not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn update_league(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(league_id): Path<LeagueId>,
    ValidatedJson(req): ValidatedJson<UpdateLeagueRequest>,
) -> ApiResult<Json<DataResponse<LeagueResponse>>> {
    let request_id = get_request_id(&headers);

    // Check RBAC permission
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::SETTINGS_MANAGE)
        .await?;

    // Convert request to domain command
    let cmd = req.try_into()?;

    let league = state
        .league_service
        .update_league_authorized(league_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueResponse::from(league),
        request_id,
    )))
}

// =============================================================================
// Member Management
// =============================================================================

/// List league members.
#[utoipa::path(
    get,
    path = "/v1/leagues/{league_id}/members",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    responses(
        (status = 200, description = "List of league members", body = Vec<LeagueMemberResponse>),
        (status = 404, description = "League not found", body = ApiError),
    ),
    tag = "leagues"
)]
pub async fn list_members(
    State(state): State<LeaguesState>,
    Path(league_id): Path<LeagueId>,
    Query(params): Query<PaginationParams>,
) -> ApiResult<Json<Vec<LeagueMemberResponse>>> {

    let offset = i64::from((params.page.max(1) - 1) * params.per_page);
    let limit = i64::from(params.per_page.clamp(1, 100));

    let (members, _total) = state.league_service.get_members(league_id, limit, offset).await?;

    let response: Vec<LeagueMemberResponse> = members
        .into_iter()
        .map(LeagueMemberResponse::from)
        .collect();

    Ok(Json(response))
}

/// Join an open league.
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/join",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    responses(
        (status = 200, description = "Joined league successfully", body = LeagueMemberResponse),
        (status = 400, description = "League is not open for joining", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "League not found", body = ApiError),
        (status = 409, description = "Already a member", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn join_league(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    Path(league_id): Path<LeagueId>,
) -> ApiResult<Json<LeagueMemberBasicResponse>> {

    // Check entry requirements via game plugin before joining
    let league = state.league_service.get_league(league_id).await?;
    check_league_entry_requirements(&state, &league, auth.player_id).await?;

    let member = state
        .league_service
        .join_league(league_id, auth.user_id)
        .await?;

    Ok(Json(LeagueMemberBasicResponse::from(member)))
}

/// Leave a league.
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/leave",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    responses(
        (status = 204, description = "Left league successfully"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "League not found or not a member", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn leave_league(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    Path(league_id): Path<LeagueId>,
) -> ApiResult<StatusCode> {

    state
        .league_service
        .leave_league(league_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Update a member's role.
#[utoipa::path(
    patch,
    path = "/v1/leagues/{league_id}/members/{user_id}",
    params(
        ("league_id" = String, Path, description = "League ID"),
        ("user_id" = String, Path, description = "User ID")
    ),
    request_body = UpdateLeagueMemberRoleRequest,
    responses(
        (status = 200, description = "Member role updated", body = LeagueMemberResponse),
        (status = 400, description = "Invalid role", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "League or member not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn update_member_role(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((league_id, user_id)): Path<(LeagueId, UserId)>,
    ValidatedJson(req): ValidatedJson<UpdateLeagueMemberRoleRequest>,
) -> ApiResult<Json<LeagueMemberBasicResponse>> {

    // Check RBAC permission
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::MEMBERS_MANAGE)
        .await?;

    // Parse membership type
    let membership_type = LeagueMembershipType::from_str(&req.membership_type)
        .ok_or_else(|| ApiError::bad_request("Invalid membership type"))?;

    let member = state
        .league_service
        .update_member_role_authorized(league_id, user_id, membership_type)
        .await?;

    Ok(Json(LeagueMemberBasicResponse::from(member)))
}

/// Remove a member from a league.
#[utoipa::path(
    delete,
    path = "/v1/leagues/{league_id}/members/{user_id}",
    params(
        ("league_id" = String, Path, description = "League ID"),
        ("user_id" = String, Path, description = "User ID")
    ),
    responses(
        (status = 204, description = "Member removed"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "League or member not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn remove_member(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((league_id, user_id)): Path<(LeagueId, UserId)>,
) -> ApiResult<StatusCode> {

    // Check RBAC permission
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::MEMBERS_MANAGE)
        .await?;

    state
        .league_service
        .remove_member_authorized(league_id, user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// Applications & Invitations
// =============================================================================

/// Apply to join a league (for application-based leagues).
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/apply",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    request_body = ApplyToLeagueRequest,
    responses(
        (status = 201, description = "Application submitted", body = LeagueInvitationResponse),
        (status = 400, description = "League does not accept applications", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "League not found", body = ApiError),
        (status = 409, description = "Already applied or already a member", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn apply_to_league(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(league_id): Path<LeagueId>,
    ValidatedJson(req): ValidatedJson<ApplyToLeagueRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueInvitationResponse>>)> {
    let request_id = get_request_id(&headers);

    // Check entry requirements via game plugin before applying
    let league = state.league_service.get_league(league_id).await?;
    check_league_entry_requirements(&state, &league, auth.player_id).await?;

    let application = state
        .league_service
        .apply_to_league(league_id, auth.user_id, req.message)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            LeagueInvitationResponse::from(application),
            request_id,
        )),
    ))
}

/// Invite a user to join a league.
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/invitations",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    request_body = InviteToLeagueRequest,
    responses(
        (status = 201, description = "Invitation sent", body = LeagueInvitationResponse),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "League not found", body = ApiError),
        (status = 409, description = "User already invited or already a member", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn invite_user(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(league_id): Path<LeagueId>,
    ValidatedJson(req): ValidatedJson<InviteToLeagueRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueInvitationResponse>>)> {
    let request_id = get_request_id(&headers);
    let target_user_id: UserId = req
        .user_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid user ID format"))?;

    // Check RBAC permission
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::MEMBERS_MANAGE)
        .await?;

    let invitation = state
        .league_service
        .invite_user_authorized(league_id, target_user_id, auth.user_id, req.message, None)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            LeagueInvitationResponse::from(invitation),
            request_id,
        )),
    ))
}

/// List pending invitations for a league.
#[utoipa::path(
    get,
    path = "/v1/leagues/{league_id}/invitations",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    responses(
        (status = 200, description = "List of invitations", body = Vec<LeagueInvitationResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "League not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn list_invitations(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path(league_id): Path<LeagueId>,
) -> ApiResult<Json<Vec<LeagueInvitationResponse>>> {

    // Check RBAC permission to view invitations
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::MEMBERS_MANAGE)
        .await?;

    let all_pending = state
        .league_service
        .get_pending_by_league_authorized(league_id)
        .await?;

    // Filter to only show invitations (sent by admins)
    let response: Vec<LeagueInvitationResponse> = all_pending
        .into_iter()
        .filter(|inv| inv.invitation_type == portal_domain::entities::league::LeagueInvitationType::Invite)
        .map(LeagueInvitationResponse::from)
        .collect();

    Ok(Json(response))
}

/// List pending applications for a league.
#[utoipa::path(
    get,
    path = "/v1/leagues/{league_id}/applications",
    params(
        ("league_id" = String, Path, description = "League ID")
    ),
    responses(
        (status = 200, description = "List of applications", body = Vec<LeagueInvitationResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "League not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn list_applications(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path(league_id): Path<LeagueId>,
) -> ApiResult<Json<Vec<LeagueInvitationResponse>>> {

    // Check RBAC permission to view applications
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::MEMBERS_MANAGE)
        .await?;

    let all_pending = state
        .league_service
        .get_pending_by_league_authorized(league_id)
        .await?;

    // Filter to only show applications (submitted by users)
    let response: Vec<LeagueInvitationResponse> = all_pending
        .into_iter()
        .filter(|inv| inv.invitation_type == portal_domain::entities::league::LeagueInvitationType::Application)
        .map(LeagueInvitationResponse::from)
        .collect();

    Ok(Json(response))
}

/// Approve an application to join a league.
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/applications/{application_id}/approve",
    params(
        ("league_id" = String, Path, description = "League ID"),
        ("application_id" = String, Path, description = "Application ID")
    ),
    responses(
        (status = 200, description = "Application approved", body = LeagueMemberResponse),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "Application not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn approve_application(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((league_id, application_id)): Path<(String, String)>,
) -> ApiResult<Json<LeagueMemberBasicResponse>> {
    let league_id: LeagueId = league_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid league ID format"))?;
    let invitation_id: portal_core::LeagueInvitationId = application_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid application ID format"))?;

    // Check RBAC permission
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::MEMBERS_MANAGE)
        .await?;

    let member = state
        .league_service
        .approve_application_authorized(invitation_id, auth.user_id)
        .await?;

    Ok(Json(LeagueMemberBasicResponse::from(member)))
}

/// Reject an application to join a league.
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/applications/{application_id}/reject",
    params(
        ("league_id" = String, Path, description = "League ID"),
        ("application_id" = String, Path, description = "Application ID")
    ),
    responses(
        (status = 204, description = "Application rejected"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "Application not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn reject_application(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((league_id, application_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let league_id: LeagueId = league_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid league ID format"))?;
    let invitation_id: portal_core::LeagueInvitationId = application_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid application ID format"))?;

    // Check RBAC permission
    perm_checker
        .require_league_permission(&auth, league_id.as_uuid(), permissions::league::MEMBERS_MANAGE)
        .await?;

    state
        .league_service
        .reject_invitation_authorized(invitation_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// User-centric endpoints
// =============================================================================

/// Get the current user's league memberships.
#[utoipa::path(
    get,
    path = "/v1/users/me/leagues",
    responses(
        (status = 200, description = "User's league memberships", body = Vec<UserLeagueMembershipResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn get_my_leagues(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
) -> ApiResult<Json<Vec<UserLeagueMembershipResponse>>> {
    let memberships = state
        .league_service
        .get_user_leagues(auth.user_id)
        .await?;

    let response: Vec<UserLeagueMembershipResponse> = memberships
        .into_iter()
        .map(UserLeagueMembershipResponse::from)
        .collect();

    Ok(Json(response))
}

/// Get the current user's pending league invitations.
#[utoipa::path(
    get,
    path = "/v1/users/me/league-invitations",
    responses(
        (status = 200, description = "User's pending invitations", body = Vec<LeagueInvitationResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn get_my_invitations(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
) -> ApiResult<Json<Vec<LeagueInvitationResponse>>> {
    let invitations = state
        .league_service
        .get_pending_invitations_for_user(auth.user_id)
        .await?;

    let response: Vec<LeagueInvitationResponse> = invitations
        .into_iter()
        .map(LeagueInvitationResponse::from)
        .collect();

    Ok(Json(response))
}

/// Accept a league invitation.
#[utoipa::path(
    post,
    path = "/v1/league-invitations/{invitation_id}/accept",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 200, description = "Invitation accepted", body = LeagueMemberResponse),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn accept_invitation(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> ApiResult<Json<LeagueMemberBasicResponse>> {
    let invitation_id: portal_core::LeagueInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    let member = state
        .league_service
        .accept_invitation(invitation_id, auth.user_id)
        .await?;

    Ok(Json(LeagueMemberBasicResponse::from(member)))
}

/// Decline a league invitation.
#[utoipa::path(
    post,
    path = "/v1/league-invitations/{invitation_id}/decline",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 204, description = "Invitation declined"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "leagues"
)]
pub async fn decline_invitation(
    State(state): State<LeaguesState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> ApiResult<StatusCode> {
    let invitation_id: portal_core::LeagueInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    state
        .league_service
        .decline_invitation(invitation_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
