//! Demo catalog API integration tests.
//!
//! Tests cover:
//! - Category A: Demo catalog browsing and management
//! - Category B: Demo-match linking operations

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

// ============================================================================
// CATEGORY A: DEMO CATALOG TESTS
// ============================================================================

/// Test listing demos when none exist (empty list).
#[tokio::test]
async fn test_list_demos_empty() {
    let app = TestApp::new().await;

    let response = app.get_auth("/v1/demos").await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["demos"].as_array().unwrap().is_empty());
    assert_eq!(body["data"]["total"], 0);
}

/// Test listing demos with various filters.
#[tokio::test]
async fn test_list_demos_with_filters() {
    let app = TestApp::new().await;

    // Test with category filter
    let response = app.get_auth("/v1/demos?category=league").await;
    response.assert_status(StatusCode::OK);

    // Test with status filter
    let response = app.get_auth("/v1/demos?status=pending").await;
    response.assert_status(StatusCode::OK);

    // Test with map filter
    let response = app.get_auth("/v1/demos?map_name=dust2").await;
    response.assert_status(StatusCode::OK);

    // Test with pagination
    let response = app.get_auth("/v1/demos?limit=10&offset=0").await;
    response.assert_status(StatusCode::OK);
}

/// Test getting a demo that doesn't exist returns 404.
#[tokio::test]
async fn test_get_demo_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/demos/00000000-0000-0000-0000-000000000000")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

/// Test getting demo players for a non-existent demo returns empty list.
#[tokio::test]
async fn test_get_demo_players_empty() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/demos/00000000-0000-0000-0000-000000000000/players")
        .await;

    // Returns 200 with empty array (endpoint doesn't verify demo existence)
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

/// Test getting demo links for a non-existent demo returns empty list.
#[tokio::test]
async fn test_get_demo_links_empty() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/demos/00000000-0000-0000-0000-000000000000/links")
        .await;

    // Returns 200 with empty array (endpoint doesn't verify demo existence)
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

// ============================================================================
// CATEGORY B: DEMO-MATCH LINKING TESTS
// ============================================================================

/// Test getting demos for a match that doesn't exist returns empty list.
#[tokio::test]
async fn test_get_demos_for_match_empty() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/matches/00000000-0000-0000-0000-000000000000/demos")
        .await;

    // Returns 200 with empty array (endpoint doesn't verify match existence)
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

/// Test getting demos for a match with query parameters.
#[tokio::test]
async fn test_get_demos_for_match_with_query_params() {
    let app = TestApp::new().await;

    // Test with include_stats=true
    let response = app
        .get_auth("/v1/matches/00000000-0000-0000-0000-000000000000/demos?include_stats=true")
        .await;
    response.assert_status(StatusCode::OK);

    // Test with game_number filter
    let response = app
        .get_auth("/v1/matches/00000000-0000-0000-0000-000000000000/demos?game_number=1")
        .await;
    response.assert_status(StatusCode::OK);

    // Test with both params
    let response = app
        .get_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/demos?include_stats=true&game_number=1",
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Test linking a demo to a match requires admin access.
#[tokio::test]
async fn test_link_demo_to_match_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/demos/00000000-0000-0000-0000-000000000000/link",
            &json!({
                "match_id": "00000000-0000-0000-0000-000000000001",
                "game_number": 1,
                "link_type": "manual"
            }),
        )
        .await;

    // Dev user is not admin, should be forbidden
    response.assert_status(StatusCode::FORBIDDEN);
}

/// Test unlinking a demo from a match requires admin access.
#[tokio::test]
async fn test_unlink_demo_from_match_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .delete_auth(
            "/v1/admin/demos/00000000-0000-0000-0000-000000000000/link/00000000-0000-0000-0000-000000000001",
        )
        .await;

    // Dev user is not admin, should be forbidden
    response.assert_status(StatusCode::FORBIDDEN);
}

/// Test unauthorized access to admin demo endpoints.
#[tokio::test]
async fn test_admin_demo_endpoints_require_auth() {
    let app = TestApp::new().await;

    // Catalog demo without auth
    let response = app
        .post_json_no_auth(
            "/v1/admin/demos",
            &json!({
                "game_id": "00000000-0000-0000-0000-000000000000",
                "file_name": "test.dem",
                "s3_bucket": "test-bucket",
                "s3_key": "test/key.dem"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}
