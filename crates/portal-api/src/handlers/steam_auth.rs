//! "Sign in through Steam" handlers (OpenID 2.0).
//!
//! Two browser-redirect endpoints:
//!
//! * `GET /v1/auth/steam/login` — 302 to Steam's OpenID endpoint with the
//!   standard `checkid_setup` parameters.
//! * `GET /v1/auth/steam/callback` — receives Steam's signed assertion,
//!   verifies it directly with Steam (`check_authentication`), finds or
//!   provisions the matching account, then 302s back to the frontend with
//!   the token pair in the URL *fragment* (never the query string, so
//!   tokens stay out of server logs).
//!
//! The outbound verification call goes through the
//! [`crate::steam_openid::SteamOpenIdVerifier`] seam on [`AuthState`] so
//! integration tests can drive the callback without network access.

use crate::error::{ApiError, ApiResult};
use crate::state::AuthState;
use crate::steam_openid::{
    OPENID_IDENTIFIER_SELECT, OPENID_NS, STEAM_OPENID_ENDPOINT, parse_steam_id_from_claimed_id,
};
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{Duration, Utc};
use portal_db::NewUserRole;
use portal_domain::repositories::refresh_token::RefreshTokenRepository;
use portal_domain::{
    generate_access_token_with_expiry, generate_refresh_token, hash_refresh_token,
};
use std::collections::HashMap;

/// Build a 302 redirect response.
fn found(location: &str) -> ApiResult<Response> {
    let location = HeaderValue::from_str(location)
        .map_err(|_| ApiError::internal("invalid redirect location"))?;
    Ok((StatusCode::FOUND, [(header::LOCATION, location)]).into_response())
}

/// Begin Steam sign-in.
///
/// Redirects the browser to Steam's OpenID 2.0 endpoint
/// (`checkid_setup` with `identifier_select`). Steam authenticates the
/// user and sends them back to `/v1/auth/steam/callback`.
#[utoipa::path(
    get,
    path = "/v1/auth/steam/login",
    responses(
        (status = 302, description = "Redirect to https://steamcommunity.com/openid/login"),
    ),
    tag = "auth"
)]
pub async fn steam_login(State(state): State<AuthState>) -> ApiResult<Response> {
    let return_to = state.steam_auth_config.return_to_url();
    let realm = &state.steam_auth_config.public_url;

    let url = reqwest::Url::parse_with_params(
        STEAM_OPENID_ENDPOINT,
        &[
            ("openid.ns", OPENID_NS),
            ("openid.mode", "checkid_setup"),
            ("openid.identity", OPENID_IDENTIFIER_SELECT),
            ("openid.claimed_id", OPENID_IDENTIFIER_SELECT),
            ("openid.return_to", return_to.as_str()),
            ("openid.realm", realm.as_str()),
        ],
    )
    .map_err(|e| ApiError::internal(format!("failed to build Steam login URL: {e}")))?;

    found(url.as_str())
}

