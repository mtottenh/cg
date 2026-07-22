//! League season handlers.

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::{CreateLeagueSeasonRequest, UpdateLeagueSeasonRequest};
use crate::dto::responses::LeagueSeasonResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::LeagueTeamState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::{LeagueId, LeagueSeasonId, permissions};

/// Query parameters for listing seasons.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ListSeasonsParams {
    /// League ID to list seasons for.
    pub league_id: String,
}

/// Create a new league season.
#[utoipa::path(
    post,
    path = "/v1/league-seasons",
    request_body = CreateLeagueSeasonRequest,
    responses(
        (status = 201, description = "Season created", body = DataResponse<LeagueSeasonResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - requires league admin", body = ApiError),
        (status = 404, description = "League not found", body = ApiError),
        (status = 409, description = "Season slug already taken", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-seasons"
)]
pub async fn create_season(
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<CreateLeagueSeasonRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueSeasonResponse>>)> {
    let request_id = get_request_id(&headers);

    let league_id: LeagueId = req
        .league_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid league ID format"))?;

    // Check league admin permission
    perm_checker
        .require_league_permission(
            &auth,
            league_id.as_uuid(),
            permissions::league::SETTINGS_MANAGE,
        )
        .await?;

    let cmd = req.try_into()?;
    let season = state
        .league_season_service
        .create_season(auth.user_id, cmd)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            LeagueSeasonResponse::from(season),
            request_id,
        )),
    ))
}

/// Get a season by ID.
#[utoipa::path(
    get,
    path = "/v1/league-seasons/{season_id}",
    params(
        ("season_id" = String, Path, description = "Season ID")
    ),
    responses(
        (status = 200, description = "Season found", body = DataResponse<LeagueSeasonResponse>),
        (status = 404, description = "Season not found", body = ApiError),
    ),
    tag = "league-seasons"
)]
pub async fn get_season(
    State(state): State<LeagueTeamState>,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
) -> ApiResult<Json<DataResponse<LeagueSeasonResponse>>> {
    let request_id = get_request_id(&headers);

    let season = state.league_season_service.get_season(season_id).await?;

    Ok(Json(DataResponse::new(
        LeagueSeasonResponse::from(season),
        request_id,
    )))
}

/// List seasons for a league.
#[utoipa::path(
    get,
    path = "/v1/league-seasons",
    params(ListSeasonsParams),
    responses(
        (status = 200, description = "Seasons list", body = DataResponse<Vec<LeagueSeasonResponse>>),
        (status = 400, description = "Invalid league ID", body = ApiError),
    ),
    tag = "league-seasons"
)]
pub async fn list_seasons(
    State(state): State<LeagueTeamState>,
    headers: HeaderMap,
    Query(params): Query<ListSeasonsParams>,
) -> ApiResult<Json<DataResponse<Vec<LeagueSeasonResponse>>>> {
    let request_id = get_request_id(&headers);

    let league_id: LeagueId = params
        .league_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid league ID format"))?;

    let seasons = state.league_season_service.list_seasons(league_id).await?;

    Ok(Json(DataResponse::new(
        seasons
            .into_iter()
            .map(LeagueSeasonResponse::from)
            .collect(),
        request_id,
    )))
}

/// Update a season.
#[utoipa::path(
    patch,
    path = "/v1/league-seasons/{season_id}",
    params(
        ("season_id" = String, Path, description = "Season ID")
    ),
    request_body = UpdateLeagueSeasonRequest,
    responses(
        (status = 200, description = "Season updated", body = DataResponse<LeagueSeasonResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Season not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-seasons"
)]
pub async fn update_season(
    State(state): State<LeagueTeamState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
    ValidatedJson(req): ValidatedJson<UpdateLeagueSeasonRequest>,
) -> ApiResult<Json<DataResponse<LeagueSeasonResponse>>> {
    let request_id = get_request_id(&headers);

    // Get the season to find the league
    let existing = state.league_season_service.get_season(season_id).await?;

    // Check league admin permission
    perm_checker
        .require_league_permission(
            &auth,
            existing.league_id.as_uuid(),
            permissions::league::SETTINGS_MANAGE,
        )
        .await?;

    let cmd = req.try_into()?;
    let updated = state
        .league_season_service
        .update_season(season_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueSeasonResponse::from(updated),
        request_id,
    )))
}
