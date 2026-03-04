//! Demo catalog API integration tests.
//!
//! Tests cover:
//! - Category A: Demo catalog browsing and management
//! - Category B: Demo-match linking operations
//! - Category C: Batch catalog and stats ingestion API


use axum::http::StatusCode;
use crate::common::TestApp;
use portal_test::prelude::*;
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

// ============================================================================
// CATEGORY C: BATCH CATALOG AND STATS INGESTION TESTS
// ============================================================================

/// Helper to grant admin role to the dev user.
async fn make_dev_user_admin(app: &TestApp) {
    let dev_user_id =
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
}

/// Test batch cataloging creates new demos.
#[tokio::test]
async fn test_batch_catalog_demos_creates_new() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/admin/demos/batch",
            &json!({
                "game_id": game_id.to_string(),
                "demos": [
                    {
                        "file_name": "match_001.dem",
                        "s3_bucket": "test-bucket",
                        "s3_key": "demos/match_001.dem",
                        "file_size_bytes": 50000000
                    },
                    {
                        "file_name": "match_002.dem",
                        "s3_bucket": "test-bucket",
                        "s3_key": "demos/match_002.dem",
                        "file_size_bytes": 60000000
                    }
                ]
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let data = &body["data"];

    assert_eq!(data["created"].as_array().unwrap().len(), 2);
    assert!(data["existing"].as_array().unwrap().is_empty());
    assert!(data["errors"].as_array().unwrap().is_empty());

    // Verify first created demo has correct fields
    let demo = &data["created"][0];
    assert_eq!(demo["file_name"], "match_001.dem");
    assert_eq!(demo["s3_bucket"], "test-bucket");
    assert_eq!(demo["s3_key"], "demos/match_001.dem");
    assert_eq!(demo["status"], "pending");
}

/// Test batch cataloging is idempotent — re-cataloging same S3 keys returns existing.
#[tokio::test]
async fn test_batch_catalog_demos_idempotent() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    let request_body = json!({
        "game_id": game_id.to_string(),
        "demos": [
            {
                "file_name": "match_idem.dem",
                "s3_bucket": "test-bucket",
                "s3_key": "demos/match_idem.dem",
                "file_size_bytes": 50000000
            }
        ]
    });

    // First call — should create
    let response = app.post_json("/v1/admin/demos/batch", &request_body).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["created"].as_array().unwrap().len(), 1);
    assert!(body["data"]["existing"].as_array().unwrap().is_empty());

    let created_id = body["data"]["created"][0]["id"].as_str().unwrap().to_string();

    // Second call — same S3 key, should return as existing
    let response = app.post_json("/v1/admin/demos/batch", &request_body).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["created"].as_array().unwrap().is_empty());
    assert_eq!(body["data"]["existing"].as_array().unwrap().len(), 1);

    // Same ID returned
    let existing_id = body["data"]["existing"][0]["id"].as_str().unwrap();
    assert_eq!(existing_id, created_id);
}

