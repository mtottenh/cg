//! Demo catalog API integration tests.
//!
//! Tests cover:
//! - Category A: Demo catalog browsing and management
//! - Category B: Demo-match linking operations
//! - Category C: Batch catalog and stats ingestion API

use crate::common::TestApp;
use axum::http::StatusCode;
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

/// Test getting demo players for a non-existent demo returns 404.
#[tokio::test]
async fn test_get_demo_players_empty() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/demos/00000000-0000-0000-0000-000000000000/players")
        .await;

    // The endpoint verifies demo existence: missing demo is a 404, not an
    // empty list.
    response.assert_status(StatusCode::NOT_FOUND);
}

/// Test getting demo links for a non-existent demo returns 404.
#[tokio::test]
async fn test_get_demo_links_empty() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/demos/00000000-0000-0000-0000-000000000000/links")
        .await;

    // The endpoint verifies demo existence: missing demo is a 404, not an
    // empty list.
    response.assert_status(StatusCode::NOT_FOUND);
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
    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
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

    let created_id = body["data"]["created"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

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
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/players")).await;
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
        .get_auth(&format!("/v1/matches/{}/evidence/discover", info.match_id))
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
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
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
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
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
        .username(format!("player2_{slug}"))
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
    assert!(
        !matches.is_empty(),
        "Tournament should have at least one match"
    );

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

// ============================================================================
// CATEGORY D: ADMIN DEMO VERBS (catalog / notes / categorize / visibility /
// associate / status counts / pending / download)
// ============================================================================

/// Helper: catalog a single demo via POST /v1/admin/demos and return its ID.
async fn catalog_single_demo(app: &TestApp, s3_key: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/admin/demos",
            &json!({
                "game_id": game_id.to_string(),
                "file_name": s3_key.rsplit('/').next().unwrap(),
                "s3_bucket": "test-bucket",
                "s3_key": s3_key,
                "file_size_bytes": 42000000
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// Test single-demo cataloging: 201 on create, 200 (same demo) on re-catalog.
#[tokio::test]
async fn test_catalog_demo_single() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let request_body = json!({
        "game_id": game_id.to_string(),
        "file_name": "single_catalog.dem",
        "s3_bucket": "test-bucket",
        "s3_key": "demos/single_catalog.dem",
        "file_size_bytes": 12345678
    });

    let response = app.post_json("/v1/admin/demos", &request_body).await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["file_name"], "single_catalog.dem");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["category"], "uncategorized");

    // Re-cataloging the same S3 key is idempotent: 200 with the same demo
    let response = app.post_json("/v1/admin/demos", &request_body).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], demo_id);
}

