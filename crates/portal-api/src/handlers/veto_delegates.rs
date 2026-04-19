//! Veto delegate handlers.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{LeagueTeamSeasonId, PlayerId, TournamentId, VetoDelegateId};

use crate::dto::common::DataResponse;
use crate::dto::requests::CreateVetoDelegateRequest;
use crate::dto::responses::{VetoDelegateListResponse, VetoDelegateResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::VetoDelegatesState;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// VETO DELEGATE ENDPOINTS
// =============================================================================

/// Create a veto delegation for a team.
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/teams/{team_id}/seasons/{season_id}/veto-delegates",
    request_body = CreateVetoDelegateRequest,
    params(
        ("league_id" = String, Path, description = "League ID"),
        ("team_id" = String, Path, description = "Team ID"),
        ("season_id" = String, Path, description = "Season ID"),
    ),
    responses(
        (status = 201, description = "Delegation created", body = DataResponse<VetoDelegateResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized to create delegations", body = ApiError),
        (status = 404, description = "Team season not found", body = ApiError),
        (status = 409, description = "Player already has an active delegation", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto_delegates"
)]
pub async fn create_delegation(
    State(state): State<VetoDelegatesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_league_id, _team_id, season_id)): Path<(String, String, String)>,
    ValidatedJson(req): ValidatedJson<CreateVetoDelegateRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<VetoDelegateResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid season ID format"))?;

    let delegate_player_id: PlayerId = req
        .player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    let tournament_id: Option<TournamentId> = req
        .tournament_id
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))
        })
        .transpose()?;

    // Create the delegation (authorization check happens in the service)
    let delegate = state
        .veto_authorization_service
        .create_delegation(
            team_season_id,
            delegate_player_id,
            auth.user_id,
            auth.player_id,
            tournament_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            VetoDelegateResponse::from(delegate),
            request_id,
        )),
    ))
}

/// List active veto delegations for a team.
#[utoipa::path(
    get,
    path = "/v1/leagues/{league_id}/teams/{team_id}/seasons/{season_id}/veto-delegates",
    params(
        ("league_id" = String, Path, description = "League ID"),
        ("team_id" = String, Path, description = "Team ID"),
        ("season_id" = String, Path, description = "Season ID"),
    ),
    responses(
        (status = 200, description = "List of active delegations", body = DataResponse<VetoDelegateListResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Team season not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto_delegates"
)]
pub async fn list_delegations(
    State(state): State<VetoDelegatesState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_league_id, _team_id, season_id)): Path<(String, String, String)>,
) -> ApiResult<Json<DataResponse<VetoDelegateListResponse>>> {
    let request_id = get_request_id(&headers);

    let team_season_id: LeagueTeamSeasonId = season_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid season ID format"))?;

    let delegates = state
        .veto_authorization_service
        .list_delegations(team_season_id)
        .await?;

    let response = VetoDelegateListResponse {
        delegates: delegates.into_iter().map(VetoDelegateResponse::from).collect(),
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Revoke a veto delegation.
#[utoipa::path(
    delete,
    path = "/v1/leagues/{league_id}/teams/{team_id}/seasons/{season_id}/veto-delegates/{delegate_id}",
    params(
        ("league_id" = String, Path, description = "League ID"),
        ("team_id" = String, Path, description = "Team ID"),
        ("season_id" = String, Path, description = "Season ID"),
        ("delegate_id" = String, Path, description = "Delegate ID"),
    ),
    responses(
        (status = 200, description = "Delegation revoked", body = DataResponse<VetoDelegateResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized to revoke this delegation", body = ApiError),
        (status = 404, description = "Delegation not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "veto_delegates"
)]
pub async fn revoke_delegation(
    State(state): State<VetoDelegatesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_league_id, _team_id, _season_id, delegate_id)): Path<(String, String, String, String)>,
) -> ApiResult<Json<DataResponse<VetoDelegateResponse>>> {
    let request_id = get_request_id(&headers);

    let delegate_id: VetoDelegateId = delegate_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid delegate ID format"))?;

    // Revoke the delegation (authorization check happens in the service)
    let delegate = state
        .veto_authorization_service
        .revoke_delegation(delegate_id, auth.user_id, auth.player_id)
        .await?;

    Ok(Json(DataResponse::new(
        VetoDelegateResponse::from(delegate),
        request_id,
    )))
}
