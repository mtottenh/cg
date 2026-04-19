//! League team handlers (persistent team identity).

use super::{default_page, default_per_page, get_request_id};
use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    CreateLeagueTeamRequest, RegisterTeamForSeasonRequest, TransferOwnershipRequest,
    UpdateLeagueTeamRequest,
};
use crate::dto::responses::{
    LeagueTeamResponse, LeagueTeamSeasonResponse, LeagueTeamSummaryResponse,
    LeagueTeamWithSeasonResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{LeagueSeasonId, LeagueTeamId, ScopeType};

/// Query parameters for listing teams in a league.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ListLeagueTeamsParams {
    /// Optional search term.
    #[serde(default)]
    pub search: Option<String>,
    /// Page number (1-based).
    #[serde(default = "default_page")]
    pub page: i64,
    /// Items per page.
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

/// Query parameters for listing team seasons.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ListTeamSeasonsParams {
    /// Page number (1-based).
    #[serde(default = "default_page")]
    pub page: i64,
    /// Items per page.
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

/// Create a new league team and register for a season.
///
/// Creates a team with persistent identity at the league level and
/// automatically registers it for the specified season.
#[utoipa::path(
    post,
    path = "/v1/league-seasons/{season_id}/teams",
    params(
        ("season_id" = String, Path, description = "Season ID to register the team for")
    ),
    request_body = CreateLeagueTeamRequest,
    responses(
        (status = 201, description = "Team created", body = DataResponse<LeagueTeamWithSeasonResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Team name/tag taken or already on a team", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn create_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(season_id): Path<String>,
    ValidatedJson(req): ValidatedJson<CreateLeagueTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueTeamWithSeasonResponse>>)> {
    let request_id = get_request_id(&headers);

    let season_id: LeagueSeasonId = season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid season ID format"))?;

    // Get the season to find the league
    let season = state.league_season_service.get_season(season_id).await?;

    let cmd = req.into_command(season.league_id, season_id);
    let (team, team_season) = state
        .league_team_service
        .create_team(auth.player_id, cmd)
        .await?;

    let response = LeagueTeamWithSeasonResponse {
        team: LeagueTeamResponse::from(team),
        team_season: LeagueTeamSeasonResponse::from(team_season),
    };

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(response, request_id)),
    ))
}

/// Register an existing team for a new season.
#[utoipa::path(
    post,
    path = "/v1/league-seasons/{season_id}/teams/register",
    params(
        ("season_id" = String, Path, description = "Season ID")
    ),
    request_body = RegisterTeamForSeasonRequest,
    responses(
        (status = 201, description = "Team registered", body = DataResponse<LeagueTeamSeasonResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - owner only", body = ApiError),
        (status = 404, description = "Team or season not found", body = ApiError),
        (status = 409, description = "Already registered or registration closed", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn register_team_for_season(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(season_id): Path<String>,
    ValidatedJson(req): ValidatedJson<RegisterTeamForSeasonRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueTeamSeasonResponse>>)> {
    let request_id = get_request_id(&headers);

    let season_id: LeagueSeasonId = season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid season ID format"))?;

    let team_id: LeagueTeamId = req
        .team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    // Allow the team owner, or a platform admin holding the team override.
    let team = state.league_team_service.get_team(team_id).await?;
    if !team.is_owner(auth.player_id)
        && !perm.has_admin_override(&auth, ScopeType::Team).await
    {
        return Err(ApiError::forbidden("Only the team owner or a platform admin can register for seasons"));
    }

    let team_season = state
        .league_team_service
        .register_for_season(team_id, season_id, auth.player_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(LeagueTeamSeasonResponse::from(team_season), request_id)),
    ))
}

/// Get a team by ID.
#[utoipa::path(
    get,
    path = "/v1/league-teams/{team_id}",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    responses(
        (status = 200, description = "Team found", body = DataResponse<LeagueTeamResponse>),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    tag = "league-teams"
)]
pub async fn get_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team_id): Path<String>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

    let team_id: LeagueTeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    let team = state.league_team_service.get_team(team_id).await?;

    Ok(Json(DataResponse::new(
        LeagueTeamResponse::from(team),
        request_id,
    )))
}

