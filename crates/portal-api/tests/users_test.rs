//! User API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn test_get_current_user() {
    let app = TestApp::new().await;

    // Dev user is already seeded by migration 0013_seed_dev_user.sql
    // Just make the authenticated request
    let response = app.get_auth("/v1/users/me").await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["username"], "devuser");
    assert_eq!(body["data"]["email"], "dev@example.com");
}

#[tokio::test]
async fn test_get_current_user_unauthorized() {
    let app = TestApp::new().await;

    // Try without authentication
    let response = app.get("/v1/users/me").await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}
