//! Team handlers.

use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{CreateTeamRequest, UpdateMemberRoleRequest, UpdateTeamRequest};
use crate::dto::responses::{TeamMemberResponse, TeamResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{permissions, PlayerId, ScopeType, TeamId};

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Create a new team.
#[utoipa::path(
    post,
    path = "/v1/teams",
    request_body = CreateTeamRequest,
    responses(
        (status = 201, description = "Team created", body = DataResponse<TeamResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Team name or tag already taken", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn create_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<CreateTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TeamResponse>>)> {
    let request_id = get_request_id(&headers);

    // Convert request to domain command
    let cmd = req.try_into()?;

    // Create the team
    let team = state
        .team_service
        .create_team(auth.player_id, cmd)
        .await?;

    // Assign RBAC scoped role: team_captain for the founder
    // This enables the user to manage the team via RBAC permissions
    if let Err(e) = state
        .role_repo
        .assign_scoped_role(
            auth.user_id.as_uuid(),
            "team_captain",
            ScopeType::Team,
            team.id.as_uuid(),
            None, // granted_by is None for self-granted founder role
        )
        .await
    {
        // Log the error but don't fail team creation
        // The team was created successfully, role assignment is secondary
        tracing::error!(
            team_id = %team.id,
            user_id = %auth.user_id,
            error = %e,
            "Failed to assign team_captain RBAC role to founder"
        );
    }

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(TeamResponse::from(team), request_id)),
    ))
}

/// Get a team by ID.
#[utoipa::path(
    get,
    path = "/v1/teams/{team_id}",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    responses(
        (status = 200, description = "Team found", body = DataResponse<TeamResponse>),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    tag = "teams"
)]
pub async fn get_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team_id): Path<String>,
) -> ApiResult<Json<DataResponse<TeamResponse>>> {
    let request_id = get_request_id(&headers);

    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    let team = state.team_service.get_team(team_id).await?;

    Ok(Json(DataResponse::new(TeamResponse::from(team), request_id)))
}

/// List teams with pagination.
///
/// Query parameters for listing teams.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ListTeamsParams {
    /// Search by name or tag.
    #[serde(default)]
    pub search: Option<String>,
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

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    20
}

/// List all teams with optional search.
#[utoipa::path(
    get,
    path = "/v1/teams",
    params(ListTeamsParams),
    responses(
        (status = 200, description = "List of teams", body = PaginatedResponse<TeamResponse>),
    ),
    tag = "teams"
)]
pub async fn list_teams(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ListTeamsParams>,
) -> ApiResult<Json<PaginatedResponse<TeamResponse>>> {
    let request_id = get_request_id(&headers);

    let page = params.page.max(1) as u32;
    let per_page = (params.per_page.clamp(1, 100)) as u32;
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    let (teams, total) = state
        .team_service
        .list_teams(params.search, limit, offset)
        .await?;

    let pagination = PaginationParams { page, per_page };

    Ok(Json(PaginatedResponse::new(
        teams.into_iter().map(TeamResponse::from).collect(),
        &pagination,
        total as u64,
        request_id,
    )))
}

/// Update a team.
#[utoipa::path(
    patch,
    path = "/v1/teams/{team_id}",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body = UpdateTeamRequest,
    responses(
        (status = 200, description = "Team updated", body = DataResponse<TeamResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing required permission", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn update_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    ValidatedJson(req): ValidatedJson<UpdateTeamRequest>,
) -> ApiResult<Json<DataResponse<TeamResponse>>> {
    let request_id = get_request_id(&headers);

    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    // Check RBAC permission: requires team.settings.manage for this team
    perm_checker
        .require_team_permission(&auth, team_id.as_uuid(), permissions::team::SETTINGS_MANAGE)
        .await?;

    // Convert request to domain command
    let cmd = req.try_into()?;

    // Update the team (permission already verified via RBAC)
    let team = state
        .team_service
        .update_team_authorized(team_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(TeamResponse::from(team), request_id)))
}

/// List team members.
#[utoipa::path(
    get,
    path = "/v1/teams/{team_id}/members",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    responses(
        (status = 200, description = "List of team members", body = Vec<TeamMemberResponse>),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    tag = "teams"
)]
pub async fn list_members(
    State(state): State<AppState>,
    Path(team_id): Path<String>,
) -> ApiResult<Json<Vec<TeamMemberResponse>>> {
    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    let members = state.team_service.get_members(team_id).await?;

    let response: Vec<TeamMemberResponse> = members.into_iter().map(TeamMemberResponse::from).collect();

    Ok(Json(response))
}

/// Update a member's role.
#[utoipa::path(
    patch,
    path = "/v1/teams/{team_id}/members/{player_id}",
    params(
        ("team_id" = String, Path, description = "Team ID"),
        ("player_id" = String, Path, description = "Player ID")
    ),
    request_body = UpdateMemberRoleRequest,
    responses(
        (status = 200, description = "Member role updated", body = TeamMemberResponse),
        (status = 400, description = "Invalid role", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "Team or member not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn update_member_role(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((team_id, player_id)): Path<(String, String)>,
    Json(req): Json<UpdateMemberRoleRequest>,
) -> ApiResult<Json<TeamMemberResponse>> {
    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;
    let player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    // Check RBAC permission: requires team.roles.manage for this team
    perm_checker
        .require_team_permission(&auth, team_id.as_uuid(), permissions::team::ROLES_MANAGE)
        .await?;

    // Convert request to domain command
    let cmd = req.into_command(player_id)?;

    // Update the member's role (permission already verified via RBAC)
    let member = state
        .team_service
        .update_member_role_authorized(team_id, cmd)
        .await?;

    Ok(Json(TeamMemberResponse::from(member)))
}

/// Remove a member from a team.
#[utoipa::path(
    delete,
    path = "/v1/teams/{team_id}/members/{player_id}",
    params(
        ("team_id" = String, Path, description = "Team ID"),
        ("player_id" = String, Path, description = "Player ID")
    ),
    responses(
        (status = 204, description = "Member removed"),
        (status = 400, description = "Cannot remove founder", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Permission denied", body = ApiError),
        (status = 404, description = "Team or member not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn remove_member(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((team_id, player_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;
    let player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    // Check RBAC permission: requires team.roster.manage for this team
    perm_checker
        .require_team_permission(&auth, team_id.as_uuid(), permissions::team::ROSTER_MANAGE)
        .await?;

    // Remove the member (permission already verified via RBAC)
    state
        .team_service
        .remove_member_authorized(team_id, player_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Leave a team.
#[utoipa::path(
    post,
    path = "/v1/teams/{team_id}/leave",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    responses(
        (status = 204, description = "Left team successfully"),
        (status = 400, description = "Cannot leave as founder", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Team not found or not a member", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn leave_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(team_id): Path<String>,
) -> ApiResult<StatusCode> {
    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    state
        .team_service
        .leave_team(team_id, auth.player_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
