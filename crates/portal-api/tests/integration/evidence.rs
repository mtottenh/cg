//! Evidence API integration tests.


use axum::http::StatusCode;
use crate::common::TestApp;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

// ============================================================================
// TOURNAMENT + MATCH SETUP HELPERS
// ============================================================================

/// Match info returned by the helper.
#[allow(dead_code)]
struct TestMatchInfo {
    tournament_id: String,
    match_id: String,
    participant1_reg_id: String,
    participant2_reg_id: String,
}

/// Helper to create a started CS2 tournament with matches.
async fn create_cs2_tournament_with_match(app: &TestApp, slug: &str) -> TestMatchInfo {
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id.to_string(),
                "name": format!("Evidence Test {}", slug),
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
    let tournament_uuid: Uuid = tournament_id.parse().unwrap();

    // Publish
    app.post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Open registration
    app.post_auth(&format!("/v1/tournaments/{}/open-registration", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Register player 1 (dev user, via API)
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/registrations/player", tournament_id),
            &json!({ "participant_name": "Player1" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let reg1 = body["data"]["id"].as_str().unwrap().to_string();

    // Approve registration 1
    app.post_auth(&format!(
        "/v1/tournaments/{}/registrations/{}/approve",
        tournament_id, reg1
    ))
    .await
    .assert_status(StatusCode::OK);

    // Register player 2 (via builder)
    let user2 = UserBuilder::new()
        .username(&format!("player2_{}", slug))
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
        &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    app.post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Get match info
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
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

// ============================================================================
// EVIDENCE UPLOAD TESTS
// ============================================================================

#[tokio::test]
async fn test_initiate_upload_invalid_match_id() {
    let app = TestApp::new().await;

    // Try to initiate upload for non-existent match
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/upload",
            &json!({
                "evidence_type": "demo",
                "file_name": "test_demo.dem",
                "file_size_bytes": 1024000,
                "mime_type": "application/octet-stream"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_initiate_upload_invalid_evidence_type() {
    let app = TestApp::new().await;

    // Try to initiate upload with invalid evidence type
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/upload",
            &json!({
                "evidence_type": "invalid_type",
                "file_name": "test.txt",
                "file_size_bytes": 1024,
                "mime_type": "text/plain"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_complete_upload_invalid_evidence_id() {
    let app = TestApp::new().await;

    // Try to complete upload for non-existent evidence
    let response = app
        .post_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001/complete",
        )
        .await;

    // Should return 404 (evidence not found)
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// EVIDENCE LINK TESTS
// ============================================================================

#[tokio::test]
async fn test_add_link_evidence_invalid_match_id() {
    let app = TestApp::new().await;

    // Try to add link evidence for non-existent match
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link",
            &json!({
                "evidence_type": "video",
                "url": "https://www.youtube.com/watch?v=test123",
                "name": "Match VOD"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_add_link_evidence_invalid_evidence_type() {
    let app = TestApp::new().await;

    // Try to add link evidence with invalid type (demo can't be a link)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link",
            &json!({
                "evidence_type": "demo",
                "url": "https://example.com/demo",
                "name": "Invalid demo link"
            }),
        )
        .await;

    // Should fail because demo type cannot be a URL
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// EVIDENCE RETRIEVAL TESTS
// ============================================================================

#[tokio::test]
async fn test_list_evidence_for_nonexistent_match() {
    let app = TestApp::new().await;

    // List evidence for non-existent match
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/evidence")
        .await;

    // Returns 200 with empty array (endpoint doesn't verify match existence)
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_get_evidence_not_found() {
    let app = TestApp::new().await;

    // Get non-existent evidence
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_access_url_not_found() {
    let app = TestApp::new().await;

    // Get access URL for non-existent evidence
    let response = app
        .get_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001/access",
        )
        .await;

    // Should return 404 (evidence not found)
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_evidence_not_found() {
    let app = TestApp::new().await;

    // Delete non-existent evidence
    let response = app
        .delete_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001",
        )
        .await;

    // Should return 404 (evidence not found)
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// EVIDENCE DISCOVERY TESTS (Plugin-Wired)
// ============================================================================

#[tokio::test]
async fn test_discover_evidence_match_not_found() {
    let app = TestApp::new().await;

    // Discovery for non-existent match should return 404
    let response = app
        .get_auth("/v1/matches/00000000-0000-0000-0000-000000000000/evidence/discover")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_discover_evidence_returns_empty_for_valid_match() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "discover-empty").await;

    // CS2 plugin returns empty discovery (S3 scanning not yet implemented)
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence/discover",
            info.match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let data = body["data"].as_array().unwrap();
    assert!(data.is_empty(), "CS2 plugin should return empty discovery list");
}

#[tokio::test]
async fn test_discover_evidence_with_query_filters() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "discover-filters").await;

    // Query with min_relevance and limit filters should still work (empty results)
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence/discover?min_relevance=0.5&limit=10",
            info.match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

// ============================================================================
// LINK DISCOVERED EVIDENCE TESTS (Plugin-Wired)
// ============================================================================

#[tokio::test]
async fn test_link_discovered_evidence_match_not_found() {
    let app = TestApp::new().await;

    // Linking for non-existent match should return 404
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link-discovered",
            &json!({
                "external_id": "demo_12345"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_link_discovered_evidence_external_id_not_found() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "link-notfound").await;

    // CS2 discovery returns empty, so any external_id will not be found
    let response = app
        .post_json(
            &format!(
                "/v1/matches/{}/evidence/link-discovered",
                info.match_id
            ),
            &json!({
                "external_id": "nonexistent_demo_12345"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
    let body: serde_json::Value = response.json();
    let detail = body["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("nonexistent_demo_12345"),
        "Error should mention the external_id that was not found, got: {}",
        detail
    );
}

#[tokio::test]
async fn test_link_discovered_evidence_with_game_number() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "link-gamenum").await;

    // Even with game_number, the external_id must exist in discovery
    let response = app
        .post_json(
            &format!(
                "/v1/matches/{}/evidence/link-discovered",
                info.match_id
            ),
            &json!({
                "external_id": "demo_99999",
                "game_number": 1
            }),
        )
        .await;

    // Should fail because CS2 discovery returns empty
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// EVIDENCE VALIDATION TESTS (Plugin-Wired)
// ============================================================================

#[tokio::test]
async fn test_validate_evidence_match_not_found() {
    let app = TestApp::new().await;

    // Validation for non-existent match should return 404
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/validate",
            &json!({
                "evidence_ids": ["00000000-0000-0000-0000-000000000001"],
                "expected_participant1_score": 2,
                "expected_participant2_score": 1
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_validate_evidence_empty_evidence_ids() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "validate-empty-ids").await;

    // Validation with no evidence IDs should return 400
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/validate", info.match_id),
            &json!({
                "evidence_ids": [],
                "expected_participant1_score": 2,
                "expected_participant2_score": 1
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_validate_evidence_nonexistent_evidence_id() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "validate-noevidence").await;

    // Validation with a non-existent evidence ID should fail
    // (the evidence must exist in DB for validate_against_result to work)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/validate", info.match_id),
            &json!({
                "evidence_ids": [Uuid::new_v4().to_string()],
                "expected_participant1_score": 2,
                "expected_participant2_score": 1
            }),
        )
        .await;

    // Evidence not found in DB → 404 from domain layer
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_evidence_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000000";

    // Verify GET /evidence endpoint exists
    let response = app
        .get(&format!("/v1/matches/{}/evidence", match_id))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence endpoint should exist"
    );

    // Verify POST /evidence/upload endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/upload", match_id),
            &json!({
                "evidence_type": "demo",
                "file_name": "test.dem",
                "file_size_bytes": 1024,
                "mime_type": "application/octet-stream"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/upload endpoint should exist"
    );

    // Verify POST /evidence/link endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link", match_id),
            &json!({
                "evidence_type": "video",
                "url": "https://example.com/video",
                "name": "Test video"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/link endpoint should exist"
    );
}

#[tokio::test]
async fn test_evidence_detail_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000000";
    let evidence_id = "00000000-0000-0000-0000-000000000001";

    // Verify GET /evidence/{evidence_id} endpoint exists
    let response = app
        .get(&format!(
            "/v1/matches/{}/evidence/{}",
            match_id, evidence_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/{{id}} endpoint should exist"
    );

    // Verify GET /evidence/{evidence_id}/access endpoint exists (authenticated)
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence/{}/access",
            match_id, evidence_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/{{id}}/access endpoint should exist"
    );

    // Verify DELETE /evidence/{evidence_id} endpoint exists (authenticated)
    let response = app
        .delete_auth(&format!(
            "/v1/matches/{}/evidence/{}",
            match_id, evidence_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "DELETE /evidence/{{id}} endpoint should exist"
    );
}

// ============================================================================
// PLUGIN-WIRED ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_plugin_evidence_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000000";

    // Verify GET /evidence/discover endpoint exists (returns 404, not 405)
    let response = app
        .get_auth(&format!("/v1/matches/{}/evidence/discover", match_id))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/discover endpoint should exist"
    );

    // Verify POST /evidence/link-discovered endpoint exists (returns 404, not 405)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-discovered", match_id),
            &json!({ "external_id": "test" }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/link-discovered endpoint should exist"
    );

    // Verify POST /evidence/validate endpoint exists (returns 404, not 405)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/validate", match_id),
            &json!({
                "evidence_ids": ["00000000-0000-0000-0000-000000000001"]
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/validate endpoint should exist"
    );
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_initiate_upload_missing_required_fields() {
    let app = TestApp::new().await;

    // Try to initiate upload without required fields
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/upload",
            &json!({
                "evidence_type": "demo"
                // Missing file_name, file_size_bytes, mime_type
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_link_evidence_missing_url() {
    let app = TestApp::new().await;

    // Try to add link evidence without URL
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link",
            &json!({
                "evidence_type": "video",
                "name": "Test video"
                // Missing url
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}
