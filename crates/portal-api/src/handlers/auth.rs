//! Authentication handlers.
//!
//! All endpoints (register, login, refresh, logout, logout-all) take the
//! domain-scoped [`AuthState`] sub-state rather than the full
//! `AppState`.

use crate::dto::common::DataResponse;
use crate::dto::requests::{LoginRequest, RefreshTokenRequest, RegisterRequest};
use crate::dto::responses::{LoginResponse, LogoutResponse, RegisterResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AuthState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Utc};
use portal_core::DomainError;
use portal_db::NewUserRole;
use portal_domain::entities::user::UserStatus;
use portal_domain::repositories::refresh_token::RefreshTokenRepository;
use portal_domain::services::{LoginCommand, RegisterUserCommand};
use portal_domain::{
    generate_access_token_with_expiry, generate_refresh_token, hash_refresh_token,
};

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Name of the httpOnly cookie carrying the refresh token.
const REFRESH_TOKEN_COOKIE: &str = "refresh_token";

/// Build the httpOnly refresh-token cookie set alongside login/refresh
/// responses.
///
/// Scoped to `/v1/auth` so the token is only sent to auth endpoints,
/// `SameSite=Lax` + `HttpOnly` so scripts can never read it. The refresh
/// token is (for now) still returned in the response body as well, for
/// clients that have not migrated to the cookie flow.
fn refresh_token_cookie(raw_refresh: &str, expiry_minutes: i64) -> Cookie<'static> {
    let mut cookie = Cookie::new(REFRESH_TOKEN_COOKIE, raw_refresh.to_owned());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/v1/auth");
    cookie.set_max_age(time::Duration::minutes(expiry_minutes));
    cookie
}

/// Build the cookie that clears the refresh-token cookie on logout.
///
/// Attributes (name, path) must match [`refresh_token_cookie`] or the browser
/// will keep the original cookie alongside the removal.
fn cleared_refresh_token_cookie() -> Cookie<'static> {
    let mut cookie = Cookie::new(REFRESH_TOKEN_COOKIE, "");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/v1/auth");
    cookie.set_max_age(time::Duration::seconds(0));
    cookie
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
    State(state): State<AuthState>,
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

    // Generate access token. Admin status is *not* encoded in the claim —
    // every request that needs it re-checks the DB via PermissionChecker.
    let access_token = generate_access_token_with_expiry(
        user.id.as_uuid(),
        player.id.as_uuid(),
        &user.username,
        &state.jwt_secret,
        state.token_config.access_token_expiry_minutes,
    )?;

    // Generate and store refresh token
    let raw_refresh = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&raw_refresh);
    let refresh_expires =
        Utc::now() + Duration::minutes(state.token_config.refresh_token_expiry_minutes);
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
    State(state): State<AuthState>,
    headers: HeaderMap,
    jar: CookieJar,
    ValidatedJson(req): ValidatedJson<LoginRequest>,
) -> ApiResult<(CookieJar, Json<DataResponse<LoginResponse>>)> {
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

    // Generate token with configurable expiry. No admin claim — the DB is
    // the single source of truth for authz and is re-checked on every
    // request via PermissionChecker.
    let access_token = generate_access_token_with_expiry(
        auth_result.user_id.as_uuid(),
        auth_result.player_id.as_uuid(),
        &auth_result.username,
        &state.jwt_secret,
        state.token_config.access_token_expiry_minutes,
    )?;

    // Generate and store refresh token
    let raw_refresh = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&raw_refresh);
    let refresh_expires =
        Utc::now() + Duration::minutes(state.token_config.refresh_token_expiry_minutes);
    state
        .refresh_token_repo
        .create(
            auth_result.user_id.as_uuid(),
            &refresh_hash,
            refresh_expires,
        )
        .await?;

    // Also set the refresh token as an httpOnly cookie (the body keeps
    // returning it for backward compatibility).
    let jar = jar.add(refresh_token_cookie(
        &raw_refresh,
        state.token_config.refresh_token_expiry_minutes,
    ));

    let response = LoginResponse {
        access_token,
        refresh_token: raw_refresh,
        user_id: auth_result.user_id.to_string(),
        player_id: auth_result.player_id.to_string(),
        username: auth_result.username,
    };

    Ok((jar, Json(DataResponse::new(response, request_id))))
}

