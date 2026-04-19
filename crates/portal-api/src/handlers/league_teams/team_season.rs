//! League team season handlers (seasonal roster management).

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::AddLeagueTeamMemberRequest;
use crate::dto::responses::{
    LeagueTeamMemberResponse, LeagueTeamMemberWithPlayerResponse, LeagueTeamSeasonResponse,
    PlayerLeagueTeamMembershipResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{LeagueTeamSeasonId, PlayerId};

/// Get a team's seasonal participation.
#[utoipa::path(
    get,
    path = "/v1/league-team-seasons/{team_season_id}",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID")
    ),
    responses(
        (status = 200, description = "Team season found", body = DataResponse<LeagueTeamSeasonResponse>),
        (status = 404, description = "Team season not found", body = ApiError),
    ),
    tag = "league-team-seasons"
)]
pub async fn get_team_season(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team_season_id): Path<String>,
) -> ApiResult<Json<DataResponse<LeagueTeamSeasonResponse>>> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    let team_season = state
        .league_team_service
        .get_team_season(team_season_id)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamSeasonResponse::from(team_season),
        request_id,
    )))
}

/// Get team season members (roster).
#[utoipa::path(
    get,
    path = "/v1/league-team-seasons/{team_season_id}/members",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID")
    ),
    responses(
        (status = 200, description = "Members list", body = DataResponse<Vec<LeagueTeamMemberWithPlayerResponse>>),
        (status = 404, description = "Team season not found", body = ApiError),
    ),
    tag = "league-team-seasons"
)]
pub async fn get_team_season_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team_season_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<LeagueTeamMemberWithPlayerResponse>>>> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    let members = state
        .league_team_service
        .get_members(team_season_id)
        .await?;

    Ok(Json(DataResponse::new(
        members
            .into_iter()
            .map(LeagueTeamMemberWithPlayerResponse::from)
            .collect(),
        request_id,
    )))
}

/// Add a member to a team's seasonal roster (captain only).
#[utoipa::path(
    post,
    path = "/v1/league-team-seasons/{team_season_id}/members",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID")
    ),
    request_body = AddLeagueTeamMemberRequest,
    responses(
        (status = 201, description = "Member added", body = DataResponse<LeagueTeamMemberResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - captain only", body = ApiError),
        (status = 404, description = "Team season or player not found", body = ApiError),
        (status = 409, description = "Already a member or roster full", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-seasons"
)]
pub async fn add_team_member(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(team_season_id): Path<String>,
    ValidatedJson(req): ValidatedJson<AddLeagueTeamMemberRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueTeamMemberResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    // Check if the player is a captain for this team season
    if !state
        .league_team_service
        .is_captain(team_season_id, auth.player_id)
        .await?
    {
        return Err(ApiError::forbidden("Only captains can add members"));
    }

    let cmd = req.into_command(team_season_id)?;
    let member = state
        .league_team_service
        .add_member_authorized(team_season_id, cmd.player_id, cmd.role, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(LeagueTeamMemberResponse::from(member), request_id)),
    ))
}

