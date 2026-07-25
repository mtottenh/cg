//! Mid-series substitution endpoints (§6.8).

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::game_server_flow;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::ids::{MatchSubstitutionId, PlayerId, TournamentMatchId};
use portal_core::permissions;
use portal_core::types::SubstitutionStatus;
use portal_domain::entities::MatchSubstitution;
use portal_domain::repositories::MatchSubstitutionRepository;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Request a mid-series substitution.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateSubstitutionRequest {
    /// Player leaving the match.
    pub player_out_id: String,
    /// Player coming in; omit to play short-handed.
    pub player_in_id: Option<String>,
}

/// A substitution request and its lifecycle state.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubstitutionResponse {
    pub id: String,
    pub match_id: String,
    pub registration_id: String,
    pub player_out_id: String,
    pub player_in_id: Option<String>,
    /// First game the substitution applies from (1-indexed).
    pub from_game_number: i32,
    pub status: SubstitutionStatus,
    pub requested_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    pub created_at: String,
}

impl From<MatchSubstitution> for SubstitutionResponse {
    fn from(s: MatchSubstitution) -> Self {
        Self {
            id: s.id.to_string(),
            match_id: s.match_id.to_string(),
            registration_id: s.registration_id.to_string(),
            player_out_id: s.player_out_id.to_string(),
            player_in_id: s.player_in_id.map(|p| p.to_string()),
            from_game_number: s.from_game_number,
            status: s.status,
            requested_by: s.requested_by.to_string(),
            failure_reason: s.failure_reason,
            applied_at: s.applied_at.map(|at| at.to_rfc3339()),
            created_at: s.created_at.to_rfc3339(),
        }
    }
}

async fn require_match_participant(
    state: &AppState,
    match_id: TournamentMatchId,
    auth: &AuthenticatedUser,
    perm_checker: &PermissionChecker,
) -> Result<(), ApiError> {
    if perm_checker
        .has_permission(auth, permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await
        || super::match_server::is_participant(state, match_id, auth).await?
    {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "only match participants and admins may view this",
        ))
    }
}

fn parse_match_id(raw: &str) -> Result<TournamentMatchId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))
}

/// Request a substitution (captain / delegate; §6.8).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/substitutions",
    params(("match_id" = String, Path, description = "Match ID")),
    request_body = CreateSubstitutionRequest,
    responses(
        (status = 201, description = "Substitution requested", body = DataResponse<SubstitutionResponse>),
        (status = 400, description = "Not substitutable", body = ApiError),
        (status = 403, description = "Not the captain/delegate", body = ApiError),
        (status = 409, description = "Already in flight", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn create_substitution(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Json(body): Json<CreateSubstitutionRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<SubstitutionResponse>>)> {
    let request_id = get_request_id(&headers);
    let match_id = parse_match_id(&match_id)?;
    let player_out: PlayerId = body
        .player_out_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid player_out_id"))?;
    let player_in: Option<PlayerId> = body
        .player_in_id
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|_| ApiError::bad_request("invalid player_in_id"))?;

    let substitution = game_server_flow::request_substitution(
        &state,
        match_id,
        auth.user_id,
        auth.player_id,
        player_out,
        player_in,
    )
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            SubstitutionResponse::from(substitution),
            request_id,
        )),
    ))
}