/// Test setting and clearing admin notes.
#[tokio::test]
async fn test_set_demo_notes() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let demo_id = catalog_single_demo(&app, "demos/notes_test.dem").await;

    // Set notes
    let response = app
        .patch_json(
            &format!("/v1/admin/demos/{demo_id}/notes"),
            &json!({ "notes": "Suspicious rounds 3-7" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["admin_notes"], "Suspicious rounds 3-7");

    // Visible via GET /v1/demos/{id}
    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["admin_notes"], "Suspicious rounds 3-7");

    // Null clears the notes
    let response = app
        .patch_json(
            &format!("/v1/admin/demos/{demo_id}/notes"),
            &json!({ "notes": null }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["admin_notes"].is_null());
}

/// Test categorizing a demo records category and auditing fields.
#[tokio::test]
async fn test_categorize_demo() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let demo_id = catalog_single_demo(&app, "demos/categorize_test.dem").await;

    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/categorize"),
            &json!({ "category": "league" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["category"], "league");
    assert!(body["data"]["categorized_by_user_id"].is_string());
    assert!(body["data"]["categorized_at"].is_string());

    // Effect is visible via GET
    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["category"], "league");

    // Unknown category is rejected
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/categorize"),
            &json!({ "category": "not-a-category" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

/// Test hiding and unhiding a demo.
#[tokio::test]
async fn test_set_demo_visibility() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let demo_id = catalog_single_demo(&app, "demos/visibility_test.dem").await;

    // Hide
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/visibility"),
            &json!({ "is_hidden": true }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["is_hidden"], true);
    assert!(body["data"]["hidden_by_user_id"].is_string());

    // Admins can still read a hidden demo
    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["is_hidden"], true);

    // A regular user (not linked to the demo) is refused
    let user2 = UserBuilder::new()
        .username("demo-viewer")
        .email("demo-viewer@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_test_token(user2.id, user2.id, "demo-viewer", TEST_JWT_SECRET);
    let response = app
        .get_with_token(&format!("/v1/demos/{demo_id}"), &token2)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Unhide
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/visibility"),
            &json!({ "is_hidden": false }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["is_hidden"], false);
}

/// Test associating a demo with a league and a tournament.
#[tokio::test]
async fn test_associate_demo() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let demo_id = catalog_single_demo(&app, "demos/associate_test.dem").await;

    let league = LeagueBuilder::new()
        .name("Demo Associate League")
        .build_persisted(app.pool())
        .await;

    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/associate"),
            &json!({ "league_id": league.id.to_string() }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["league_id"], league.id.to_string());

    // Effect is visible via GET
    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["league_id"], league.id.to_string());
}

/// Test the admin status-count dashboard endpoint.
#[tokio::test]
async fn test_get_demo_status_counts() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let demo1 = catalog_single_demo(&app, "demos/counts_1.dem").await;
    let _demo2 = catalog_single_demo(&app, "demos/counts_2.dem").await;

    // Fail one so we exercise more than one bucket
    app.post_json(
        &format!("/v1/admin/demos/{demo1}/stats-failed"),
        &json!({ "error": "corrupt header" }),
    )
    .await
    .assert_status(StatusCode::OK);

    let response = app.get_auth("/v1/admin/demos/stats").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["pending"], 1);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["ready"], 0);
    assert_eq!(body["data"]["processing"], 0);
    assert_eq!(body["data"]["archived"], 0);
}

/// Test listing demos pending processing.
#[tokio::test]
async fn test_get_pending_demos() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let demo_id = catalog_single_demo(&app, "demos/pending_list.dem").await;

    let response = app.get_auth("/v1/admin/demos/pending").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demos = body["data"].as_array().unwrap();
    assert_eq!(demos.len(), 1);
    assert_eq!(demos[0]["id"], demo_id);
    assert_eq!(demos[0]["status"], "pending");

    // Limit parameter is honored
    let response = app.get_auth("/v1/admin/demos/pending?limit=0").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

/// Test the demo download endpoint returns S3 coordinates and a URL.
#[tokio::test]
async fn test_get_demo_download() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    let demo_id = catalog_single_demo(&app, "demos/download_test.dem").await;

    let response = app.get_auth(&format!("/v1/demos/{demo_id}/download")).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], demo_id);
    assert_eq!(body["data"]["file_name"], "download_test.dem");
    assert_eq!(body["data"]["s3_bucket"], "test-bucket");
    assert_eq!(body["data"]["s3_key"], "demos/download_test.dem");

    // Without a configured demo-service base URL the handler falls back to
    // s3:// coordinates; either way the URL embeds bucket and key.
    let download_url = body["data"]["download_url"].as_str().unwrap();
    assert!(download_url.contains("test-bucket"));
    assert!(download_url.contains("demos/download_test.dem"));

    // Missing demo is a 404
    let response = app
        .get_auth("/v1/demos/00000000-0000-0000-0000-000000000000/download")
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// MATCH DEMO LISTING VISIBILITY
// ============================================================================

