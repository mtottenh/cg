//! Authentication handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{LoginRequest, RefreshTokenRequest, RegisterRequest};
use crate::dto::responses::{LoginResponse, RegisterResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::ValidatedJson;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{Duration, Utc};
use portal_core::DomainError;
use portal_db::NewUserRole;
use portal_domain::repositories::refresh_token::RefreshTokenRepository;
use portal_domain::{generate_access_token_with_admin_and_expiry, generate_refresh_token, hash_refresh_token};
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
    let access_token = generate_access_token_with_admin_and_expiry(
        user.id.as_uuid(),
        player.id.as_uuid(),
        &user.username,
        &state.jwt_secret,
        false, // new users are not admins
        state.token_config.access_token_expiry_minutes,
    )?;

    // Generate and store refresh token
    let raw_refresh = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&raw_refresh);
    let refresh_expires = Utc::now()
        + Duration::minutes(state.token_config.refresh_token_expiry_minutes);
    state
        .refresh_token_repo
        .create(user.id.as_uuid(), &refresh_hash, refresh_expires)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            RegisterResponse::new(user, player, access_token, raw_refresh),
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

    // Generate token with admin claim and configurable expiry
    let access_token = generate_access_token_with_admin_and_expiry(
        auth_result.user_id.as_uuid(),
        auth_result.player_id.as_uuid(),
        &auth_result.username,
        &state.jwt_secret,
        is_admin,
        state.token_config.access_token_expiry_minutes,
    )?;

    // Generate and store refresh token
    let raw_refresh = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&raw_refresh);
    let refresh_expires = Utc::now()
        + Duration::minutes(state.token_config.refresh_token_expiry_minutes);
    state
        .refresh_token_repo
        .create(auth_result.user_id.as_uuid(), &refresh_hash, refresh_expires)
        .await?;

    let response = LoginResponse {
        access_token,
        refresh_token: raw_refresh,
        user_id: auth_result.user_id.to_string(),
        player_id: auth_result.player_id.to_string(),
        username: auth_result.username,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Refresh an access token using a refresh token.
///
/// Validates the refresh token, revokes it (rotation), and issues a new access + refresh token pair.
/// Does NOT require an Authorization header — the refresh token itself is the credential.
#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, description = "Token refreshed successfully", body = DataResponse<LoginResponse>),
        (status = 401, description = "Invalid or expired refresh token", body = ApiError),
    ),
    tag = "auth"
)]
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<RefreshTokenRequest>,
) -> ApiResult<Json<DataResponse<LoginResponse>>> {
    let request_id = get_request_id(&headers);

    // Hash incoming token and look up in DB
    let token_hash = hash_refresh_token(&req.refresh_token);
    let stored = state
        .refresh_token_repo
        .find_active_by_hash(&token_hash)
        .await?
        .ok_or(DomainError::RefreshTokenRevoked)?;

    // Validate expiry
    if stored.is_expired() {
        // Revoke the expired token for hygiene
        let _ = state.refresh_token_repo.revoke(stored.id).await;
        return Err(DomainError::RefreshTokenExpired.into());
    }

    // Revoke old token (rotation — each refresh token can only be used once)
    state.refresh_token_repo.revoke(stored.id).await?;

    // Look up user to get current info for the new JWT
    let user = state
        .user_service
        .get_user(stored.user_id.into())
        .await?;

    // Look up player for the user
    let player = state
        .player_service
        .get_player_by_user_id(stored.user_id.into())
        .await?;

    // Check admin status
    let is_admin = state
        .permission_service
        .is_admin(stored.user_id.into())
        .await
        .unwrap_or(false);

    // Issue new access token
    let access_token = generate_access_token_with_admin_and_expiry(
        user.id.as_uuid(),
        player.id.as_uuid(),
        &user.username,
        &state.jwt_secret,
        is_admin,
        state.token_config.access_token_expiry_minutes,
    )?;

    // Issue new refresh token (rotation)
    let raw_refresh = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&raw_refresh);
    let refresh_expires = Utc::now()
        + Duration::minutes(state.token_config.refresh_token_expiry_minutes);
    state
        .refresh_token_repo
        .create(user.id.as_uuid(), &refresh_hash, refresh_expires)
        .await?;

    let response = LoginResponse {
        access_token,
        refresh_token: raw_refresh,
        user_id: user.id.to_string(),
        player_id: player.id.to_string(),
        username: user.username,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}
