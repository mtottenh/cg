//! League team handlers (persistent team identity).

use super::{default_page, default_per_page, get_request_id};
use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    CreateLeagueTeamRequest, MoveTeamRequest, RegisterTeamForSeasonRequest,
    TransferOwnershipRequest, UpdateLeagueTeamRequest,
};
use crate::dto::responses::{
    LeagueTeamResponse, LeagueTeamSeasonResponse, LeagueTeamSummaryResponse,
    LeagueTeamWithSeasonResponse, MovedTeamResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{
    AuthenticatedUser, OptionalAuthenticatedUser, PermissionChecker, ValidatedJson,
};
use crate::state::LeagueTeamState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::{LeagueSeasonId, LeagueTeamId, permissions};

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
    /// Include archived teams.
    ///
    /// Permission-gated: archiving exists to hide something from players, so
    /// asking for the hidden rows requires `league.settings.manage` on the
    /// league that owns the season (which platform admins hold everywhere).
    #[serde(default)]
    pub include_archived: bool,
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
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
    ValidatedJson(req): ValidatedJson<CreateLeagueTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueTeamWithSeasonResponse>>)> {
    let request_id = get_request_id(&headers);

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
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
    ValidatedJson(req): ValidatedJson<RegisterTeamForSeasonRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueTeamSeasonResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_id: LeagueTeamId = req
        .team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    require_team_settings_manage(&perm, &auth, team_id).await?;

    let team_season = state
        .league_team_service
        .register_for_season(team_id, season_id, auth.player_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            LeagueTeamSeasonResponse::from(team_season),
            request_id,
        )),
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
    State(state): State<LeagueTeamState>,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

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
    State(state): State<LeagueTeamState>,
    auth: OptionalAuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
    Query(params): Query<ListTeamSeasonsParams>,
) -> ApiResult<Json<PaginatedResponse<LeagueTeamSummaryResponse>>> {
    let request_id = get_request_id(&headers);

    if params.include_archived {
        let Some(user) = auth.0.as_ref() else {
            return Err(ApiError::forbidden(
                "Missing required permission: league.settings.manage",
            ));
        };
        let season = state.league_season_service.get_season(season_id).await?;
        perm.require_league_permission(
            user,
            season.league_id.as_uuid(),
            permissions::league::SETTINGS_MANAGE,
        )
        .await?;
    }

    let per_page = params.per_page.clamp(1, 100) as u32;
    let page = params.page.max(1) as u32;
    let offset = i64::from((page - 1) * per_page);

    let (summaries, total) = state
        .league_team_service
        .list_team_summaries(
            season_id,
            params.include_archived,
            i64::from(per_page),
            offset,
        )
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
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
    ValidatedJson(req): ValidatedJson<UpdateLeagueTeamRequest>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

    require_team_settings_manage(&perm, &auth, team_id).await?;

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
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    Path(team_id): Path<LeagueTeamId>,
) -> ApiResult<StatusCode> {
    require_team_settings_manage(&perm, &auth, team_id).await?;

    state.league_team_service.disband_team(team_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Archive a team.
///
/// It stops appearing in player-facing listings. Distinct from disbanding,
/// which is the team's own status saying it is over: a disbanded team that is
/// archived and later restored comes back disbanded.
#[utoipa::path(
    post,
    path = "/v1/league-teams/{team_id}/archive",
    params(("team_id" = String, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team archived", body = DataResponse<LeagueTeamResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing required permission", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn archive_team(
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

    require_team_settings_manage(&perm, &auth, team_id).await?;

    let team = state
        .league_team_service
        .archive_team(team_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamResponse::from(team),
        request_id,
    )))
}

/// Restore an archived team.
#[utoipa::path(
    post,
    path = "/v1/league-teams/{team_id}/restore",
    params(("team_id" = String, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team restored", body = DataResponse<LeagueTeamResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing required permission", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn restore_team(
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

    require_team_settings_manage(&perm, &auth, team_id).await?;

    let team = state.league_team_service.restore_team(team_id).await?;

    Ok(Json(DataResponse::new(
        LeagueTeamResponse::from(team),
        request_id,
    )))
}

/// Move a team into another league.
///
/// The repair for a team filed under the wrong league. A platform-level
/// action (`admin.teams.manage_any`): it takes a team out of one league's
/// competition and puts it into another's, which is not a decision either
/// league's own admins can make alone.
///
/// Narrow by design — see `LeagueTeamService::move_team_to_league` for what
/// it refuses and why.
#[utoipa::path(
    post,
    path = "/v1/league-teams/{team_id}/move",
    params(("team_id" = String, Path, description = "Team ID")),
    request_body = MoveTeamRequest,
    responses(
        (status = 200, description = "Team moved; `withdrawn_from` names cups it left", body = DataResponse<MovedTeamResponse>),
        (status = 400, description = "Invalid target, or the team cannot be moved", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing required permission", body = ApiError),
        (status = 404, description = "Team or season not found", body = ApiError),
        (status = 409, description = "Name or tag already taken in the target league", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn move_team(
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
    ValidatedJson(req): ValidatedJson<MoveTeamRequest>,
) -> ApiResult<Json<DataResponse<MovedTeamResponse>>> {
    let request_id = get_request_id(&headers);

    perm.require_permission(&auth, permissions::admin::TEAMS_MANAGE_ANY)
        .await?;

    let (league_id, season_id) = req.parse_target()?;

    let moved = state
        .league_team_service
        .move_team_to_league(team_id, league_id, season_id)
        .await?;

    Ok(Json(DataResponse::new(
        MovedTeamResponse::from(moved),
        request_id,
    )))
}

/// Require that the caller holds `team.settings.manage` for `team_id`.
///
/// Team owners are granted the `team_captain` RBAC role (scoped to the
/// team) at creation time — see
/// `PgLeagueTeamRepository::create_team_with_season_and_captain` and the
/// 0061 backfill migration — so this single check now covers owners,
/// explicitly-assigned captains, and platform admins (via the
/// `admin.teams.manage_any` override folded into
/// `require_team_permission`).
///
/// Thin wrapper kept for readability at the call sites, and because the
/// earlier `require_team_owner_or_admin` helper — which did an extra
/// `get_team()` roundtrip before checking ownership — was removed as
/// part of the I4 cleanup.
async fn require_team_settings_manage(
    perm: &PermissionChecker,
    auth: &AuthenticatedUser,
    team_id: LeagueTeamId,
) -> ApiResult<()> {
    perm.require_team_permission(auth, team_id.as_uuid(), permissions::team::SETTINGS_MANAGE)
        .await
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
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
    ValidatedJson(req): ValidatedJson<TransferOwnershipRequest>,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers);

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
