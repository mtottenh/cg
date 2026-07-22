//! Auth API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_core::UserId;
use portal_db::RoleRepository;
use portal_domain::{generate_access_token_with_expiry, validate_token};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_register_user() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "newuser",
                "email": "newuser@example.com",
                "password": "SecurePass123!",
                "display_name": "New User"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["user"]["username"], "newuser");
    assert_eq!(body["data"]["user"]["email"], "newuser@example.com");
    assert_eq!(body["data"]["player"]["display_name"], "New User");
    // User and player should have the same ID
    assert_eq!(
        body["data"]["user"]["id"],
        body["data"]["player"]["user_id"]
    );
}

#[tokio::test]
async fn test_register_duplicate_username() {
    let app = TestApp::new().await;

    // Register first user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "duplicate",
                "email": "first@example.com",
                "password": "SecurePass123!",
                "display_name": "First User"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Try to register with same username
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "duplicate",
                "email": "second@example.com",
                "password": "SecurePass123!",
                "display_name": "Second User"
            }),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_register_duplicate_email() {
    let app = TestApp::new().await;

    // Register first user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "first",
                "email": "duplicate@example.com",
                "password": "SecurePass123!",
                "display_name": "First User"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Try to register with same email
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "second",
                "email": "duplicate@example.com",
                "password": "SecurePass123!",
                "display_name": "Second User"
            }),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_register_validation_errors() {
    let app = TestApp::new().await;

    // Username too short
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "ab",  // Too short
                "email": "test@example.com",
                "password": "SecurePass123!",
                "display_name": "Test User"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Invalid email
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "validuser",
                "email": "not-an-email",
                "password": "SecurePass123!",
                "display_name": "Test User"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Password too short
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "validuser",
                "email": "test@example.com",
                "password": "short",  // Too short
                "display_name": "Test User"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ===========================================
// Registration Token Tests
// ===========================================

#[tokio::test]
async fn test_register_returns_access_token() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "tokenuser",
                "email": "tokenuser@example.com",
                "password": "SecurePass123!",
                "display_name": "Token User"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();

    // Verify access token is returned
    assert!(
        body["data"]["access_token"].is_string(),
        "Registration should return access_token"
    );

    let token = body["data"]["access_token"].as_str().unwrap();
    assert!(!token.is_empty(), "Access token should not be empty");

    // Verify the token is valid JWT with correct claims
    let claims = validate_token(token, "test-jwt-secret").expect("Token should be valid");
    assert_eq!(claims.username, "tokenuser");
    assert_eq!(claims.sub, body["data"]["user"]["id"].as_str().unwrap());
}

#[tokio::test]
async fn test_register_token_can_access_protected_endpoint() {
    let app = TestApp::new().await;

    // Register a user and get token
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "authuser",
                "email": "authuser@example.com",
                "password": "SecurePass123!",
                "display_name": "Auth User"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let token = body["data"]["access_token"].as_str().unwrap();

    // Use the token to access a protected endpoint
    let response = app.get_with_token("/v1/users/me", token).await;
    response.assert_status(StatusCode::OK);

    let user_body: serde_json::Value = response.json();
    assert_eq!(user_body["data"]["username"], "authuser");
}

#[tokio::test]
async fn test_register_assigns_default_role() {
    let app = TestApp::new().await;

    // Register a user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "roleuser",
                "email": "roleuser@example.com",
                "password": "SecurePass123!",
                "display_name": "Role User"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let user_id_str = body["data"]["user"]["id"].as_str().unwrap();
    let user_id = Uuid::parse_str(user_id_str).expect("valid user id");

    // Verify the user has the default "user" role assigned
    let role_repo = RoleRepository::new(app.pool().clone());
    let roles = role_repo
        .get_user_roles(UserId::from(user_id))
        .await
        .expect("should get user roles");

    assert!(!roles.is_empty(), "User should have at least one role");
    assert!(
        roles.iter().any(|r| r.name == "user"),
        "User should have the 'user' role assigned"
    );
}

// ===========================================
// Login Tests
// ===========================================

#[tokio::test]
async fn test_login_with_username() {
    let app = TestApp::new().await;

    // First register a user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "loginuser",
                "email": "loginuser@example.com",
                "password": "SecurePass123!",
                "display_name": "Login User"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let register_body: serde_json::Value = response.json();
    let user_id = register_body["data"]["user"]["id"].as_str().unwrap();

    // Login with username
    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": "loginuser",
                "password": "SecurePass123!"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["access_token"].is_string());
    assert_eq!(body["data"]["user_id"], user_id);
    assert_eq!(body["data"]["username"], "loginuser");
}

