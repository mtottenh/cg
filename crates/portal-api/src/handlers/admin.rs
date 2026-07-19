//! Admin handlers.

use crate::dto::common::DataResponse;
use crate::dto::responses::PlatformStatsResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedUser;
use crate::state::AdminState;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Get platform statistics for admin dashboard.
#[utoipa::path(
    get,
    path = "/v1/admin/stats",
    responses(
        (status = 200, description = "Platform statistics", body = DataResponse<PlatformStatsResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn get_stats(
    State(state): State<AdminState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<PlatformStatsResponse>>> {
    let request_id = get_request_id(&headers);

    // Check if user is admin
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    // Get stats from repository
    let stats = state
        .stats_repo
        .get_platform_stats()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch stats: {e}")))?;

    let response = PlatformStatsResponse {
        total_users: stats.total_users as u64,
        total_players: stats.total_players as u64,
        total_teams: stats.total_teams as u64,
        active_games: stats.active_games as u64,
        active_bans: stats.active_bans as u64,
        users_last_24h: stats.users_last_24h as u64,
        users_last_7d: stats.users_last_7d as u64,
        teams_last_7d: stats.teams_last_7d as u64,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}