/// Complete Steam sign-in.
///
/// Receives Steam's OpenID assertion as query parameters, validates
/// `openid.return_to` against this deployment's public URL, verifies the
/// assertion directly with Steam (`check_authentication`), then finds or
/// creates the account matching the asserted SteamID64 and redirects to
/// `{frontend}/auth/steam/complete` with the access + refresh tokens in
/// the URL fragment.
#[utoipa::path(
    get,
    path = "/v1/auth/steam/callback",
    params(
        ("openid.mode" = String, Query, description = "OpenID response mode (id_res on success)"),
        ("openid.claimed_id" = String, Query, description = "Claimed identity URL carrying the SteamID64"),
        ("openid.return_to" = String, Query, description = "Return-to URL echoed by Steam; must match this deployment"),
    ),
    responses(
        (status = 302, description = "Redirect to the frontend with tokens in the URL fragment"),
        (status = 400, description = "Malformed or cancelled OpenID response", body = ApiError),
        (status = 401, description = "Steam rejected the assertion", body = ApiError),
        (status = 403, description = "Account is not active", body = ApiError),
    ),
    tag = "auth"
)]
// Axum's Query extractor requires a concrete map type, so the generic-
// hasher lint cannot be satisfied here.
#[allow(clippy::implicit_hasher)]
pub async fn steam_callback(
    State(state): State<AuthState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let mode = params.get("openid.mode").map(String::as_str);
    if mode == Some("cancel") {
        return Err(ApiError::bad_request("Steam sign-in was cancelled"));
    }
    if mode != Some("id_res") {
        return Err(ApiError::bad_request(
            "Unexpected OpenID response mode from Steam",
        ));
    }

    // (a) The echoed return_to must point at *this* deployment's callback.
    // A mismatch means the assertion was minted for some other realm.
    let expected_return_to = state.steam_auth_config.return_to_url();
    let return_to = params
        .get("openid.return_to")
        .ok_or_else(|| ApiError::bad_request("Missing openid.return_to parameter"))?;
    let return_to_matches = return_to == &expected_return_to
        || return_to
            .strip_prefix(expected_return_to.as_str())
            .is_some_and(|rest| rest.starts_with('?'));
    if !return_to_matches {
        return Err(ApiError::bad_request(
            "openid.return_to does not match this deployment",
        ));
    }

    // (c, checked early so we fail cheap) The claimed id must be a
    // steamcommunity.com identity URL carrying a numeric SteamID64.
    let claimed_id = params
        .get("openid.claimed_id")
        .ok_or_else(|| ApiError::bad_request("Missing openid.claimed_id parameter"))?;
    let steam_id_64 = parse_steam_id_from_claimed_id(claimed_id)
        .ok_or_else(|| ApiError::bad_request("openid.claimed_id is not a Steam identity URL"))?;

    // (b) Direct verification: POST the assertion back to Steam with
    // mode=check_authentication and require is_valid:true.
    let openid_params: Vec<(String, String)> = params
        .iter()
        .filter(|(k, _)| k.starts_with("openid."))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let is_valid = state
        .steam_verifier
        .check_authentication(&openid_params)
        .await?;
    if !is_valid {
        return Err(ApiError::unauthorized(
            "Steam rejected the OpenID assertion",
        ));
    }

    // Optional enrichment: persona name via the Steam Web API. Degrades
    // to None when no API key is configured or the call fails.
    let persona_name = match state.steam_auth_config.api_key.as_deref() {
        Some(api_key) => {
            state
                .steam_verifier
                .fetch_persona_name(api_key, steam_id_64)
                .await
        }
        None => None,
    };

    // Find the account owning this SteamID64, or provision one.
    let (user, player, created) = state
        .user_service
        .login_with_steam(steam_id_64, persona_name.as_deref())
        .await?;

    // Newly provisioned accounts get the default `user` role, exactly
    // like password registration.
    if created {
        if let Ok(Some(default_role)) = state.role_repo.find_by_name("user").await {
            let assignment = NewUserRole {
                user_id: user.id.into(),
                role_id: default_role.id,
                scope_type: None,
                scope_id: None,
                granted_by: None,
                expires_at: None,
            };
            if let Err(e) = state.role_repo.assign_to_user(assignment).await {
                tracing::warn!("Failed to assign default role to Steam user: {e:?}");
            }
        } else {
            tracing::warn!("Default 'user' role not found - skipping role assignment");
        }
    }

    // Issue the same token pair as password login.
    let access_token = generate_access_token_with_expiry(
        user.id.as_uuid(),
        player.id.as_uuid(),
        &user.username,
        &state.jwt_secret,
        state.token_config.access_token_expiry_minutes,
    )?;

    let raw_refresh = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&raw_refresh);
    let refresh_expires =
        Utc::now() + Duration::minutes(state.token_config.refresh_token_expiry_minutes);
    state
        .refresh_token_repo
        .create(user.id.as_uuid(), &refresh_hash, refresh_expires)
        .await?;

    // Tokens travel in the fragment (never the query string) so they
    // don't land in server or proxy logs. Both are URL-safe (base64url
    // JWT, hex refresh token).
    let redirect = format!(
        "{}/auth/steam/complete#access_token={access_token}&refresh_token={raw_refresh}",
        state.steam_auth_config.frontend_url
    );
    found(&redirect)
}