/// Hidden demos must not leak through the match demo listing: only admins
/// (and players appearing in the demo) see them.
#[tokio::test]
async fn test_hidden_demos_filtered_from_match_listing() {
    let app = TestApp::new().await;
    // Helper assigns platform_admin to the dev user, so admin demo-catalog
    // endpoints work with the dev token.
    let (_tournament_id, match_id, _reg1, _reg2) =
        crate::tournaments::create_tournament_with_matches(&app, "hidden-demo-match").await;

    let demo_id = catalog_single_demo(&app, "demos/hidden_match_demo.dem").await;

    // Link the demo to the match.
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/link"),
            &json!({ "match_id": match_id }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Hide the demo.
    app.post_json(
        &format!("/v1/admin/demos/{demo_id}/visibility"),
        &json!({ "is_hidden": true }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Admin (dev token) still sees the hidden demo in the match listing.
    let response = app.get_auth(&format!("/v1/matches/{match_id}/demos")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // A regular user (not in the demo) does not.
    let viewer = UserBuilder::new()
        .username("match-demo-viewer")
        .build_persisted(app.pool())
        .await;
    let viewer_token =
        create_test_token(viewer.id, viewer.id, "match-demo-viewer", TEST_JWT_SECRET);
    let response = app
        .get_with_token(&format!("/v1/matches/{match_id}/demos"), &viewer_token)
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        0,
        "Hidden demo should be filtered for non-admins"
    );

    // Unhide — the regular user sees it again.
    app.post_json(
        &format!("/v1/admin/demos/{demo_id}/visibility"),
        &json!({ "is_hidden": false }),
    )
    .await
    .assert_status(StatusCode::OK);
    let response = app
        .get_with_token(&format!("/v1/matches/{match_id}/demos"), &viewer_token)
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
}

// ============================================================================
// CATEGORY E: DEMO -> TOURNAMENT MATCH AUTO-LINKING
// ============================================================================

/// Set a player's `steam_id_64` from a tournament registration ID.
async fn set_registration_steam_id(app: &TestApp, registration_id: &str, steam_id: i64) {
    let reg_uuid = uuid::Uuid::parse_str(registration_id).unwrap();
    sqlx::query(
        "UPDATE players SET steam_id_64 = $1
         WHERE id = (SELECT player_id FROM tournament_registrations WHERE id = $2)",
    )
    .bind(steam_id)
    .bind(reg_uuid)
    .execute(app.pool())
    .await
    .expect("failed to set steam_id_64");
}

/// Build a minimal Cs2DemoStats-shaped stats submission body whose
/// `raw_stats.player_summaries` is keyed by the given Steam IDs.
fn auto_link_stats_body(steam_ids: &[&str], match_date: &str) -> serde_json::Value {
    let mut player_summaries = serde_json::Map::new();
    let mut players = Vec::new();
    for (i, sid) in steam_ids.iter().enumerate() {
        player_summaries.insert(
            (*sid).to_string(),
            json!({
                "player_id": sid,
                "player_name": format!("Player{}", i + 1),
                "team": { "team_id": 1, "team_name": "TeamA", "team_side": "CT" },
                "kills": 10, "deaths": 5, "assists": 2,
                "headshot_kills": 4, "damage_dealt": 800,
                "adr": 80.0, "hs_percentage": 40.0
            }),
        );
        players.push(json!({
            "steam_id": sid,
            "player_name": format!("Player{}", i + 1),
            "team_name": "TeamA",
            "stats": { "kills": 10, "deaths": 5 }
        }));
    }
    json!({
        "map_name": "de_dust2",
        "match_date": match_date,
        "team1_name": "TeamA",
        "team2_name": "TeamB",
        "team1_score": 13,
        "team2_score": 7,
        "total_rounds": 20,
        "raw_stats": {
            "map": "de_dust2",
            "match_date": match_date,
            "player_summaries": player_summaries
        },
        "players": players
    })
}

/// Full flow: stats submission auto-links the demo to a scheduled match with
/// full-overlap confidence, stamps the tournament, and resolves player IDs.
#[tokio::test]
async fn test_auto_link_demo_on_stats_submission() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, _token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "auto-link-flow")
            .await;

    // Schedule the match at time T (helper already made the dev user admin).
    let t = chrono::Utc::now() + chrono::Duration::hours(1);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": t.to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Give both participants Steam IDs.
    set_registration_steam_id(&app, &reg1, 76_561_198_000_000_101).await;
    set_registration_steam_id(&app, &reg2, 76_561_198_000_000_102).await;

    // Catalog a demo and submit stats featuring exactly those players.
    let demo_id = catalog_single_demo(&app, "demos/auto_link_flow.dem").await;
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &auto_link_stats_body(&["76561198000000101", "76561198000000102"], &t.to_rfc3339()),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();

    // The returned demo is stamped with the tournament.
    assert_eq!(body["data"]["tournament_id"], tournament_id);

    // An auto_matched link with confidence 1.0 exists.
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let links = body["data"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["match_id"], match_id);
    assert_eq!(links[0]["link_type"], "auto_matched");
    let confidence = links[0]["confidence_score"].as_f64().unwrap();
    assert!(
        (confidence - 1.0).abs() < 1e-6,
        "expected confidence 1.0, got {confidence}"
    );

    // Demo players were resolved to portal player accounts.
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/players")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let players = body["data"].as_array().unwrap();
    assert_eq!(players.len(), 2);
    for player in players {
        assert!(
            player["player_id"].is_string(),
            "demo player {} should be resolved to a portal player",
            player["steam_id"]
        );
    }
}

/// Negative: no Steam-ID overlap between demo and match participants — no link.
#[tokio::test]
async fn test_auto_link_no_steam_id_overlap() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, _token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "auto-link-no-ovl")
            .await;

    let t = chrono::Utc::now() + chrono::Duration::hours(1);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": t.to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);

    set_registration_steam_id(&app, &reg1, 76_561_198_000_000_201).await;
    set_registration_steam_id(&app, &reg2, 76_561_198_000_000_202).await;

    // Demo features entirely different players.
    let demo_id = catalog_single_demo(&app, "demos/auto_link_no_overlap.dem").await;
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &auto_link_stats_body(&["76561198000000901", "76561198000000902"], &t.to_rfc3339()),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["tournament_id"].is_null());

    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

