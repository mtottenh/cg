//! Games API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
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
    assert!(games.iter().any(|g| g["slug"] == "cs2"));
}

#[tokio::test]
async fn test_get_game() {
    let app = TestApp::new().await;

    // Get CS2 (seeded game)
    let response = app.get("/v1/games/cs2").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["slug"], "cs2");
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

// ============================================================================
// UUID-OR-SLUG RESOLUTION
// ============================================================================
//
// `GET /v1/games` returns each game's `id` as a UUID, so a client doing
// list -> detail addresses the single-game routes by UUID. Every
// `/v1/games/{game_id}` endpoint must therefore accept both forms.

/// Read the UUID that `GET /v1/games` advertises for CS2.
async fn cs2_uuid(app: &TestApp) -> String {
    let response = app.get("/v1/games").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["slug"] == "cs2")
        .expect("cs2 should be seeded")["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_get_game_by_uuid_matches_slug() {
    let app = TestApp::new().await;
    let uuid = cs2_uuid(&app).await;

    let by_uuid = app.get(&format!("/v1/games/{uuid}")).await;
    by_uuid.assert_status(StatusCode::OK);
    let by_uuid: serde_json::Value = by_uuid.json();

    let by_slug = app.get("/v1/games/cs2").await;
    by_slug.assert_status(StatusCode::OK);
    let by_slug: serde_json::Value = by_slug.json();

    assert_eq!(by_uuid["data"]["slug"], "cs2");
    assert_eq!(by_uuid["data"]["id"], uuid);
    assert_eq!(by_uuid["data"], by_slug["data"]);
}

#[tokio::test]
async fn test_get_maps_by_uuid_matches_slug() {
    let app = TestApp::new().await;
    let uuid = cs2_uuid(&app).await;

    let by_uuid = app.get(&format!("/v1/games/{uuid}/maps")).await;
    by_uuid.assert_status(StatusCode::OK);
    let by_uuid: serde_json::Value = by_uuid.json();

    let by_slug = app.get("/v1/games/cs2/maps").await;
    by_slug.assert_status(StatusCode::OK);
    let by_slug: serde_json::Value = by_slug.json();

    assert!(!by_uuid["data"].as_array().unwrap().is_empty());
    assert_eq!(by_uuid["data"], by_slug["data"]);
}

#[tokio::test]
async fn test_get_rank_tiers_by_uuid_matches_slug() {
    let app = TestApp::new().await;
    let uuid = cs2_uuid(&app).await;

    let by_uuid = app.get(&format!("/v1/games/{uuid}/rank-tiers")).await;
    by_uuid.assert_status(StatusCode::OK);
    let by_uuid: serde_json::Value = by_uuid.json();

    let by_slug = app.get("/v1/games/cs2/rank-tiers").await;
    by_slug.assert_status(StatusCode::OK);
    let by_slug: serde_json::Value = by_slug.json();

    assert!(!by_uuid["data"].as_array().unwrap().is_empty());
    assert_eq!(by_uuid["data"], by_slug["data"]);
}

/// A well-formed UUID that is not a game still 404s — the slug fallback
/// must not turn unknown identifiers into matches.
#[tokio::test]
async fn test_get_game_unknown_uuid_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get("/v1/games/00000000-0000-0000-0000-0000000000ff")
        .await;
    response.assert_status(StatusCode::NOT_FOUND);

    let response = app
        .get("/v1/games/00000000-0000-0000-0000-0000000000ff/maps")
        .await;
    response.assert_status(StatusCode::NOT_FOUND);

    let response = app
        .get("/v1/games/00000000-0000-0000-0000-0000000000ff/rank-tiers")
        .await;
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
    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
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
    assert_eq!(body["data"]["slug"], "cs2");
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
    assert!(!games.iter().any(|g| g["slug"] == "cs2"));

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
    assert!(games.iter().any(|g| g["slug"] == "cs2"));
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

// ============================================================================
// MAP CATALOG TESTS
// ============================================================================

#[tokio::test]
async fn test_add_map() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_workshop_map",
                "display_name": "Workshop Map",
                "game_modes": ["competitive"],
                "external_id": "123456789",
                "external_url": "https://steamcommunity.com/sharedfiles/filedetails/?id=123456789"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let maps = body["data"].as_array().unwrap();
    // Should now include the custom map + the original 7
    assert!(maps.len() > 7);
    let custom = maps.iter().find(|m| m["id"] == "de_workshop_map").unwrap();
    assert_eq!(custom["display_name"], "Workshop Map");
    assert_eq!(custom["external_id"], "123456789");
}

