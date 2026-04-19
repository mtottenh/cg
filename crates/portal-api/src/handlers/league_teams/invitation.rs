//! League team invitation handlers.

use super::{get_request_id, require_captain_or_admin};
use crate::dto::common::DataResponse;
use crate::dto::requests::{
    ApplyToLeagueTeamRequest, InviteToLeagueTeamRequest, RespondToInvitationRequest,
};
use crate::dto::responses::{
    LeagueTeamInvitationResponse, LeagueTeamInvitationWithTeamResponse, LeagueTeamMemberResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{LeagueTeamInvitationId, LeagueTeamSeasonId};

/// Invite a player to join a team's seasonal roster.
#[utoipa::path(
    post,
    path = "/v1/league-team-seasons/{team_season_id}/invitations",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID")
    ),
    request_body = InviteToLeagueTeamRequest,
    responses(
        (status = 201, description = "Invitation sent", body = DataResponse<LeagueTeamInvitationResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - captain only", body = ApiError),
        (status = 404, description = "Team season or player not found", body = ApiError),
        (status = 409, description = "Already invited or already a member", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-invitations"
)]
pub async fn invite_to_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_season_id): Path<String>,
    ValidatedJson(req): ValidatedJson<InviteToLeagueTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueTeamInvitationResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    require_captain_or_admin(&state, &perm, &auth, team_season_id, "send invitations").await?;

    let cmd = req.into_command(team_season_id)?;
    let invitation = state
        .league_team_invitation_service
        .create_invitation(team_season_id, cmd.player_id, cmd.role, cmd.message, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(LeagueTeamInvitationResponse::from(invitation), request_id)),
    ))
}

/// Apply to join a team's seasonal roster.
#[utoipa::path(
    post,
    path = "/v1/league-team-seasons/{team_season_id}/apply",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID")
    ),
    request_body = ApplyToLeagueTeamRequest,
    responses(
        (status = 201, description = "Application sent", body = DataResponse<LeagueTeamInvitationResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Team season not found", body = ApiError),
        (status = 409, description = "Already applied or already a member", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-invitations"
)]
pub async fn apply_to_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(team_season_id): Path<String>,
    ValidatedJson(req): ValidatedJson<ApplyToLeagueTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<LeagueTeamInvitationResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    let cmd = req.into_command(team_season_id, auth.player_id)?;
    let invitation = state
        .league_team_invitation_service
        .create_join_request(team_season_id, auth.player_id, cmd.role, cmd.message)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(LeagueTeamInvitationResponse::from(invitation), request_id)),
    ))
}

/// Get pending invitations for the current player.
#[utoipa::path(
    get,
    path = "/v1/league-team-invitations/me",
    operation_id = "get_my_team_invitations",
    responses(
        (status = 200, description = "Player's pending invitations", body = DataResponse<Vec<LeagueTeamInvitationWithTeamResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-invitations"
)]
pub async fn get_my_invitations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<LeagueTeamInvitationWithTeamResponse>>>> {
    let request_id = get_request_id(&headers);

    let invitations = state
        .league_team_invitation_service
        .get_player_invitations(auth.player_id)
        .await?;

    Ok(Json(DataResponse::new(
        invitations
            .into_iter()
            .map(LeagueTeamInvitationWithTeamResponse::from)
            .collect(),
        request_id,
    )))
}

/// Get pending invitations for a team season.
#[utoipa::path(
    get,
    path = "/v1/league-team-seasons/{team_season_id}/invitations",
    params(
        ("team_season_id" = String, Path, description = "Team Season ID")
    ),
    responses(
        (status = 200, description = "Team's pending invitations", body = DataResponse<Vec<LeagueTeamInvitationResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - captain only", body = ApiError),
        (status = 404, description = "Team season not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-invitations"
)]
pub async fn get_team_invitations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_season_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<LeagueTeamInvitationResponse>>>> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = team_season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team season ID format"))?;

    require_captain_or_admin(&state, &perm, &auth, team_season_id, "view team invitations").await?;

    let invitations = state
        .league_team_invitation_service
        .get_team_invitations(team_season_id)
        .await?;

    Ok(Json(DataResponse::new(
        invitations
            .into_iter()
            .map(LeagueTeamInvitationResponse::from)
            .collect(),
        request_id,
    )))
}

/// Accept an invitation.
#[utoipa::path(
    post,
    path = "/v1/league-team-invitations/{invitation_id}/accept",
    operation_id = "accept_team_invitation",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 200, description = "Invitation accepted", body = DataResponse<LeagueTeamMemberResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - not your invitation", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
        (status = 409, description = "Invitation expired or already responded", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-invitations"
)]
pub async fn accept_invitation(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(invitation_id): Path<String>,
) -> ApiResult<Json<DataResponse<LeagueTeamMemberResponse>>> {
    let request_id = get_request_id(&headers);

    let invitation_id: LeagueTeamInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    let member = state
        .league_team_invitation_service
        .accept_invitation(invitation_id, auth.player_id)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamMemberResponse::from(member),
        request_id,
    )))
}

/// Decline an invitation.
#[utoipa::path(
    post,
    path = "/v1/league-team-invitations/{invitation_id}/decline",
    operation_id = "decline_team_invitation",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    request_body = RespondToInvitationRequest,
    responses(
        (status = 204, description = "Invitation declined"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - not authorized", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
        (status = 409, description = "Invitation expired or already responded", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-invitations"
)]
pub async fn decline_invitation(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<String>,
    ValidatedJson(req): ValidatedJson<RespondToInvitationRequest>,
) -> ApiResult<StatusCode> {
    let invitation_id: LeagueTeamInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    state
        .league_team_invitation_service
        .decline_invitation(invitation_id, auth.player_id, req.message)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Cancel an invitation (captain only).
#[utoipa::path(
    delete,
    path = "/v1/league-team-invitations/{invitation_id}",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 204, description = "Invitation cancelled"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - captain only", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-team-invitations"
)]
pub async fn cancel_invitation(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> ApiResult<StatusCode> {
    let invitation_id: LeagueTeamInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    state
        .league_team_invitation_service
        .cancel_invitation(invitation_id, auth.player_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
