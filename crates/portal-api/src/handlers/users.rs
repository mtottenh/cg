//! User handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::tournament::MyMatchesQuery;
use crate::dto::responses::{
    ActionItemResponse, TournamentMatchResponse, UserResponse, UserRoleAssignmentResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedUser;
use crate::state::UsersState;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use portal_core::types::TournamentMatchStatus;
use portal_core::TournamentId;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Get the current authenticated user.
#[utoipa::path(
    get,
    path = "/v1/users/me",
    responses(
        (status = 200, description = "Current user", body = DataResponse<UserResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "users"
)]
pub async fn get_current_user(
    State(state): State<UsersState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<UserResponse>>> {
    let request_id = get_request_id(&headers);

    let user = state.user_service.get_current_user(auth.user_id).await?;

    Ok(Json(DataResponse::new(UserResponse::from(user), request_id)))
}

/// Get the current user's role assignments.
#[utoipa::path(
    get,
    path = "/v1/users/me/roles",
    responses(
        (status = 200, description = "Current user's role assignments", body = DataResponse<Vec<UserRoleAssignmentResponse>>),
        (status = 401, description = "Unauthorized", body = crate::error::ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "users"
)]
pub async fn get_my_roles(
    State(state): State<UsersState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<UserRoleAssignmentResponse>>>> {
    let request_id = get_request_id(&headers);
    let user_uuid = auth.user_id.as_uuid();

    let assignments = state
        .role_repo
        .get_user_role_assignments(user_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch user roles: {e}")))?;

    let mut responses: Vec<UserRoleAssignmentResponse> = Vec::new();
    for assignment in assignments {
        if let Ok(Some(role)) = state.role_repo.find_by_id(assignment.role_id).await {
            responses.push(UserRoleAssignmentResponse::new(assignment, role));
        }
    }
    responses.sort_by(|a, b| b.role.priority.cmp(&a.role.priority));

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Get the current user's tournament matches.
#[utoipa::path(
    get,
    path = "/v1/users/me/matches",
    params(MyMatchesQuery),
    responses(
        (status = 200, description = "User's tournament matches", body = DataResponse<Vec<TournamentMatchResponse>>),
        (status = 401, description = "Unauthorized", body = crate::error::ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "users"
)]
pub async fn get_my_matches(
    State(state): State<UsersState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<MyMatchesQuery>,
) -> ApiResult<Json<DataResponse<Vec<TournamentMatchResponse>>>> {
    let request_id = get_request_id(&headers);

    let status_filter = query
        .status
        .as_deref()
        .map(|s| s.parse::<TournamentMatchStatus>())
        .transpose()
        .map_err(|e| ApiError::bad_request(format!("Invalid status: {e}")))?;

    let tournament_id_filter = query
        .tournament_id
        .as_deref()
        .map(|s| s.parse::<TournamentId>())
        .transpose()
        .map_err(|_| ApiError::bad_request("Invalid tournament_id format"))?;

    let limit = query.limit.unwrap_or(50).min(100).max(1);
    let offset = query.offset.unwrap_or(0).max(0);

    let matches = state
        .tournament_service
        .get_player_matches(
            auth.player_id,
            status_filter,
            tournament_id_filter,
            limit,
            offset,
        )
        .await?;

    let data: Vec<TournamentMatchResponse> = matches.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Get the current user's pending action items.
#[utoipa::path(
    get,
    path = "/v1/users/me/action-items",
    responses(
        (status = 200, description = "Pending action items", body = DataResponse<Vec<ActionItemResponse>>),
        (status = 401, description = "Unauthorized", body = crate::error::ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "users"
)]
pub async fn get_my_action_items(
    State(state): State<UsersState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<ActionItemResponse>>>> {
    let request_id = get_request_id(&headers);

    let items = state
        .action_item_repo
        .list_by_player(auth.player_id.as_uuid())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch action items: {e}")))?;

    let data: Vec<ActionItemResponse> = items.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}