/// Refresh an access token using a refresh token.
///
/// Validates the refresh token, revokes it (rotation), and issues a new access + refresh token pair.
/// Does NOT require an Authorization header — the refresh token itself is the credential.
/// The token is read from the request body when present, otherwise from the
/// `refresh_token` httpOnly cookie set by login/refresh.
#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, description = "Token refreshed successfully", body = DataResponse<LoginResponse>),
        (status = 401, description = "Invalid or expired refresh token", body = ApiError),
        (status = 403, description = "Account is not active (banned/suspended)", body = ApiError),
    ),
    tag = "auth"
)]
pub async fn refresh(
    State(state): State<AuthState>,
    headers: HeaderMap,
    jar: CookieJar,
    ValidatedJson(req): ValidatedJson<RefreshTokenRequest>,
) -> ApiResult<(CookieJar, Json<DataResponse<LoginResponse>>)> {
    let request_id = get_request_id(&headers);

    // Prefer the token from the body (backward compat with pre-cookie
    // clients); fall back to the httpOnly cookie.
    let refresh_token = req
        .refresh_token
        .or_else(|| jar.get(REFRESH_TOKEN_COOKIE).map(|c| c.value().to_owned()))
        .ok_or_else(|| {
            ApiError::bad_request("Missing refresh token (provide it in the body or cookie)")
        })?;

    // Hash incoming token and look it up regardless of revoked state so we
    // can distinguish "never existed" from "already revoked" (replay).
    let token_hash = hash_refresh_token(&refresh_token);
    let stored = state
        .refresh_token_repo
        .find_by_hash(&token_hash)
        .await?
        .ok_or(DomainError::RefreshTokenRevoked)?;

    // Replay detection. A hash that matches a *revoked* row means either:
    //   (a) the legitimate client retried with a token they already rotated,
    //   (b) someone stole the old token and is using it after rotation.
    // We can't tell (a) from (b), and (b) is the serious case — revoke every
    // active token for the user so the attacker's stolen token chain dies
    // and the user is forced to reauthenticate.
    if stored.revoked_at.is_some() {
        tracing::warn!(
            user_id = %stored.user_id,
            token_id = %stored.id,
            "refresh token replay detected; revoking all tokens for user"
        );
        // Best effort — we still want to return 401 even if this fails.
        if let Err(e) = state
            .refresh_token_repo
            .revoke_all_for_user(stored.user_id)
            .await
        {
            tracing::error!(error = %e, user_id = %stored.user_id, "failed to revoke all tokens on replay");
        }
        return Err(DomainError::RefreshTokenRevoked.into());
    }

    // Validate expiry
    if stored.is_expired() {
        // Revoke the expired token for hygiene
        let _ = state.refresh_token_repo.revoke(stored.id).await;
        return Err(DomainError::RefreshTokenExpired.into());
    }

    // Race-safe rotation. try_revoke returns false if another concurrent
    // refresh already consumed this token; that racer gets the new session
    // and we fail. Preserves one-use semantics even under overlap.
    let revoked = state.refresh_token_repo.try_revoke(stored.id).await?;
    if !revoked {
        tracing::warn!(
            user_id = %stored.user_id,
            token_id = %stored.id,
            "refresh token rotation race — another request already consumed this token"
        );
        return Err(DomainError::RefreshTokenRevoked.into());
    }

    // Look up user to get current info for the new JWT
    let user = state.user_service.get_user(stored.user_id.into()).await?;

    // Account-status gate: a banned/suspended user must not be able to
    // keep a session alive by rotating refresh tokens. Kill the whole
    // chain so residual access is capped at the access-token lifetime.
    if user.status != UserStatus::Active {
        if let Err(e) = state
            .refresh_token_repo
            .revoke_all_for_user(stored.user_id)
            .await
        {
            tracing::error!(
                error = %e,
                user_id = %stored.user_id,
                "failed to revoke tokens for non-active account on refresh"
            );
        }
        return Err(DomainError::Forbidden(format!("account is {}", user.status)).into());
    }

    // Look up player for the user
    let player = state
        .player_service
        .get_player_by_user_id(stored.user_id.into())
        .await?;

    // Issue new access token. No admin claim — see comments on login().
    let access_token = generate_access_token_with_expiry(
        user.id.as_uuid(),
        player.id.as_uuid(),
        &user.username,
        &state.jwt_secret,
        state.token_config.access_token_expiry_minutes,
    )?;

    // Issue new refresh token (rotation)
    let raw_refresh = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&raw_refresh);
    let refresh_expires =
        Utc::now() + Duration::minutes(state.token_config.refresh_token_expiry_minutes);
    state
        .refresh_token_repo
        .create(user.id.as_uuid(), &refresh_hash, refresh_expires)
        .await?;

    // Rotate the httpOnly cookie alongside the body token.
    let jar = jar.add(refresh_token_cookie(
        &raw_refresh,
        state.token_config.refresh_token_expiry_minutes,
    ));

    let response = LoginResponse {
        access_token,
        refresh_token: raw_refresh,
        user_id: user.id.to_string(),
        player_id: player.id.to_string(),
        username: user.username,
    };

    Ok((jar, Json(DataResponse::new(response, request_id))))
}

