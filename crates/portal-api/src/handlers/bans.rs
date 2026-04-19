//! Ban handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{CreateBanRequest, LiftBanRequest, ListBansQuery};
use crate::dto::responses::{BanListResponse, BanResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedUser;
use crate::state::BanState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{BanId, UserId};
use portal_domain::entities::{BanFilters, BanType, CreateBanCommand, LiftBanCommand};
use validator::Validate;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// List bans with filtering and pagination.
#[utoipa::path(
    get,
    path = "/v1/admin/bans",
    params(
        ("user_id" = Option<String>, Query, description = "Filter by user ID"),
        ("ban_type" = Option<String>, Query, description = "Filter by ban type"),
        ("scope_type" = Option<String>, Query, description = "Filter by scope type"),
        ("scope_id" = Option<String>, Query, description = "Filter by scope ID"),
        ("active_only" = Option<bool>, Query, description = "Only show active bans"),
        ("page" = Option<i64>, Query, description = "Page number (1-indexed)"),
        ("per_page" = Option<i64>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of bans", body = DataResponse<BanListResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn list_bans(
    State(state): State<BanState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<ListBansQuery>,
) -> ApiResult<Json<DataResponse<BanListResponse>>> {
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

    let filters = BanFilters {
        user_id: query.user_id.map(UserId::from),
        ban_type: query.ban_type.as_ref().and_then(|t| t.parse::<BanType>().ok()),
        active_only: query.active_only,
        scope_type: query.scope_type,
        scope_id: query.scope_id,
    };

    let paginated = state
        .ban_service
        .list_bans(filters, query.page, query.per_page)
        .await?;

    Ok(Json(DataResponse::new(
        BanListResponse::from(paginated),
        request_id,
    )))
}

/// Get a ban by ID.
#[utoipa::path(
    get,
    path = "/v1/admin/bans/{id}",
    params(
        ("id" = String, Path, description = "Ban ID"),
    ),
    responses(
        (status = 200, description = "Ban details", body = DataResponse<BanResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Ban not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn get_ban(
    State(state): State<BanState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<DataResponse<BanResponse>>> {
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

    let ban_id: BanId = id.parse().map_err(|_| ApiError::bad_request("Invalid ban ID"))?;

    let ban = state.ban_service.get_ban(ban_id).await?;

    Ok(Json(DataResponse::new(BanResponse::from(ban), request_id)))
}

/// Create a new ban.
#[utoipa::path(
    post,
    path = "/v1/admin/bans",
    request_body = CreateBanRequest,
    responses(
        (status = 201, description = "Ban created", body = DataResponse<BanResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 409, description = "User already has an active ban of this type", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn create_ban(
    State(state): State<BanState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Json(body): Json<CreateBanRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<BanResponse>>)> {
    let request_id = get_request_id(&headers);

    // Validate request
    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    // Check if user is admin
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let ban_type: BanType = body
        .ban_type
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid ban type"))?;

    let cmd = CreateBanCommand {
        user_id: UserId::from(body.user_id),
        ban_type,
        reason: body.reason,
        scope_type: body.scope_type,
        scope_id: body.scope_id,
        issued_by: Some(auth.user_id),
        starts_at: None, // Starts immediately
        duration_seconds: body.duration_seconds,
    };

    let ban = state.ban_service.create_ban(cmd).await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(BanResponse::from(ban), request_id)),
    ))
}

/// Lift (revoke) a ban.
#[utoipa::path(
    post,
    path = "/v1/admin/bans/{id}/lift",
    params(
        ("id" = String, Path, description = "Ban ID"),
    ),
    request_body = LiftBanRequest,
    responses(
        (status = 200, description = "Ban lifted", body = DataResponse<BanResponse>),
        (status = 400, description = "Validation error or ban already lifted/expired", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Ban not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn lift_ban(
    State(state): State<BanState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<LiftBanRequest>,
) -> ApiResult<Json<DataResponse<BanResponse>>> {
    let request_id = get_request_id(&headers);

    // Validate request
    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    // Check if user is admin
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let ban_id: BanId = id.parse().map_err(|_| ApiError::bad_request("Invalid ban ID"))?;

    let cmd = LiftBanCommand {
        ban_id,
        lifted_by: auth.user_id,
        lift_reason: body.reason,
    };

    let ban = state.ban_service.lift_ban(cmd).await?;

    Ok(Json(DataResponse::new(BanResponse::from(ban), request_id)))
}

/// Get a user's ban history.
#[utoipa::path(
    get,
    path = "/v1/admin/users/{user_id}/bans",
    params(
        ("user_id" = String, Path, description = "User ID"),
    ),
    responses(
        (status = 200, description = "User's ban history", body = DataResponse<Vec<BanResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn get_user_bans(
    State(state): State<BanState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(user_id): Path<UserId>,
) -> ApiResult<Json<DataResponse<Vec<BanResponse>>>> {
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

    let bans = state.ban_service.get_user_ban_history(user_id).await?;

    let responses: Vec<BanResponse> = bans.into_iter().map(BanResponse::from).collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}
