//! User handlers.

use crate::dto::common::DataResponse;
use crate::dto::responses::UserResponse;
use crate::error::ApiResult;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;

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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<UserResponse>>> {
    let request_id = get_request_id(&headers);

    let user = state.user_service.get_current_user(auth.user_id).await?;

    Ok(Json(DataResponse::new(UserResponse::from(user), request_id)))
}