#[tokio::test]
async fn test_add_duplicate_map() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Add a map first
    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_custom",
                "display_name": "Custom",
                "game_modes": ["competitive"]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Try to add the same map again
    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_custom",
                "display_name": "Custom 2",
                "game_modes": ["competitive"]
            }),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_add_map_existing_plugin_map_conflict() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Try to add a map that already exists as a plugin default
    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_dust2",
                "display_name": "Dust II Again",
                "game_modes": ["competitive"]
            }),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_update_map() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // First add a custom map
    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_updatable",
                "display_name": "Original Name",
                "game_modes": ["competitive"]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Update it
    let response = app
        .patch_json(
            "/v1/games/cs2/maps/catalog/de_updatable",
            &json!({
                "display_name": "Updated Name",
                "external_id": "99999"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], "de_updatable");
    assert_eq!(body["data"]["display_name"], "Updated Name");
    assert_eq!(body["data"]["external_id"], "99999");
}

#[tokio::test]
async fn test_update_map_not_found() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .patch_json(
            "/v1/games/cs2/maps/catalog/de_nonexistent",
            &json!({
                "display_name": "Will not work"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_remove_map() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Add a map
    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_removable",
                "display_name": "Removable Map",
                "game_modes": ["competitive"]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Remove it
    let response = app
        .delete_auth("/v1/games/cs2/maps/catalog/de_removable")
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone by checking maps
    let response = app.get("/v1/games/cs2/maps").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let maps = body["data"].as_array().unwrap();
    assert!(!maps.iter().any(|m| m["id"] == "de_removable"));
}

#[tokio::test]
async fn test_remove_map_not_found() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .delete_auth("/v1/games/cs2/maps/catalog/de_nonexistent")
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// RANK TIERS TESTS
// ============================================================================

#[tokio::test]
async fn test_set_rank_tiers() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .put_json(
            "/v1/games/cs2/rank-tiers",
            &json!({
                "rank_tiers": [
                    {
                        "id": "bronze",
                        "display_name": "Bronze",
                        "min_rating": 0,
                        "max_rating": 999,
                        "color": "#CD7F32",
                        "order": 1
                    },
                    {
                        "id": "silver",
                        "display_name": "Silver",
                        "min_rating": 1000,
                        "max_rating": 1999,
                        "color": "#C0C0C0",
                        "order": 2
                    },
                    {
                        "id": "gold",
                        "display_name": "Gold",
                        "min_rating": 2000,
                        "color": "#FFD700",
                        "order": 3
                    }
                ]
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let tiers = body["data"].as_array().unwrap();
    assert_eq!(tiers.len(), 3);
    assert_eq!(tiers[0]["id"], "bronze");
    assert_eq!(tiers[2]["id"], "gold");

    // Verify via GET endpoint
    let response = app.get("/v1/games/cs2/rank-tiers").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let tiers = body["data"].as_array().unwrap();
    assert_eq!(tiers.len(), 3);
    assert_eq!(tiers[0]["id"], "bronze");
}

#[tokio::test]
async fn test_set_rank_tiers_invalid_overlap() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .put_json(
            "/v1/games/cs2/rank-tiers",
            &json!({
                "rank_tiers": [
                    {
                        "id": "bronze",
                        "display_name": "Bronze",
                        "min_rating": 0,
                        "max_rating": 1000,
                        "order": 1
                    },
                    {
                        "id": "silver",
                        "display_name": "Silver",
                        "min_rating": 500,
                        "max_rating": 2000,
                        "order": 2
                    }
                ]
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// TEAM SIZE TESTS
// ============================================================================

#[tokio::test]
async fn test_update_team_size() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .patch_json(
            "/v1/games/cs2/team-size",
            &json!({
                "min": 3,
                "max": 7,
                "default": 5
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["min"], 3);
    assert_eq!(body["data"]["max"], 7);
    assert_eq!(body["data"]["default"], 5);

    // Verify via GET game detail
    let response = app.get("/v1/games/cs2").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["team_size"]["min"], 3);
    assert_eq!(body["data"]["team_size"]["max"], 7);
    assert_eq!(body["data"]["team_size"]["default"], 5);
}

#[tokio::test]
async fn test_update_team_size_partial() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Only update max (CS2 default is min=5, max=5, default=5)
    let response = app
        .patch_json(
            "/v1/games/cs2/team-size",
            &json!({
                "max": 10
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["min"], 5);
    assert_eq!(body["data"]["max"], 10);
    assert_eq!(body["data"]["default"], 5);
}

#[tokio::test]
async fn test_update_team_size_invalid_min_gt_max() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .patch_json(
            "/v1/games/cs2/team-size",
            &json!({
                "min": 10,
                "max": 3,
                "default": 5
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_update_team_size_invalid_default_gt_max() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .patch_json(
            "/v1/games/cs2/team-size",
            &json!({
                "min": 1,
                "max": 3,
                "default": 5
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// CUSTOM MAP + MAP POOL INTEGRATION
// ============================================================================

#[tokio::test]
async fn test_set_map_pool_with_custom_map() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Add a custom map
    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_workshop",
                "display_name": "Workshop",
                "game_modes": ["competitive"]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Now set map pool including the custom map
    let response = app
        .put_json(
            "/v1/games/cs2/maps",
            &json!({
                "map_ids": ["de_dust2", "de_mirage", "de_workshop"]
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let maps = body["data"].as_array().unwrap();
    assert_eq!(maps.len(), 3);
    assert!(maps.iter().any(|m| m["id"] == "de_workshop"));
}

// ============================================================================
// AUTHORIZATION TESTS FOR NEW ENDPOINTS
// ============================================================================

#[tokio::test]
async fn test_add_map_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_test",
                "display_name": "Test",
                "game_modes": ["competitive"]
            }),
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_update_map_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .patch_json(
            "/v1/games/cs2/maps/catalog/de_dust2",
            &json!({
                "display_name": "Test"
            }),
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_remove_map_requires_admin() {
    let app = TestApp::new().await;

    let response = app.delete_auth("/v1/games/cs2/maps/catalog/de_dust2").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_set_rank_tiers_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .put_json(
            "/v1/games/cs2/rank-tiers",
            &json!({
                "rank_tiers": [{
                    "id": "test",
                    "display_name": "Test",
                    "min_rating": 0,
                    "order": 1
                }]
            }),
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_update_team_size_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .patch_json("/v1/games/cs2/team-size", &json!({ "min": 1 }))
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// P-88 — THE ADMIN CATALOG MUST BE ABLE TO SEE A DISABLED GAME
// ============================================================================

/// P-88: a disabled game vanished from every list the product has, taking the
/// only control that could re-enable it with it.
///
/// `GET /v1/games` was unconditionally `list_active()` (`WHERE status =
/// 'active'`), and the admin games table is its only consumer with an Enable
/// button — a button that lives *inside a row*. Disable a game and the row is
/// gone on the next fetch, permanently, with no admin remedy short of SQL.
///
/// The assertion that matters is the third one: not "the parameter is accepted"
/// but "the disabled game is actually in the payload, still marked
/// `maintenance`", because that is the row whose Enable button the admin needs.
#[tokio::test]
async fn test_list_games_include_inactive_returns_disabled_games() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Precondition: aoe4 is seeded active and visible on the default list.
    let listed = app.get("/v1/games").await;
    listed.assert_status(StatusCode::OK);
    let listed: serde_json::Value = listed.json();
    assert!(
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["slug"] == "aoe4"),
    );

    let response = app.post_auth("/v1/games/aoe4/disable").await;
    response.assert_status(StatusCode::OK);

    // The default list still hides it — the public catalog is unchanged.
    let listed = app.get("/v1/games").await;
    listed.assert_status(StatusCode::OK);
    let listed: serde_json::Value = listed.json();
    assert!(
        !listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["slug"] == "aoe4"),
        "the default catalog must stay active-only"
    );

    // ...and the admin can still find it, with the status that explains why it
    // is missing from the default list.
    let listed = app.get_auth("/v1/games?include_inactive=true").await;
    listed.assert_status(StatusCode::OK);
    let listed: serde_json::Value = listed.json();
    let aoe4 = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["slug"] == "aoe4")
        .expect("P-88: a disabled game must be reachable from the admin catalog");
    assert_eq!(aoe4["status"], "maintenance");

    // And it can be re-enabled from there, which is the whole point.
    let response = app.post_auth("/v1/games/aoe4/enable").await;
    response.assert_status(StatusCode::OK);
    let listed = app.get("/v1/games").await;
    let listed: serde_json::Value = listed.json();
    assert!(
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["slug"] == "aoe4"),
    );
}

/// The unfiltered catalog is admin-only, and a caller that cannot have it is
/// **refused** rather than quietly handed the active list — otherwise a client
/// could believe it holds the whole catalog when it holds a filtered one.
#[tokio::test]
async fn test_list_games_include_inactive_requires_admin() {
    let app = TestApp::new().await;

    // Anonymous.
    let response = app.get("/v1/games?include_inactive=true").await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Authenticated, but the dev identity carries no roles by default.
    let response = app.get_auth("/v1/games?include_inactive=true").await;
    response.assert_status(StatusCode::FORBIDDEN);

    // The default list stays public and unauthenticated.
    let response = app.get("/v1/games").await;
    response.assert_status(StatusCode::OK);

    // ...and `include_inactive=false` is the default list, so it must not 403.
    let response = app.get("/v1/games?include_inactive=false").await;
    response.assert_status(StatusCode::OK);
}

// ============================================================================
// P-90 — SORT ORDER IS READABLE, AND SETTABLE TO ZERO
// ============================================================================

/// P-90: `sort_order` was writable through `PATCH /v1/games/{id}` but appeared
/// in no response, so the admin edit modal could not seed its "Sort Order"
/// field and hardcoded `0` — a number that was never the truth (cs2 is 1, aoe4
/// is 2). To avoid writing that fabricated `0` over the real value the modal
/// then only sent the field when it was non-zero, which made `0` unsettable for
/// every game.
///
/// Both halves are asserted here: the seeded values are visible on the list and
/// the detail, and a round trip to `0` sticks.
#[tokio::test]
async fn test_game_responses_expose_sort_order_and_zero_is_settable() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // The migration seeds cs2 = 1, aoe4 = 2 (0003_create_games.sql:68-70).
    let listed = app.get("/v1/games").await;
    listed.assert_status(StatusCode::OK);
    let listed: serde_json::Value = listed.json();
    let games = listed["data"].as_array().unwrap();
    let cs2 = games.iter().find(|g| g["slug"] == "cs2").unwrap();
    let aoe4 = games.iter().find(|g| g["slug"] == "aoe4").unwrap();
    assert_eq!(cs2["sort_order"], 1);
    assert_eq!(aoe4["sort_order"], 2);

    let detail = app.get("/v1/games/aoe4").await;
    detail.assert_status(StatusCode::OK);
    let detail: serde_json::Value = detail.json();
    assert_eq!(detail["data"]["sort_order"], 2);

    // Zero is a legal sort order and must round-trip.
    let response = app
        .patch_json("/v1/games/aoe4", &json!({ "sort_order": 0 }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["sort_order"], 0,
        "P-90: sort_order 0 must be settable"
    );

    let detail = app.get("/v1/games/aoe4").await;
    let detail: serde_json::Value = detail.json();
    assert_eq!(detail["data"]["sort_order"], 0, "and it must persist");
}

// ============================================================================
// HEALTH PROBES (here rather than a dedicated file — two small tests)
// ============================================================================

#[tokio::test]
async fn test_health_probes_db() {
    let app = TestApp::new().await;
    let response = app.get("/health").await;
    response.assert_status(StatusCode::OK);
    assert_eq!(response.text(), "OK");
}

#[tokio::test]
async fn test_health_ready_reports_dependencies() {
    let app = TestApp::new().await;
    let response = app.get("/health/ready").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db"], "ok");
    // No CS2_DEMO_SERVICE_URL configured in tests.
    assert_eq!(body["demo_service"], "unconfigured");
}

// ============================================================================
// P-87 — GAME-CONFIG WRITES ADDRESSED BY UUID
// ============================================================================

/// P-87: every game-config WRITE 404'd when the game was addressed by UUID.
///
/// `GameRepository::update` is keyed by SLUG ("Update a game by slug",
/// repositories/game.rs:106) and returns not-found otherwise. Six handlers
/// resolved the game with `find_by_id_or_slug` — which accepts either — and then
/// passed the raw `{game_id}` path parameter to that slug-keyed write. Since
/// migration `0024` made `games.id` a UUID, `GameSummaryResponse.id` is the
/// UUID, and that is exactly what the admin UI sends. So Add Map, Edit Map,
/// Delete Map and Save Pool were dead controls that popped a failure snackbar
/// every time.
///
/// The reads were already covered by UUID (see the `*_by_uuid_matches_slug`
/// tests above) — which is precisely why this went unnoticed. These drive the
/// **writes** by UUID, and each asserts the mutation PERSISTED rather than
/// merely returning 2xx: a handler that resolves the game, 404s on the write and
/// still returns the pre-read state would satisfy a status-only assertion.
#[tokio::test]
async fn test_game_config_writes_by_uuid_persist() {
    let app = TestApp::new().await;
    let uuid = cs2_uuid(&app).await;

    // These writes are gated on `admin.games.manage`; the dev-token identity
    // carries no roles by default.
    let dev_user = portal_test::helpers::get_dev_user_id(app.pool()).await;
    portal_test::helpers::assign_role_to_user(app.pool(), dev_user, "super_admin").await;

    // --- map catalog: add, addressed by UUID ---------------------------------
    let map_id = "de_p87_probe";
    let response = app
        .post_json(
            &format!("/v1/games/{uuid}/maps/catalog"),
            &json!({
                "id": map_id,
                "display_name": "P-87 Probe",
                "game_modes": ["competitive"],
                "is_active": true
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // The catalog is read back through `GET /maps` — it serves `available_maps`,
    // the same column `add_map` writes. There is no `GET /maps/catalog` route.
    let maps = app.get(&format!("/v1/games/{uuid}/maps")).await;
    maps.assert_status(StatusCode::OK);
    let maps: serde_json::Value = maps.json();
    assert!(
        maps["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == map_id),
        "P-87: the added map must be persisted when the game is addressed by UUID"
    );

    // --- rank tiers, addressed by UUID ---------------------------------------
    let response = app
        .put_json(
            &format!("/v1/games/{uuid}/rank-tiers"),
            &json!({
                "rank_tiers": [
                    { "id": "p87_low", "display_name": "P87 Low", "min_rating": 0, "max_rating": 999, "order": 1 },
                    { "id": "p87_high", "display_name": "P87 High", "min_rating": 1000, "order": 2 }
                ]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let tiers = app.get(&format!("/v1/games/{uuid}/rank-tiers")).await;
    tiers.assert_status(StatusCode::OK);
    let tiers: serde_json::Value = tiers.json();
    assert!(
        tiers["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == "p87_high"),
        "P-87: rank tiers must persist when the game is addressed by UUID"
    );

    // --- team size, addressed by UUID ----------------------------------------
    let response = app
        .patch_json(
            &format!("/v1/games/{uuid}/team-size"),
            &json!({ "min": 3, "max": 7, "default": 5 }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let game = app.get(&format!("/v1/games/{uuid}")).await;
    game.assert_status(StatusCode::OK);
    let game: serde_json::Value = game.json();
    assert_eq!(
        game["data"]["team_size"]["min"], 3,
        "P-87: team size must persist when the game is addressed by UUID"
    );
    assert_eq!(game["data"]["team_size"]["max"], 7);
}

/// P-121: `GET /v1/games` threaded `PaginationParams` into the response
/// metadata but never applied it to the list, so every page carried the
/// COMPLETE catalog under page-N metadata. A paginating client renders page
/// 1's items again under a "page 2" heading, or duplicates them by appending.
///
/// Asserted through the wire rather than the handler because the defect was
/// precisely that metadata and payload disagreed — checking either alone
/// reproduces the blind spot that let this ship.
#[tokio::test]
async fn test_list_games_actually_paginates() {
    let app = TestApp::new().await;

    // Establish the catalog size from an explicitly large page.
    //
    // Every assertion below is deliberately independent of this number staying
    // stable across requests. It cannot change here — TestDb gives each test its
    // own database — but a test that would break if it did is a test whose
    // failures are ambiguous, and this one already produced one ambiguous report:
    // a concurrent full-suite run saw it red while a red-proof probe for this
    // very defect was momentarily in the shared tree, and it was reported as
    // flakiness. Order-independence makes the next failure mean one thing.
    let all = app.get("/v1/games?page=1&per_page=100").await;
    all.assert_status(StatusCode::OK);
    let all_body: serde_json::Value = all.json();
    let total = all_body["pagination"]["total_items"]
        .as_u64()
        .expect("pagination.total_items present");
    let all_len = all_body["data"].as_array().unwrap().len() as u64;
    assert_eq!(
        all_len, total,
        "a page large enough to hold the catalog should return all of it"
    );
    assert!(
        total >= 2,
        "need at least 2 seeded games to prove pagination slices ({total} present); \
         with fewer, per_page=1 would return the whole catalog and pass vacuously"
    );

    // A single-item page must contain exactly one game...
    let first = app.get("/v1/games?page=1&per_page=1").await;
    first.assert_status(StatusCode::OK);
    let first_body: serde_json::Value = first.json();
    let first_page = first_body["data"].as_array().unwrap();
    assert_eq!(
        first_page.len(),
        1,
        "per_page=1 must return 1 game, got {} — the list is not being sliced",
        first_page.len()
    );

    // ...while `total` still reports the CATALOG, not the page. Counting after
    // the slice would make this 1, collapsing total_pages and silently disabling
    // the client's "next page" control. Asserted as `>= 2` rather than `== total`
    // so it tests the property rather than the stability of an earlier read.
    assert!(
        first_body["pagination"]["total_items"].as_u64().unwrap() >= 2,
        "total must count the catalog, not the returned page (got {})",
        first_body["pagination"]["total_items"]
    );

    // Page 2 must be DIFFERENT items. This is the assertion that fails on the
    // original bug: it returned the full catalog for every page, so page 2's
    // first item was page 1's first item.
    let second = app.get("/v1/games?page=2&per_page=1").await;
    second.assert_status(StatusCode::OK);
    let second_body: serde_json::Value = second.json();
    let second_page = second_body["data"].as_array().unwrap();
    assert_eq!(second_page.len(), 1, "page 2 must also hold exactly 1 game");
    assert_ne!(
        first_page[0]["id"], second_page[0]["id"],
        "page 2 returned the same game as page 1 — pagination is decorative"
    );

    // Far past the end: empty page. A fixed, absurdly high page number rather
    // than one derived from `total`, so this cannot depend on an earlier read.
    let past_end = app.get("/v1/games?page=10000&per_page=100").await;
    past_end.assert_status(StatusCode::OK);
    let past_body: serde_json::Value = past_end.json();
    assert!(
        past_body["data"].as_array().unwrap().is_empty(),
        "a page past the end must be empty, not a repeat of the catalog"
    );
}

/// P-120: rank tiers could be set but never REMOVED — `SetRankTiersRequest`
/// required at least one tier, so once a custom set existed the only way
/// back to the plugin defaults was SQL. An empty list now clears the stored
/// override, and reads fall back to the plugin's built-in tiers.
#[tokio::test]
async fn test_rank_tiers_can_be_cleared_back_to_plugin_defaults() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Baseline: the plugin defaults, before any override.
    let defaults = app.get("/v1/games/cs2/rank-tiers").await;
    defaults.assert_status(StatusCode::OK);
    let defaults: serde_json::Value = defaults.json();
    let default_count = defaults["data"].as_array().unwrap().len();
    assert!(default_count > 0, "cs2's plugin ships default tiers");

    // Install a one-tier custom override.
    let response = app
        .put_json(
            "/v1/games/cs2/rank-tiers",
            &json!({
                "rank_tiers": [{
                    "id": "only",
                    "display_name": "Only Tier",
                    "min_rating": 0,
                    "max_rating": null,
                    "color": "#ffffff",
                    "order": 1
                }]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let installed = app.get("/v1/games/cs2/rank-tiers").await;
    let installed: serde_json::Value = installed.json();
    assert_eq!(installed["data"].as_array().unwrap().len(), 1);

    // Clear it: the empty list is a valid request, not a 400.
    let cleared = app
        .put_json("/v1/games/cs2/rank-tiers", &json!({ "rank_tiers": [] }))
        .await;
    cleared.assert_status(StatusCode::OK);

    // Reads are back on the plugin defaults.
    let after = app.get("/v1/games/cs2/rank-tiers").await;
    after.assert_status(StatusCode::OK);
    let after: serde_json::Value = after.json();
    assert_eq!(
        after["data"].as_array().unwrap().len(),
        default_count,
        "clearing the override must fall back to the plugin's tiers"
    );
}

// ============================================================================
// WORKSHOP MAP TESTS
// ============================================================================

/// Stub workshop metadata provider: one known CS2 map, everything else 404s.
struct StubWorkshopProvider;

#[async_trait::async_trait]
impl portal_api::steam_workshop::WorkshopMetadataProvider for StubWorkshopProvider {
    async fn published_file_details(
        &self,
        file_id: u64,
    ) -> Result<Option<portal_api::steam_workshop::WorkshopFileDetails>, String> {
        if file_id != 3437809122 {
            return Ok(None);
        }
        Ok(Some(portal_api::steam_workshop::WorkshopFileDetails {
            workshop_id: file_id.to_string(),
            title: Some("Cache".to_string()),
            preview_url: Some("https://img.example/cache.jpg".to_string()),
            filename: Some("de_cache.vpk".to_string()),
            file_size_bytes: Some(734_003_200),
            time_updated: Some(1_750_000_000),
            consumer_app_id: Some(730),
            visibility: Some(0),
            banned: false,
        }))
    }
}

#[tokio::test]
async fn test_workshop_lookup_returns_prefill_metadata() {
    let app = TestApp::new_with_workshop_metadata(std::sync::Arc::new(StubWorkshopProvider)).await;
    grant_games_admin_permission(&app).await;

    let response = app.get_auth("/v1/games/cs2/workshop-maps/3437809122").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let data = &body["data"];
    assert_eq!(data["workshop_id"], "3437809122");
    assert_eq!(data["title"], "Cache");
    assert_eq!(data["engine_name_hint"], "de_cache");
    assert_eq!(data["consumer_app_id"], 730);
    assert_eq!(data["visibility"], 0);
    assert_eq!(data["banned"], false);
    assert_eq!(
        data["workshop_url"],
        "https://steamcommunity.com/sharedfiles/filedetails/?id=3437809122"
    );

    // Unknown item → 404; non-numeric id → 400.
    let response = app.get_auth("/v1/games/cs2/workshop-maps/999").await;
    response.assert_status(StatusCode::NOT_FOUND);
    let response = app.get_auth("/v1/games/cs2/workshop-maps/de_cache").await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_workshop_lookup_requires_games_admin() {
    let app = TestApp::new_with_workshop_metadata(std::sync::Arc::new(StubWorkshopProvider)).await;
    // No admin grant.
    let response = app.get_auth("/v1/games/cs2/workshop-maps/3437809122").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_map_engine_name_roundtrip_and_clear() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    // Add a workshop map whose engine name differs from the portal id.
    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_cache_ws",
                "display_name": "Cache (Workshop)",
                "game_modes": ["competitive"],
                "engine_name": "de_cache",
                "external_id": "3437809122",
                "external_url": "https://steamcommunity.com/sharedfiles/filedetails/?id=3437809122"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let added = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "de_cache_ws")
        .unwrap()
        .clone();
    assert_eq!(added["engine_name"], "de_cache");

    // The stored catalog serves it back on public reads.
    let response = app.get("/v1/games/cs2/maps").await;
    let body: serde_json::Value = response.json();
    let served = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "de_cache_ws")
        .unwrap()
        .clone();
    assert_eq!(served["engine_name"], "de_cache");

    // Patching with an empty engine_name clears the override.
    let response = app
        .patch_json(
            "/v1/games/cs2/maps/catalog/de_cache_ws",
            &json!({ "engine_name": "" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body["data"].get("engine_name").is_none() || body["data"]["engine_name"].is_null(),
        "empty engine_name must clear the override, got: {}",
        body["data"]
    );
}

#[tokio::test]
async fn test_add_map_engine_name_equal_to_id_is_normalized_away() {
    let app = TestApp::new().await;
    grant_games_admin_permission(&app).await;

    let response = app
        .post_json(
            "/v1/games/cs2/maps/catalog",
            &json!({
                "id": "de_season",
                "display_name": "Season",
                "game_modes": ["competitive"],
                "engine_name": "de_season"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let added = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "de_season")
        .unwrap()
        .clone();
    assert!(
        added.get("engine_name").is_none() || added["engine_name"].is_null(),
        "engine_name equal to the id is noise and must not be stored"
    );
}