/// List a match's substitutions.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/substitutions",
    params(("match_id" = String, Path, description = "Match ID")),
    responses(
        (status = 200, description = "Substitutions", body = DataResponse<Vec<SubstitutionResponse>>),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn list_substitutions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<SubstitutionResponse>>>> {
    let request_id = get_request_id(&headers);
    let match_id = parse_match_id(&match_id)?;
    require_match_participant(&state, match_id, &auth, &perm_checker).await?;
    let subs = state
        .match_substitution_repo
        .list_by_match(match_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(DataResponse::new(
        subs.into_iter().map(SubstitutionResponse::from).collect(),
        request_id,
    )))
}

/// Cancel a substitution before it applies (requester or admin).
#[utoipa::path(
    delete,
    path = "/v1/matches/{match_id}/substitutions/{substitution_id}",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("substitution_id" = String, Path, description = "Substitution ID"),
    ),
    responses(
        (status = 204, description = "Cancelled"),
        (status = 400, description = "Already applied", body = ApiError),
        (status = 403, description = "Not the requester", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn cancel_substitution(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((_match_id, substitution_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let id: MatchSubstitutionId = substitution_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid substitution id"))?;
    let substitution = state
        .match_substitution_repo
        .find_by_id(id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("substitution not found"))?;

    let is_admin = perm_checker
        .has_permission(&auth, permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await;
    if substitution.requested_by != auth.user_id && !is_admin {
        return Err(ApiError::forbidden("only the requester can cancel"));
    }
    if matches!(
        substitution.status,
        SubstitutionStatus::Applied | SubstitutionStatus::Failed
    ) {
        return Err(ApiError::bad_request(
            "the substitution has already been applied",
        ));
    }
    state
        .match_substitution_repo
        .set_status(id, SubstitutionStatus::Cancelled, None, None)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Approve a pending substitution (admin; `admin_approval` policy).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/substitutions/{substitution_id}/approve",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("substitution_id" = String, Path, description = "Substitution ID"),
    ),
    responses(
        (status = 200, description = "Approved and applying", body = DataResponse<SubstitutionResponse>),
        (status = 403, description = "Missing admin permission", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn approve_substitution(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_match_id, substitution_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<SubstitutionResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;
    let request_id = get_request_id(&headers);
    let id: MatchSubstitutionId = substitution_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid substitution id"))?;
    let substitution = state
        .match_substitution_repo
        .find_by_id(id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("substitution not found"))?;
    if substitution.status != SubstitutionStatus::AwaitingApproval {
        return Err(ApiError::bad_request(
            "substitution is not awaiting approval",
        ));
    }
    state
        .match_substitution_repo
        .set_status(id, SubstitutionStatus::Pending, None, Some(auth.user_id))
        .await
        .map_err(ApiError::from)?;

    game_server_flow::approve_and_apply(&state, id)
        .await
        .map_err(ApiError::from)?;

    let substitution = state
        .match_substitution_repo
        .find_by_id(id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("substitution not found"))?;
    Ok(Json(DataResponse::new(
        SubstitutionResponse::from(substitution),
        request_id,
    )))
}

/// Reject a pending substitution (admin).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/substitutions/{substitution_id}/reject",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("substitution_id" = String, Path, description = "Substitution ID"),
    ),
    responses(
        (status = 204, description = "Rejected"),
        (status = 403, description = "Missing admin permission", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn reject_substitution(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path((_match_id, substitution_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    perm_checker
        .require_permission(&auth, permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;
    let id: MatchSubstitutionId = substitution_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid substitution id"))?;
    let substitution = state
        .match_substitution_repo
        .find_by_id(id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("substitution not found"))?;
    // An applied substitution is history — it cannot be "rejected" while
    // the roster edit stands (review minor).
    if !matches!(
        substitution.status,
        SubstitutionStatus::Pending
            | SubstitutionStatus::AwaitingApproval
            | SubstitutionStatus::Applying
    ) {
        return Err(ApiError::bad_request(
            "only pending/awaiting/applying substitutions can be rejected",
        ));
    }
    state
        .match_substitution_repo
        .set_status(id, SubstitutionStatus::Rejected, None, Some(auth.user_id))
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// A player option for the substitution picker.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubstitutionPlayerOption {
    pub player_id: String,
    pub display_name: String,
    /// Bench players without a linked Steam ID cannot come in.
    pub has_steam: bool,
}

/// One side's roster options.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubstitutionOptionsSide {
    pub registration_id: String,
    pub participant_name: String,
    /// Players currently listed on the server.
    pub active: Vec<SubstitutionPlayerOption>,
    /// Rostered players not currently listed.
    pub bench: Vec<SubstitutionPlayerOption>,
}

/// Roster options for the substitution picker.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/substitutions/options",
    params(("match_id" = String, Path, description = "Match ID")),
    responses(
        (status = 200, description = "Per-side options", body = DataResponse<Vec<SubstitutionOptionsSide>>),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn substitution_options(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<SubstitutionOptionsSide>>>> {
    let request_id = get_request_id(&headers);
    let match_id = parse_match_id(&match_id)?;
    // Roster + bench enumeration is participant/admin-only (review minor).
    require_match_participant(&state, match_id, &auth, &perm_checker).await?;
    let sides = game_server_flow::substitution_options(&state, match_id)
        .await
        .map_err(ApiError::from)?;
    let response = sides
        .into_iter()
        .map(|side| SubstitutionOptionsSide {
            registration_id: side.registration_id.to_string(),
            participant_name: side.participant_name,
            active: side
                .active
                .into_iter()
                .map(|(id, name)| SubstitutionPlayerOption {
                    player_id: id.to_string(),
                    display_name: name,
                    has_steam: true,
                })
                .collect(),
            bench: side
                .bench
                .into_iter()
                .map(|(id, name, has_steam)| SubstitutionPlayerOption {
                    player_id: id.to_string(),
                    display_name: name,
                    has_steam,
                })
                .collect(),
        })
        .collect();
    Ok(Json(DataResponse::new(response, request_id)))
}
