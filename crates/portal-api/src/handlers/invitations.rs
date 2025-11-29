//! Team invitation handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::InvitePlayerRequest;
use crate::dto::responses::{
    InvitationCountResponse, TeamInvitationResponse, TeamInvitationWithTeamResponse,
    TeamMemberResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::TeamInvitationId;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Invite a player to a team.
#[utoipa::path(
    post,
    path = "/v1/teams/{team_id}/invitations",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body = InvitePlayerRequest,
    responses(
        (status = 201, description = "Invitation sent", body = DataResponse<TeamInvitationResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a team captain", body = ApiError),
        (status = 404, description = "Team or player not found", body = ApiError),
        (status = 409, description = "Already a member or invitation pending", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "invitations"
)]
pub async fn invite_player(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    ValidatedJson(req): ValidatedJson<InvitePlayerRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TeamInvitationResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_id = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    let cmd = req.into_command()?;

    let invitation = state
        .invitation_service
        .invite_player(team_id, auth.player_id, cmd)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TeamInvitationResponse::from(invitation),
            request_id,
        )),
    ))
}

/// Get pending invitations for a team (captain only).
#[utoipa::path(
    get,
    path = "/v1/teams/{team_id}/invitations",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    responses(
        (status = 200, description = "List of pending invitations", body = Vec<TeamInvitationResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a team captain", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "invitations"
)]
pub async fn get_team_invitations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(team_id): Path<String>,
) -> ApiResult<Json<Vec<TeamInvitationResponse>>> {
    let team_id = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    let invitations = state
        .invitation_service
        .get_team_invitations(team_id, auth.player_id)
        .await?;

    let response: Vec<TeamInvitationResponse> =
        invitations.into_iter().map(TeamInvitationResponse::from).collect();

    Ok(Json(response))
}

/// Get my pending invitations.
#[utoipa::path(
    get,
    path = "/v1/invitations/me",
    responses(
        (status = 200, description = "List of pending invitations with team details", body = Vec<TeamInvitationWithTeamResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "invitations"
)]
pub async fn get_my_invitations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Json<Vec<TeamInvitationWithTeamResponse>>> {
    let invitations = state
        .invitation_service
        .get_pending_invitations(auth.player_id)
        .await?;

    // Build response with team details for each invitation
    let mut response = Vec::with_capacity(invitations.len());
    for invitation in invitations {
        // Get team info
        let team = state.team_service.get_team(invitation.team_id).await?;

        response.push(TeamInvitationWithTeamResponse {
            invitation: TeamInvitationResponse::from(invitation),
            team_name: team.name,
            team_tag: team.tag,
            team_logo_url: team.logo_url,
        });
    }

    Ok(Json(response))
}

/// Count my pending invitations.
#[utoipa::path(
    get,
    path = "/v1/invitations/me/count",
    responses(
        (status = 200, description = "Count of pending invitations", body = InvitationCountResponse),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "invitations"
)]
pub async fn count_my_invitations(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Json<InvitationCountResponse>> {
    let count = state
        .invitation_service
        .count_pending_invitations(auth.player_id)
        .await?;

    Ok(Json(InvitationCountResponse { count }))
}

/// Accept an invitation.
#[utoipa::path(
    post,
    path = "/v1/invitations/{invitation_id}/accept",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 200, description = "Invitation accepted - returns team member info", body = TeamMemberResponse),
        (status = 400, description = "Invitation expired or invalid", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not your invitation", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "invitations"
)]
pub async fn accept_invitation(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> ApiResult<Json<TeamMemberResponse>> {
    let invitation_id: TeamInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    let member = state
        .invitation_service
        .accept_invitation(invitation_id, auth.player_id)
        .await?;

    Ok(Json(TeamMemberResponse::from(member)))
}

/// Decline an invitation.
#[utoipa::path(
    post,
    path = "/v1/invitations/{invitation_id}/decline",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 200, description = "Invitation declined", body = TeamInvitationResponse),
        (status = 400, description = "Invitation expired or invalid", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not your invitation", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "invitations"
)]
pub async fn decline_invitation(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> ApiResult<Json<TeamInvitationResponse>> {
    let invitation_id: TeamInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    let invitation = state
        .invitation_service
        .decline_invitation(invitation_id, auth.player_id)
        .await?;

    Ok(Json(TeamInvitationResponse::from(invitation)))
}

/// Cancel an invitation (captain only).
#[utoipa::path(
    delete,
    path = "/v1/invitations/{invitation_id}",
    params(
        ("invitation_id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 204, description = "Invitation cancelled"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a team captain", body = ApiError),
        (status = 404, description = "Invitation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "invitations"
)]
pub async fn cancel_invitation(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> ApiResult<StatusCode> {
    let invitation_id: TeamInvitationId = invitation_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid invitation ID format"))?;

    state
        .invitation_service
        .cancel_invitation(invitation_id, auth.player_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
