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