/// Remove a member from a team's seasonal roster (captain only).
#[utoipa::path(
    delete,
    path = "/v1/league-team-seasons/{team_season_id}/members/{player_id}",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID"),
        ("player_id" = String, Path, description = "Player ID to remove")
    ),
    responses(
        (status = 204, description = "Member removed"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - captain only", body = ApiError),
        (status = 404, description = "Team season or member not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-seasons"
)]
pub async fn remove_team_member(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((team_season_id, player_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    let target_player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    // Check if the player is a captain for this team season
    if !state
        .league_team_service
        .is_captain(team_season_id, auth.player_id)
        .await?
    {
        return Err(ApiError::forbidden("Only captains can remove members"));
    }

    state
        .league_team_service
        .remove_member_authorized(team_season_id, target_player_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Leave a team's seasonal roster voluntarily.
#[utoipa::path(
    post,
    path = "/v1/league-team-seasons/{team_season_id}/leave",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID")
    ),
    responses(
        (status = 204, description = "Left the team"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Team season not found or not a member", body = ApiError),
        (status = 409, description = "Last captain cannot leave", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-seasons"
)]
pub async fn leave_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(team_season_id): Path<String>,
) -> ApiResult<StatusCode> {
    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    state
        .league_team_service
        .leave_team(team_season_id, auth.player_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Promote a member to captain (captain only, multiple captains allowed).
#[utoipa::path(
    post,
    path = "/v1/league-team-seasons/{team_season_id}/members/{player_id}/promote",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID"),
        ("player_id" = String, Path, description = "Player ID to promote")
    ),
    responses(
        (status = 200, description = "Member promoted to captain", body = DataResponse<LeagueTeamMemberResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - captain only", body = ApiError),
        (status = 404, description = "Team season or member not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-seasons"
)]
pub async fn promote_to_captain(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((team_season_id, player_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<LeagueTeamMemberResponse>>> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    let target_player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    // Check if the player is a captain
    if !state
        .league_team_service
        .is_captain(team_season_id, auth.player_id)
        .await?
    {
        return Err(ApiError::forbidden("Only captains can promote members"));
    }

    let member = state
        .league_team_service
        .promote_to_captain(team_season_id, target_player_id)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamMemberResponse::from(member),
        request_id,
    )))
}

/// Demote a captain to player (captain only, must keep at least one captain).
#[utoipa::path(
    post,
    path = "/v1/league-team-seasons/{team_season_id}/members/{player_id}/demote",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID"),
        ("player_id" = String, Path, description = "Player ID to demote")
    ),
    responses(
        (status = 200, description = "Captain demoted to player", body = DataResponse<LeagueTeamMemberResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - captain only", body = ApiError),
        (status = 404, description = "Team season or member not found", body = ApiError),
        (status = 409, description = "Cannot demote last captain", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-seasons"
)]
pub async fn demote_from_captain(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((team_season_id, player_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<LeagueTeamMemberResponse>>> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    let target_player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    // Check if the player is a captain
    if !state
        .league_team_service
        .is_captain(team_season_id, auth.player_id)
        .await?
    {
        return Err(ApiError::forbidden("Only captains can demote members"));
    }

    let member = state
        .league_team_service
        .demote_from_captain(team_season_id, target_player_id)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamMemberResponse::from(member),
        request_id,
    )))
}

// =============================================================================
// PLAYER MEMBERSHIP HANDLERS
// =============================================================================

/// Get the current player's league team memberships.
#[utoipa::path(
    get,
    path = "/v1/players/me/league-teams",
    responses(
        (status = 200, description = "Player's league team memberships", body = DataResponse<Vec<PlayerLeagueTeamMembershipResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "players"
)]
pub async fn get_my_league_teams(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<PlayerLeagueTeamMembershipResponse>>>> {
    let request_id = get_request_id(&headers);

    let memberships = state
        .league_team_service
        .get_player_memberships(auth.player_id)
        .await?;

    Ok(Json(DataResponse::new(
        memberships
            .into_iter()
            .map(PlayerLeagueTeamMembershipResponse::from)
            .collect(),
        request_id,
    )))
}

/// Get a player's league team memberships.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/league-teams",
    params(
        ("player_id" = String, Path, description = "Player ID")
    ),
    responses(
        (status = 200, description = "Player's league team memberships", body = DataResponse<Vec<PlayerLeagueTeamMembershipResponse>>),
        (status = 404, description = "Player not found", body = ApiError),
    ),
    tag = "players"
)]
pub async fn get_player_league_teams(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(player_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<PlayerLeagueTeamMembershipResponse>>>> {
    let request_id = get_request_id(&headers);

    let player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    let memberships = state
        .league_team_service
        .get_player_memberships(player_id)
        .await?;

    Ok(Json(DataResponse::new(
        memberships
            .into_iter()
            .map(PlayerLeagueTeamMembershipResponse::from)
            .collect(),
        request_id,
    )))
}