/// Log out: revoke the presented refresh token.
///
/// The refresh token is the credential here, exactly as for `refresh` — no
/// `Authorization` header is required, so a client whose access token has
/// already expired can still terminate its session. The token is read from
/// the body when present, otherwise from the `refresh_token` httpOnly cookie.
///
/// Idempotent and non-enumerable: an unknown, already-revoked or expired
/// token still returns 200 so this endpoint cannot be used to probe which
/// token values exist.
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, description = "Session revoked", body = DataResponse<LogoutResponse>),
        (status = 400, description = "No refresh token supplied", body = ApiError),
    ),
    tag = "auth"
)]
pub async fn logout(
    State(state): State<AuthState>,
    headers: HeaderMap,
    jar: CookieJar,
    ValidatedJson(req): ValidatedJson<RefreshTokenRequest>,
) -> ApiResult<(CookieJar, Json<DataResponse<LogoutResponse>>)> {
    let request_id = get_request_id(&headers);

    let refresh_token = req
        .refresh_token
        .or_else(|| jar.get(REFRESH_TOKEN_COOKIE).map(|c| c.value().to_owned()))
        .ok_or_else(|| {
            ApiError::bad_request("Missing refresh token (provide it in the body or cookie)")
        })?;

    // Look the token up regardless of revoked state; `revoke` is idempotent.
    // Failures to find it are deliberately not surfaced to the caller.
    let token_hash = hash_refresh_token(&refresh_token);
    if let Some(stored) = state.refresh_token_repo.find_by_hash(&token_hash).await?
        && stored.revoked_at.is_none()
    {
        state.refresh_token_repo.revoke(stored.id).await?;
        tracing::info!(user_id = %stored.user_id, "refresh token revoked via logout");
    }

    // Clear the httpOnly cookie so a cookie-flow client is fully signed out.
    let jar = jar.add(cleared_refresh_token_cookie());

    Ok((
        jar,
        Json(DataResponse::new(
            LogoutResponse { logged_out: true },
            request_id,
        )),
    ))
}

/// Log out everywhere: revoke every refresh token for the authenticated user.
///
/// Requires a valid access token — unlike `logout`, this affects sessions the
/// caller did not present a token for, so it must be bound to the user
/// identity rather than to a single token value. Use after a suspected
/// compromise; residual access is then capped at the 15-minute access-token
/// lifetime.
#[utoipa::path(
    post,
    path = "/v1/auth/logout-all",
    responses(
        (status = 200, description = "All sessions revoked", body = DataResponse<LogoutResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "auth"
)]
pub async fn logout_all(
    State(state): State<AuthState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<DataResponse<LogoutResponse>>)> {
    let request_id = get_request_id(&headers);

    state
        .refresh_token_repo
        .revoke_all_for_user(auth.user_id.as_uuid())
        .await?;
    tracing::info!(user_id = %auth.user_id, "all refresh tokens revoked via logout-all");

    let jar = jar.add(cleared_refresh_token_cookie());

    Ok((
        jar,
        Json(DataResponse::new(
            LogoutResponse { logged_out: true },
            request_id,
        )),
    ))
}