/// Negative: match date far outside the 24h window around the scheduled
/// time — no link, even with full Steam-ID overlap.
#[tokio::test]
async fn test_auto_link_outside_time_window() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, _token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "auto-link-window")
            .await;

    let t = chrono::Utc::now() + chrono::Duration::hours(1);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": t.to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);

    set_registration_steam_id(&app, &reg1, 76_561_198_000_000_301).await;
    set_registration_steam_id(&app, &reg2, 76_561_198_000_000_302).await;

    // Same players, but the demo was played 10 days after the match slot.
    let far_away = t + chrono::Duration::days(10);
    let demo_id = catalog_single_demo(&app, "demos/auto_link_window.dem").await;
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &auto_link_stats_body(
                &["76561198000000301", "76561198000000302"],
                &far_away.to_rfc3339(),
            ),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["tournament_id"].is_null());

    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

/// Backfill: stats submitted before any match existed stay unlinked; once the
/// match is created and scheduled, `POST /v1/admin/demos/process-unlinked`
/// links the demo.
#[tokio::test]
async fn test_process_unlinked_backfill() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;

    // Catalog + submit stats while no matches exist yet.
    let t = chrono::Utc::now() + chrono::Duration::hours(1);
    let demo_id = catalog_single_demo(&app, "demos/auto_link_backfill.dem").await;
    app.post_json(
        &format!("/v1/admin/demos/{demo_id}/stats"),
        &auto_link_stats_body(&["76561198000000401", "76561198000000402"], &t.to_rfc3339()),
    )
    .await
    .assert_status(StatusCode::OK);

    // Nothing to link against yet.
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());

    // Now create the tournament, schedule its match, and wire up Steam IDs.
    let (tournament_id, match_id, reg1, reg2, _token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "auto-link-backfill")
            .await;
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": t.to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);
    set_registration_steam_id(&app, &reg1, 76_561_198_000_000_401).await;
    set_registration_steam_id(&app, &reg2, 76_561_198_000_000_402).await;

    // Run the backfill pass.
    let response = app.post_auth("/v1/admin/demos/process-unlinked").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["examined"], 1);
    assert_eq!(body["data"]["linked"], 1);
    assert_eq!(body["data"]["skipped"], 0);

    // The link now exists and the demo is stamped.
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    let body: serde_json::Value = response.json();
    let links = body["data"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["match_id"], match_id);
    assert_eq!(links[0]["link_type"], "auto_matched");

    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["tournament_id"], tournament_id);
}