/// Test submitting stats for a pending demo transitions it to ready.
#[tokio::test]
async fn test_submit_demo_stats() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    // First catalog a demo
    let response = app
        .post_json(
            "/v1/admin/demos/batch",
            &json!({
                "game_id": game_id.to_string(),
                "demos": [{
                    "file_name": "stats_test.dem",
                    "s3_bucket": "test-bucket",
                    "s3_key": "demos/stats_test.dem",
                    "file_size_bytes": 40000000
                }]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["created"][0]["id"].as_str().unwrap();

    // Submit stats
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &json!({
                "map_name": "de_dust2",
                "match_date": "2025-01-15T18:00:00Z",
                "duration_seconds": 2400,
                "team1_name": "Team Alpha",
                "team2_name": "Team Beta",
                "team1_score": 16,
                "team2_score": 12,
                "total_rounds": 28,
                "raw_stats": { "source": "test" },
                "players": [
                    {
                        "steam_id": "76561198000000001",
                        "player_name": "Player1",
                        "team_name": "Team Alpha",
                        "stats": { "kills": 25, "deaths": 18, "assists": 5 }
                    },
                    {
                        "steam_id": "76561198000000002",
                        "player_name": "Player2",
                        "team_name": "Team Beta",
                        "stats": { "kills": 18, "deaths": 22, "assists": 3 }
                    }
                ]
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demo = &body["data"];

    assert_eq!(demo["status"], "ready");

    // Verify players were created
    let response = app
        .get_auth(&format!("/v1/demos/{demo_id}/players"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let players = body["data"].as_array().unwrap();
    assert_eq!(players.len(), 2);

    // CS2 typed stats should be extracted
    let player1 = players
        .iter()
        .find(|p| p["steam_id"] == "76561198000000001")
        .expect("Player1 should exist");
    assert_eq!(player1["stats"]["kills"], 25);
    assert_eq!(player1["stats"]["deaths"], 18);
}

/// Test re-submitting stats is idempotent — replaces existing players.
#[tokio::test]
async fn test_submit_demo_stats_idempotent() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    // Catalog a demo
    let response = app
        .post_json(
            "/v1/admin/demos/batch",
            &json!({
                "game_id": game_id.to_string(),
                "demos": [{
                    "file_name": "idem_stats.dem",
                    "s3_bucket": "test-bucket",
                    "s3_key": "demos/idem_stats.dem",
                    "file_size_bytes": 40000000
                }]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["created"][0]["id"].as_str().unwrap();

    // Submit stats with 2 players
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &json!({
                "map_name": "de_inferno",
                "team1_score": 13,
                "team2_score": 16,
                "raw_stats": { "version": 1 },
                "players": [
                    { "steam_id": "111", "player_name": "A", "stats": { "kills": 10 } },
                    { "steam_id": "222", "player_name": "B", "stats": { "kills": 20 } }
                ]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Verify 2 players
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/players")).await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);

    // Re-submit with 3 players (different set)
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &json!({
                "map_name": "de_inferno",
                "team1_score": 13,
                "team2_score": 16,
                "raw_stats": { "version": 2 },
                "players": [
                    { "steam_id": "333", "player_name": "C", "stats": { "kills": 5 } },
                    { "steam_id": "444", "player_name": "D", "stats": { "kills": 15 } },
                    { "steam_id": "555", "player_name": "E", "stats": { "kills": 25 } }
                ]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Verify now 3 players (old ones were deleted)
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/players")).await;
    let body: serde_json::Value = response.json();
    let players = body["data"].as_array().unwrap();
    assert_eq!(players.len(), 3);

    // Verify old players are gone
    let steam_ids: Vec<&str> = players
        .iter()
        .map(|p| p["steam_id"].as_str().unwrap())
        .collect();
    assert!(!steam_ids.contains(&"111"));
    assert!(!steam_ids.contains(&"222"));
    assert!(steam_ids.contains(&"333"));
    assert!(steam_ids.contains(&"444"));
    assert!(steam_ids.contains(&"555"));
}

/// Test marking a demo as failed stores the error.
#[tokio::test]
async fn test_mark_demo_stats_failed() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    // Catalog a demo
    let response = app
        .post_json(
            "/v1/admin/demos/batch",
            &json!({
                "game_id": game_id.to_string(),
                "demos": [{
                    "file_name": "fail_test.dem",
                    "s3_bucket": "test-bucket",
                    "s3_key": "demos/fail_test.dem",
                    "file_size_bytes": 30000000
                }]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["created"][0]["id"].as_str().unwrap();

    // Mark as failed
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats-failed"),
            &json!({ "error": "Demo parsing failed: corrupt header" }),
        )
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "failed");

    // Verify via GET
    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "failed");
}

/// Test evidence discovery endpoint returns results (integration smoke test).
///
/// This test verifies the endpoint works end-to-end with catalog-based discovery
/// wired in. Since the current `build_evidence_context` doesn't populate steam_ids
/// from player profiles, catalog results are empty — but the endpoint should
/// still return 200 with the plugin results.
#[tokio::test]
async fn test_discover_evidence_finds_catalog_demos() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    // Create a tournament with a match
    let info = create_cs2_tournament_with_match(&app, "discover-catalog").await;

    // Catalog a demo with players that would match
    let response = app
        .post_json(
            "/v1/admin/demos/batch",
            &json!({
                "game_id": game_id.to_string(),
                "demos": [{
                    "file_name": "discover_test.dem",
                    "s3_bucket": "test-bucket",
                    "s3_key": "demos/discover_test.dem",
                    "file_size_bytes": 50000000
                }]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["created"][0]["id"].as_str().unwrap();

    // Submit stats to make it ready
    app.post_json(
        &format!("/v1/admin/demos/{demo_id}/stats"),
        &json!({
            "map_name": "de_dust2",
            "match_date": chrono::Utc::now().to_rfc3339(),
            "team1_score": 16,
            "team2_score": 14,
            "raw_stats": {},
            "players": [
                { "steam_id": "76561198000000001", "player_name": "P1", "stats": {} },
                { "steam_id": "76561198000000002", "player_name": "P2", "stats": {} }
            ]
        }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Call discover endpoint — should return 200 even though catalog results
    // are empty (no steam_ids in context yet). Verifies the merged pipeline works.
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence/discover",
            info.match_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().is_some());
}

/// Test linking catalog-discovered evidence creates both Evidence and DemoMatchLink.
#[tokio::test]
async fn test_link_catalog_discovered_evidence() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    // Create a tournament with a match
    let info = create_cs2_tournament_with_match(&app, "link-catalog").await;

    // Catalog and submit stats for a demo
    let response = app
        .post_json(
            "/v1/admin/demos/batch",
            &json!({
                "game_id": game_id.to_string(),
                "demos": [{
                    "file_name": "link_test.dem",
                    "s3_bucket": "test-bucket",
                    "s3_key": "demos/link_test.dem",
                    "file_size_bytes": 50000000
                }]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["created"][0]["id"].as_str().unwrap();

    // Submit stats so demo is ready
    app.post_json(
        &format!("/v1/admin/demos/{demo_id}/stats"),
        &json!({
            "map_name": "de_inferno",
            "team1_score": 16,
            "team2_score": 10,
            "raw_stats": {},
            "players": [
                { "steam_id": "76561198000000010", "player_name": "A", "stats": {} }
            ]
        }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Link the catalog demo as discovered evidence using catalog:{demo_id} format
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-discovered", info.match_id),
            &json!({
                "external_id": format!("catalog:{demo_id}"),
                "game_number": 1
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence = &body["data"];
    assert_eq!(evidence["evidence_type"], "demo");
    assert!(evidence["id"].as_str().is_some());

    // Verify a DemoMatchLink was created
    let response = app
        .get_auth(&format!("/v1/demos/{demo_id}/links"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let links = body["data"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["match_id"], info.match_id);
    assert_eq!(links[0]["link_type"], "evidence");
}

// ============================================================================
// TOURNAMENT + MATCH SETUP HELPER (same pattern as evidence_test.rs)
// ============================================================================

#[allow(dead_code)]
struct TestMatchInfo {
    tournament_id: String,
    match_id: String,
    participant1_reg_id: String,
    participant2_reg_id: String,
}

async fn create_cs2_tournament_with_match(app: &TestApp, slug: &str) -> TestMatchInfo {
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id.to_string(),
                "name": format!("Demo Test {slug}"),
                "slug": slug,
                "format": "single_elimination",
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();
    let tournament_uuid: uuid::Uuid = tournament_id.parse().unwrap();

    // Publish
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);

    // Open registration
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/open-registration"))
        .await
        .assert_status(StatusCode::OK);

    // Register player 1 (dev user)
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Player1" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let reg1 = body["data"]["id"].as_str().unwrap().to_string();

    // Approve registration 1
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/registrations/{reg1}/approve"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Register player 2 via builder
    let user2 = UserBuilder::new()
        .username(&format!("player2_{slug}"))
        .build_persisted(app.pool())
        .await;

    let _reg2 = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament_uuid)
        .player_id_from_uuid(user2.id)
        .participant_name("Player2")
        .registered_by_uuid(user2.id)
        .approved()
        .build_persisted(app.pool())
        .await;

    // Seed and start
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    // Get match info
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/matches"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let matches = body["data"].as_array().unwrap();
    assert!(!matches.is_empty(), "Tournament should have at least one match");

    let match_data = &matches[0];
    let match_id = match_data["id"].as_str().unwrap().to_string();
    let participant1_reg_id = match_data["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();
    let participant2_reg_id = match_data["participant2_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    TestMatchInfo {
        tournament_id,
        match_id,
        participant1_reg_id,
        participant2_reg_id,
    }
}
