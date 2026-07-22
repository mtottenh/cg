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