#[tokio::test]
async fn test_login_with_email() {
    let app = TestApp::new().await;

    // First register a user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "emaillogin",
                "email": "emaillogin@example.com",
                "password": "SecurePass123!",
                "display_name": "Email Login"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Login with email
    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": "emaillogin@example.com",
                "password": "SecurePass123!"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["access_token"].is_string());
    assert_eq!(body["data"]["username"], "emaillogin");
}

#[tokio::test]
async fn test_login_invalid_password() {
    let app = TestApp::new().await;

    // First register a user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "wrongpass",
                "email": "wrongpass@example.com",
                "password": "SecurePass123!",
                "display_name": "Wrong Pass"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Login with wrong password
    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": "wrongpass",
                "password": "WrongPassword123!"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": "nonexistent",
                "password": "SomePassword123!"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_token_can_access_protected_endpoint() {
    let app = TestApp::new().await;

    // Register a user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "loginauth",
                "email": "loginauth@example.com",
                "password": "SecurePass123!",
                "display_name": "Login Auth"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Login and get token
    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": "loginauth",
                "password": "SecurePass123!"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let token = body["data"]["access_token"].as_str().unwrap();

    // Use the token to access a protected endpoint
    let response = app.get_with_token("/v1/users/me", token).await;
    response.assert_status(StatusCode::OK);

    let user_body: serde_json::Value = response.json();
    assert_eq!(user_body["data"]["username"], "loginauth");
}

// ===========================================
// Refresh Token Tests
// ===========================================

#[tokio::test]
async fn test_refresh_token_happy_path() {
    let app = TestApp::new().await;

    // Register a user — the response carries the initial refresh token
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "refreshuser",
                "email": "refreshuser@example.com",
                "password": "SecurePass123!",
                "display_name": "Refresh User"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let refresh_token = body["data"]["refresh_token"].as_str().unwrap().to_string();
    assert!(
        !refresh_token.is_empty(),
        "Registration should return a refresh token"
    );

    // Exchange it for a new token pair
    let response = app
        .post_json_no_auth(
            "/v1/auth/refresh",
            &json!({ "refresh_token": refresh_token.as_str() }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let new_access = body["data"]["access_token"].as_str().unwrap();
    let new_refresh = body["data"]["refresh_token"].as_str().unwrap();
    assert_eq!(body["data"]["username"], "refreshuser");
    assert_ne!(
        new_refresh, refresh_token,
        "Rotation should issue a different refresh token"
    );

    // The new access token works on a protected endpoint
    let response = app.get_with_token("/v1/users/me", new_access).await;
    response.assert_status(StatusCode::OK);
    let user_body: serde_json::Value = response.json();
    assert_eq!(user_body["data"]["username"], "refreshuser");
}

#[tokio::test]
async fn test_refresh_token_invalid_rejected() {
    let app = TestApp::new().await;

    // A garbage token that never existed is rejected
    let response = app
        .post_json_no_auth(
            "/v1/auth/refresh",
            &json!({ "refresh_token": "definitely-not-a-real-refresh-token" }),
        )
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // An empty token fails validation
    let response = app
        .post_json_no_auth("/v1/auth/refresh", &json!({ "refresh_token": "" }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_refresh_token_reuse_rejected() {
    let app = TestApp::new().await;

    // Register a user
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "replayuser",
                "email": "replayuser@example.com",
                "password": "SecurePass123!",
                "display_name": "Replay User"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let old_refresh = body["data"]["refresh_token"].as_str().unwrap().to_string();

    // First refresh succeeds and rotates the token
    let response = app
        .post_json_no_auth(
            "/v1/auth/refresh",
            &json!({ "refresh_token": old_refresh.as_str() }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let new_refresh = body["data"]["refresh_token"].as_str().unwrap().to_string();

    // Replaying the rotated (revoked) token is rejected
    let response = app
        .post_json_no_auth(
            "/v1/auth/refresh",
            &json!({ "refresh_token": old_refresh.as_str() }),
        )
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // Replay detection revokes every active token for the user, so the
    // newly issued refresh token is dead too.
    let response = app
        .post_json_no_auth("/v1/auth/refresh", &json!({ "refresh_token": new_refresh }))
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ===========================================
// JWT Token Validation Tests
// ===========================================

#[tokio::test]
async fn test_invalid_token_rejected() {
    let app = TestApp::new().await;

    let response = app
        .get_with_token("/v1/users/me", "invalid-jwt-token")
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_expired_token_rejected() {
    let app = TestApp::new().await;

    // Generate an already-expired token (use -5 minutes to account for jsonwebtoken's 60-second leeway)
    let user_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let expired_token = generate_access_token_with_expiry(
        user_id,
        player_id,
        "testuser",
        "test-jwt-secret",
        -5, // Well past the 60-second leeway
    )
    .unwrap();

    let response = app.get_with_token("/v1/users/me", &expired_token).await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_wrong_secret_token_rejected() {
    let app = TestApp::new().await;

    // Generate a token with a different secret
    let user_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let bad_token =
        generate_access_token_with_expiry(user_id, player_id, "testuser", "wrong-secret", 15)
            .unwrap();

    let response = app.get_with_token("/v1/users/me", &bad_token).await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_missing_auth_header_rejected() {
    let app = TestApp::new().await;

    // GET without any auth
    let response = app.get("/v1/users/me").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_register_rejects_reserved_steam_placeholder_email() {
    let app = TestApp::new().await;

    // steam_<id64>@steam.invalid is derivable from a public SteamID64 and
    // reserved for Steam-provisioned accounts — registering it would let
    // an attacker capture that Steam user's first sign-in.
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "steamsquatter",
                "email": "steam_76561197960287930@steam.invalid",
                "password": "SecurePass123!",
                "display_name": "Steam Squatter"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Case variations are rejected too.
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "steamsquatter2",
                "email": "steam_76561197960287930@STEAM.INVALID",
                "password": "SecurePass123!",
                "display_name": "Steam Squatter"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ===========================================
// Logout Tests
// ===========================================

/// Register a user and return `(access_token, refresh_token)`.
async fn register_and_get_tokens(app: &TestApp, username: &str) -> (String, String) {
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": "SecurePass123!",
                "display_name": username
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    (
        body["data"]["access_token"].as_str().unwrap().to_string(),
        body["data"]["refresh_token"].as_str().unwrap().to_string(),
    )
}

/// POST /v1/auth/logout revokes the presented refresh token so it can no
/// longer be exchanged (previously a stolen token stayed live for 7 days).
#[tokio::test]
async fn test_logout_revokes_presented_refresh_token() {
    let app = TestApp::new().await;
    let (_access, refresh_token) = register_and_get_tokens(&app, "logoutuser").await;

    let response = app
        .post_json_no_auth(
            "/v1/auth/logout",
            &json!({ "refresh_token": refresh_token.as_str() }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["logged_out"], true);

    // The token is dead.
    let response = app
        .post_json_no_auth(
            "/v1/auth/refresh",
            &json!({ "refresh_token": refresh_token.as_str() }),
        )
        .await;
    assert_eq!(
        response.status,
        StatusCode::UNAUTHORIZED,
        "Refresh after logout must fail, got {}: {}",
        response.status,
        response.text()
    );
}

/// Logout is idempotent and does not leak whether a token existed.
#[tokio::test]
async fn test_logout_unknown_token_is_ok() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth(
            "/v1/auth/logout",
            &json!({ "refresh_token": "definitely-not-a-real-refresh-token" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // No token at all is a 400 (nothing to revoke).
    let response = app.post_json_no_auth("/v1/auth/logout", &json!({})).await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

/// Logging out one session must not kill the user's other sessions.
#[tokio::test]
async fn test_logout_does_not_affect_other_sessions() {
    let app = TestApp::new().await;
    let (_access, first_refresh) = register_and_get_tokens(&app, "logoutscopeduser").await;

    // A second session for the same user.
    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": "logoutscopeduser",
                "password": "SecurePass123!"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let second_refresh = body["data"]["refresh_token"].as_str().unwrap().to_string();

    // Log the first session out.
    app.post_json_no_auth(
        "/v1/auth/logout",
        &json!({ "refresh_token": first_refresh.as_str() }),
    )
    .await
    .assert_status(StatusCode::OK);

    // The second session still refreshes.
    let response = app
        .post_json_no_auth(
            "/v1/auth/refresh",
            &json!({ "refresh_token": second_refresh.as_str() }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// POST /v1/auth/logout-all revokes every refresh token for the caller.
#[tokio::test]
async fn test_logout_all_revokes_every_session() {
    let app = TestApp::new().await;
    let (access, first_refresh) = register_and_get_tokens(&app, "logoutalluser").await;

    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": "logoutalluser",
                "password": "SecurePass123!"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let second_refresh = body["data"]["refresh_token"].as_str().unwrap().to_string();

    // logout-all is bound to the access token, not to a refresh token value.
    let response = app.post_with_token("/v1/auth/logout-all", &access).await;
    response.assert_status(StatusCode::OK);

    for token in [&first_refresh, &second_refresh] {
        let response = app
            .post_json_no_auth("/v1/auth/refresh", &json!({ "refresh_token": token }))
            .await;
        assert_eq!(
            response.status,
            StatusCode::UNAUTHORIZED,
            "Refresh after logout-all must fail, got {}: {}",
            response.status,
            response.text()
        );
    }
}

/// logout-all requires authentication.
#[tokio::test]
async fn test_logout_all_requires_auth() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth("/v1/auth/logout-all", &json!({}))
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}