/// List teams registered for a season (with summaries).
#[utoipa::path(
    get,
    path = "/v1/league-seasons/{season_id}/teams",
    params(
        ("season_id" = String, Path, description = "Season ID"),
        ListTeamSeasonsParams
    ),
    responses(
        (status = 200, description = "Teams list", body = PaginatedResponse<LeagueTeamSummaryResponse>),
        (status = 400, description = "Invalid parameters", body = ApiError),
    ),
    tag = "league-teams"
)]
pub async fn list_teams_in_season(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(season_id): Path<String>,
    Query(params): Query<ListTeamSeasonsParams>,
) -> ApiResult<Json<PaginatedResponse<LeagueTeamSummaryResponse>>> {
    let request_id = get_request_id(&headers);

    let season_id: LeagueSeasonId = season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid season ID format"))?;

    let per_page = params.per_page.clamp(1, 100) as u32;
    let page = params.page.max(1) as u32;
    let offset = i64::from((page - 1) * per_page);

    let (summaries, total) = state
        .league_team_service
        .list_team_summaries(season_id, i64::from(per_page), offset)
        .await?;

    let pagination_params = PaginationParams { page, per_page };

    Ok(Json(PaginatedResponse::new(
        summaries
            .into_iter()
            .map(LeagueTeamSummaryResponse::from)
            .collect(),
        &pagination_params,
        total as u64,
        request_id,
    )))
}

/// Update a team's persistent identity (owner only).
#[utoipa::path(
    patch,
    path = "/v1/league-teams/{team_id}",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body = UpdateLeagueTeamRequest,
    responses(
        (status = 200, description = "Team updated", body = DataResponse<LeagueTeamResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - owner only", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn update_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    ValidatedJson(req): ValidatedJson<UpdateLeagueTeamRequest>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

    let team_id: LeagueTeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    // Allow the team owner, or a platform admin holding the team override.
    let team = state.league_team_service.get_team(team_id).await?;
    if !team.is_owner(auth.player_id)
        && !perm.has_admin_override(&auth, ScopeType::Team).await
    {
        return Err(ApiError::forbidden("Only the team owner or a platform admin can update team settings"));
    }

    let cmd = req.into();
    let updated = state
        .league_team_service
        .update_team_authorized(team_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamResponse::from(updated),
        request_id,
    )))
}

/// Disband a team (owner only).
#[utoipa::path(
    delete,
    path = "/v1/league-teams/{team_id}",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    responses(
        (status = 204, description = "Team disbanded"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - owner only", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn disband_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    Path(team_id): Path<String>,
) -> ApiResult<StatusCode> {
    let team_id: LeagueTeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    // Allow the team owner, or a platform admin holding the team override.
    let team = state.league_team_service.get_team(team_id).await?;
    if !team.is_owner(auth.player_id)
        && !perm.has_admin_override(&auth, ScopeType::Team).await
    {
        return Err(ApiError::forbidden("Only the team owner or a platform admin can disband the team"));
    }

    state.league_team_service.disband_team(team_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Transfer team ownership to another player (owner only).
#[utoipa::path(
    post,
    path = "/v1/league-teams/{team_id}/transfer-ownership",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body = TransferOwnershipRequest,
    responses(
        (status = 200, description = "Ownership transferred", body = DataResponse<LeagueTeamResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - owner only", body = ApiError),
        (status = 404, description = "Team or new owner not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn transfer_ownership(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    ValidatedJson(req): ValidatedJson<TransferOwnershipRequest>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

    let team_id: LeagueTeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    let new_owner_id = req.parse_new_owner()?;

    let team = state
        .league_team_service
        .transfer_ownership(team_id, auth.player_id, new_owner_id)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamResponse::from(team),
        request_id,
    )))
}
