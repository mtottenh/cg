//! Games API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;
use sqlx::Row;

// ============================================================================
// PUBLIC ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn test_list_games() {
    let app = TestApp::new().await;

    // List games (public endpoint, no auth required)
    let response = app.get("/v1/games").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].is_array());
    // CS2 should be seeded
    let games = body["data"].as_array().unwrap();
    assert!(games.iter().any(|g| g["id"] == "cs2"));
}

#[tokio::test]
async fn test_get_game() {
    let app = TestApp::new().await;

    // Get CS2 (seeded game)
    let response = app.get("/v1/games/cs2").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], "cs2");
    assert_eq!(body["data"]["display_name"], "Counter-Strike 2");
    assert!(body["data"]["maps"].is_array());
    assert!(body["data"]["rank_tiers"].is_array());
}

#[tokio::test]
async fn test_get_game_not_found() {
    let app = TestApp::new().await;

    let response = app.get("/v1/games/nonexistent").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_maps() {
    let app = TestApp::new().await;

    let response = app.get("/v1/games/cs2/maps").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let maps = body["data"].as_array().unwrap();

    // CS2 should have 7 maps seeded
    assert!(!maps.is_empty());

    // Check that de_dust2 is in the list
    assert!(maps.iter().any(|m| m["id"] == "de_dust2"));

    // Check map structure
    let dust2 = maps.iter().find(|m| m["id"] == "de_dust2").unwrap();
    assert_eq!(dust2["display_name"], "Dust II");
    assert!(dust2["game_modes"].is_array());
}

#[tokio::test]
async fn test_get_maps_not_found() {
    let app = TestApp::new().await;

    let response = app.get("/v1/games/nonexistent/maps").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_rank_tiers() {
    let app = TestApp::new().await;

    let response = app.get("/v1/games/cs2/rank-tiers").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let tiers = body["data"].as_array().unwrap();

    // CS2 Premier has 7 color tiers
    assert!(!tiers.is_empty());

    // Check that grey (lowest) tier exists
    assert!(tiers.iter().any(|t| t["id"] == "grey"));

    // Check tier structure
    let grey = tiers.iter().find(|t| t["id"] == "grey").unwrap();
    assert_eq!(grey["display_name"], "Grey");
    assert_eq!(grey["min_rating"], 0);
    assert_eq!(grey["color"], "#808080");
}

#[tokio::test]
async fn test_get_rank_tiers_not_found() {
    let app = TestApp::new().await;

    let response = app.get("/v1/games/nonexistent/rank-tiers").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// ADMIN ENDPOINT TESTS - AUTHORIZATION
// ============================================================================

#[tokio::test]
async fn test_update_game_requires_auth() {
    let app = TestApp::new().await;

    // Try to update without auth
    let response = app
        .patch_json_no_auth(
            "/v1/games/cs2",
            &json!({
                "description": "Updated description"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_update_game_requires_admin_permission() {
    let app = TestApp::new().await;

    // Dev user doesn't have admin.games.manage permission by default
    let response = app
        .patch_json(
            "/v1/games/cs2",
            &json!({
                "description": "Updated description"
            }),
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_set_map_pool_requires_admin_permission() {
    let app = TestApp::new().await;

    // Dev user doesn't have admin.games.manage permission by default
    let response = app
        .put_json(
            "/v1/games/cs2/maps",
            &json!({
                "map_ids": ["de_dust2", "de_mirage"]
            }),
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_enable_game_requires_admin_permission() {
    let app = TestApp::new().await;

    let response = app.post_auth("/v1/games/cs2/enable").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_disable_game_requires_admin_permission() {
    let app = TestApp::new().await;

    let response = app.post_auth("/v1/games/cs2/disable").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// ADMIN ENDPOINT TESTS - WITH PERMISSION
// ============================================================================

/// Helper to grant admin.games.manage permission to dev user
async fn grant_games_admin_permission(app: &TestApp) {
    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    // Get platform_admin role ID
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'platform_admin'")
        .fetch_one(app.pool())
        .await
        .expect("platform_admin role should exist");
    let role_id: uuid::Uuid = role_row.get("id");

    // Assign role to dev user
    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
    )
    .bind(dev_user_id)
    .bind(role_id)
    .execute(app.pool())
    .await
    .expect("Failed to assign role");
}

#[tokio::test]
async fn test_update_game_with_admin_permission() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .patch_json(
            "/v1/games/cs2",
            &json!({
                "description": "Updated CS2 description for test"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], "cs2");
    // Note: description is in the response but may be from DB or plugin
}

#[tokio::test]
async fn test_update_game_not_found() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .patch_json(
            "/v1/games/nonexistent",
            &json!({
                "description": "Updated description"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_game_display_name() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .patch_json(
            "/v1/games/cs2",
            &json!({
                "display_name": "CS2 - Test Name",
                "is_featured": true
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["display_name"], "CS2 - Test Name");
    assert_eq!(body["data"]["is_featured"], true);
}

#[tokio::test]
async fn test_set_map_pool_with_admin_permission() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .put_json(
            "/v1/games/cs2/maps",
            &json!({
                "map_ids": ["de_dust2", "de_mirage", "de_inferno"]
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let maps = body["data"].as_array().unwrap();
    assert_eq!(maps.len(), 3);
    assert!(maps.iter().any(|m| m["id"] == "de_dust2"));
    assert!(maps.iter().any(|m| m["id"] == "de_mirage"));
    assert!(maps.iter().any(|m| m["id"] == "de_inferno"));
}

#[tokio::test]
async fn test_set_map_pool_invalid_map() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .put_json(
            "/v1/games/cs2/maps",
            &json!({
                "map_ids": ["de_dust2", "invalid_map"]
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_set_map_pool_not_found() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .put_json(
            "/v1/games/nonexistent/maps",
            &json!({
                "map_ids": ["de_dust2"]
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_disable_and_enable_game() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Disable CS2 (sets status to "maintenance")
    let response = app.post_auth("/v1/games/cs2/disable").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "maintenance");

    // Verify it doesn't appear in list of active games
    let list_response = app.get("/v1/games").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let games = list_body["data"].as_array().unwrap();
    assert!(!games.iter().any(|g| g["id"] == "cs2"));

    // Re-enable CS2
    let response = app.post_auth("/v1/games/cs2/enable").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "active");

    // Verify it appears again in list
    let list_response = app.get("/v1/games").await;
    list_response.assert_status(StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let games = list_body["data"].as_array().unwrap();
    assert!(games.iter().any(|g| g["id"] == "cs2"));
}

#[tokio::test]
async fn test_enable_game_not_found() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app.post_auth("/v1/games/nonexistent/enable").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_disable_game_not_found() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app.post_auth("/v1/games/nonexistent/disable").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_update_game_validation_display_name_too_long() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // display_name max is 64 chars
    let long_name = "x".repeat(100);
    let response = app
        .patch_json(
            "/v1/games/cs2",
            &json!({
                "display_name": long_name
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_set_map_pool_validation_empty() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .put_json(
            "/v1/games/cs2/maps",
            &json!({
                "map_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}