/// The backfill endpoint is admin-gated.
#[tokio::test]
async fn test_process_unlinked_requires_admin() {
    let app = TestApp::new().await;

    let response = app.post_auth("/v1/admin/demos/process-unlinked").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// DEMO LINK ADMINISTRATION (correction) + RBAC
// ============================================================================

/// Catalog a demo via the admin API; returns its id.
async fn catalog_test_demo(app: &TestApp, key_suffix: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/admin/demos",
            &json!({
                "game_id": game_id,
                "file_name": format!("link-admin-{key_suffix}.dem"),
                "s3_bucket": "portal-demos",
                "s3_key": format!("demos/link-admin-{key_suffix}.dem.bz2"),
                "file_size_bytes": 1000
            }),
        )
        .await;
    assert!(
        response.status == StatusCode::CREATED || response.status == StatusCode::OK,
        "catalog failed: {}",
        response.text()
    );
    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// Manual admin link stamps the demo's tournament; unlinking the last link
/// to that tournament clears the stamp — the correction flow admins use
/// when the auto-linker (or a human) got it wrong.
#[tokio::test]
async fn test_admin_link_stamps_and_unlink_clears_tournament() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let (tournament_id, match_id, _, _) =
        crate::tournaments::create_tournament_with_matches(&app, "link-admin-test").await;
    let demo_id = catalog_test_demo(&app, "stamp").await;

    // Link → stamped.
    app.post_json(
        &format!("/v1/admin/demos/{demo_id}/link"),
        &json!({ "match_id": match_id, "link_type": "manual" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["tournament_id"].as_str(),
        Some(tournament_id.as_str()),
        "manual link should stamp the tournament"
    );

    // Unlink → stamp cleared (it was the only link).
    app.delete_auth(&format!("/v1/admin/demos/{demo_id}/link/{match_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    let body: serde_json::Value = response.json();
    assert!(
        body["data"]["tournament_id"].is_null(),
        "unlinking the last link must clear the tournament stamp: {body}"
    );
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
}

/// Demo-catalog mutations require admin.demos.manage — a moderator
/// (users.view_all holder) can read dashboards but cannot link, unlink,
/// or otherwise mutate the catalog.
#[tokio::test]
async fn test_demo_mutations_denied_for_moderator() {
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let (_, match_id, _, _) =
        crate::tournaments::create_tournament_with_matches(&app, "link-rbac-test").await;
    let demo_id = catalog_test_demo(&app, "rbac").await;

    let moderator = UserBuilder::new()
        .username("demo_link_moderator")
        .build_persisted(app.pool())
        .await;
    assign_role_to_user(app.pool(), moderator.id, "moderator").await;
    let mod_token = create_test_token(
        moderator.id,
        moderator.id,
        "demo_link_moderator",
        TEST_JWT_SECRET,
    );

    // Moderator can read the admin pipeline dashboard (view gate)...
    app.get_with_token("/v1/admin/demos/stats", &mod_token)
        .await
        .assert_status(StatusCode::OK);

    // ...but every catalog mutation is 403.
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let attempts: Vec<(&str, String, Option<serde_json::Value>)> = vec![
        (
            "POST",
            "/v1/admin/demos".to_string(),
            Some(json!({
                "game_id": game_id,
                "file_name": "mod.dem",
                "s3_bucket": "b",
                "s3_key": "k.dem.bz2"
            })),
        ),
        (
            "POST",
            format!("/v1/admin/demos/{demo_id}/link"),
            Some(json!({ "match_id": match_id, "link_type": "manual" })),
        ),
        (
            "DELETE",
            format!("/v1/admin/demos/{demo_id}/link/{match_id}"),
            None,
        ),
        (
            "POST",
            format!("/v1/admin/demos/{demo_id}/visibility"),
            Some(json!({ "is_hidden": true })),
        ),
        ("DELETE", format!("/v1/admin/demos/{demo_id}"), None),
        ("POST", "/v1/admin/demos/process-unlinked".to_string(), None),
        (
            "PUT",
            "/v1/admin/demos/auto-link".to_string(),
            Some(json!({ "enabled": false })),
        ),
        ("GET", "/v1/admin/demos/auto-link".to_string(), None),
    ];
    for (method, uri, body) in attempts {
        let response = match (method, &body) {
            ("POST", Some(b)) => app.post_json_with_token(&uri, b, &mod_token).await,
            ("POST", None) => app.post_with_token(&uri, &mod_token).await,
            ("PUT", Some(b)) => app.put_json_with_token(&uri, b, &mod_token).await,
            ("GET", _) => app.get_with_token(&uri, &mod_token).await,
            ("DELETE", _) => app.delete_with_token(&uri, &mod_token).await,
            _ => unreachable!(),
        };
        assert_eq!(
            response.status,
            StatusCode::FORBIDDEN,
            "{method} {uri} should be 403 for moderator, got {}: {}",
            response.status,
            response.text()
        );
    }
}

/// Auto-link kill-switch: with the setting disabled, stats submission skips
/// auto-linking and the backfill endpoint refuses with 409; re-enabling and
/// running the backfill links the demo.
#[tokio::test]
async fn test_auto_link_toggle_disables_and_reenables() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, _token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "auto-link-toggle")
            .await;

    let t = chrono::Utc::now() + chrono::Duration::hours(1);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": t.to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);

    set_registration_steam_id(&app, &reg1, 76_561_198_000_000_401).await;
    set_registration_steam_id(&app, &reg2, 76_561_198_000_000_402).await;

    // Default setting is enabled.
    let response = app.get_auth("/v1/admin/demos/auto-link").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["enabled"], true);

    // Disable auto-linking.
    let response = app
        .put_json("/v1/admin/demos/auto-link", &json!({ "enabled": false }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["enabled"], false);

    // Full-overlap stats submission no longer links.
    let demo_id = catalog_single_demo(&app, "demos/auto_link_toggle.dem").await;
    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &auto_link_stats_body(&["76561198000000401", "76561198000000402"], &t.to_rfc3339()),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body["data"]["tournament_id"].is_null(),
        "demo must not be stamped while auto-linking is disabled"
    );
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());

    // The backfill endpoint refuses while disabled.
    app.post_auth("/v1/admin/demos/process-unlinked")
        .await
        .assert_status(StatusCode::CONFLICT);

    // Re-enable and backfill: the demo links and the tournament is stamped.
    app.put_json("/v1/admin/demos/auto-link", &json!({ "enabled": true }))
        .await
        .assert_status(StatusCode::OK);
    let response = app.post_auth("/v1/admin/demos/process-unlinked").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["linked"].as_i64().unwrap() >= 1);

    let response = app.get_auth(&format!("/v1/demos/{demo_id}/links")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let links = body["data"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["match_id"], match_id);

    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["tournament_id"], tournament_id);
}
