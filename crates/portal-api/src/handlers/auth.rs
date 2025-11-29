//! Authentication handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{LoginRequest, RegisterRequest};
use crate::dto::responses::{LoginResponse, RegisterResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::ValidatedJson;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_db::NewUserRole;
use portal_domain::generate_access_token_with_admin;
use portal_domain::services::{LoginCommand, RegisterUserCommand};

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Register a new user.
///
/// Creates a new user account with a player profile and returns an access token.
#[utoipa::path(
    post,
    path = "/v1/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered successfully", body = DataResponse<RegisterResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 409, description = "Username or email already exists", body = ApiError),
    ),
    tag = "auth"
)]
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<RegisterResponse>>)> {
    let request_id = get_request_id(&headers);

    let cmd = RegisterUserCommand {
        username: req.username,
        email: req.email,
        password: req.password,
        display_name: req.display_name,
    };

    let (user, player) = state.user_service.register_user(cmd).await?;

    // Assign default "user" role to the newly registered user
    if let Some(default_role) = state.role_repo.find_by_name("user").await.map_err(|e| {
        tracing::error!("Failed to find default role: {:?}", e);
        ApiError::internal("Failed to assign default role")
    })? {
        let assignment = NewUserRole {
            user_id: user.id.into(),
            role_id: default_role.id,
            scope_type: None,
            scope_id: None,
            granted_by: None,
            expires_at: None,
        };
        if let Err(e) = state.role_repo.assign_to_user(assignment).await {
            tracing::warn!("Failed to assign default role to user: {:?}", e);
            // Don't fail registration if role assignment fails - user can still function
        }
    } else {
        tracing::warn!("Default 'user' role not found - skipping role assignment");
    }

    // Generate access token for the newly registered user (new users are never admin)
    let access_token = generate_access_token_with_admin(
        user.id.as_uuid(),
        player.id.as_uuid(),
        &user.username,
        &state.jwt_secret,
        false, // new users are not admins
    )?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            RegisterResponse::new(user, player, access_token),
            request_id,
        )),
    ))
}

/// Login with username/email and password.
///
/// Authenticates a user and returns an access token.
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = DataResponse<LoginResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Invalid credentials", body = ApiError),
    ),
    tag = "auth"
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<LoginRequest>,
) -> ApiResult<Json<DataResponse<LoginResponse>>> {
    let request_id = get_request_id(&headers);

    let cmd = LoginCommand {
        username_or_email: req.username_or_email,
        password: req.password,
    };

    // Authenticate user (verify credentials)
    let auth_result = state
        .user_service
        .authenticate(cmd, &state.jwt_secret)
        .await?;

    // Check if user is admin for JWT claim
    let is_admin = state
        .permission_service
        .is_admin(auth_result.user_id)
        .await
        .unwrap_or(false);

    // Generate token with admin claim
    let access_token = generate_access_token_with_admin(
        auth_result.user_id.as_uuid(),
        auth_result.player_id.as_uuid(),
        &auth_result.username,
        &state.jwt_secret,
        is_admin,
    )?;

    let response = LoginResponse {
        access_token,
        user_id: auth_result.user_id.to_string(),
        player_id: auth_result.player_id.to_string(),
        username: auth_result.username,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}
