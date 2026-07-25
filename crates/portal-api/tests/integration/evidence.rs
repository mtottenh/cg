//! Evidence API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);

    // Open registration
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Register player 1 (dev user, via API)
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Player1" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let reg1 = body["data"]["id"].as_str().unwrap().to_string();

    // Registration 1 is already approved — the tournament is
    // `registration_type: open`, which auto-approves (P-2). The shared
    // helper is a no-op in that case and still works if the fixture ever
    // switches to an approval-gated tournament.
    crate::tournaments::approve_registration(app, &tournament_id, &reg1).await;

    // Register player 2 (via builder)
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
        .get_auth(&format!("/v1/matches/{}/evidence/discover", info.match_id))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let data = body["data"].as_array().unwrap();
    assert!(
        data.is_empty(),
        "CS2 plugin should return empty discovery list"
    );
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
            &format!("/v1/matches/{}/evidence/link-discovered", info.match_id),
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
        "Error should mention the external_id that was not found, got: {detail}"
    );
}

#[tokio::test]
async fn test_link_discovered_evidence_with_game_number() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "link-gamenum").await;

    // Even with game_number, the external_id must exist in discovery
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-discovered", info.match_id),
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
    let response = app.get(&format!("/v1/matches/{match_id}/evidence")).await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence endpoint should exist"
    );

    // Verify POST /evidence/upload endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/evidence/upload"),
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
            &format!("/v1/matches/{match_id}/evidence/link"),
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
        .get(&format!("/v1/matches/{match_id}/evidence/{evidence_id}"))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/{{id}} endpoint should exist"
    );

    // Verify GET /evidence/{evidence_id}/access endpoint exists (authenticated)
    let response = app
        .get_auth(&format!(
            "/v1/matches/{match_id}/evidence/{evidence_id}/access"
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/{{id}}/access endpoint should exist"
    );

    // Verify DELETE /evidence/{evidence_id} endpoint exists (authenticated)
    let response = app
        .delete_auth(&format!("/v1/matches/{match_id}/evidence/{evidence_id}"))
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
        .get_auth(&format!("/v1/matches/{match_id}/evidence/discover"))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/discover endpoint should exist"
    );

    // Verify POST /evidence/link-discovered endpoint exists (returns 404, not 405)
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/evidence/link-discovered"),
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
            &format!("/v1/matches/{match_id}/evidence/validate"),
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

// ============================================================================
// EVIDENCE AUTHORIZATION TESTS
// ============================================================================

/// Create a fresh non-participant user and JWT.
fn evidence_outsider_token(user_id: Uuid, username: &str) -> String {
    create_test_token(user_id, user_id, username, TEST_JWT_SECRET)
}

/// Non-participants must not be able to initiate uploads or attach link
/// evidence (previously the participant lookup error was swallowed).
#[tokio::test]
async fn test_evidence_mutations_by_nonparticipant_rejected() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "evidence-authz")
            .await;

    let outsider = UserBuilder::new()
        .username("evidence_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = evidence_outsider_token(outsider.id, "evidence_outsider");

    // Initiate upload as a non-participant is rejected.
    // Accept 401 or 403 while the NotAuthorized status-code mapping is in flight.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/upload"),
            &json!({
                "evidence_type": "demo",
                "file_name": "sneaky.dem",
                "file_size_bytes": 1024,
                "mime_type": "application/octet-stream"
            }),
            &outsider_token,
        )
        .await;
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::UNAUTHORIZED,
        "Outsider initiate_upload should be rejected, got {}",
        response.status
    );

    // Adding link evidence as a non-participant is rejected.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/link"),
            &json!({
                "evidence_type": "video",
                "url": "https://example.com/fake-vod",
                "name": "Fake VOD"
            }),
            &outsider_token,
        )
        .await;
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::UNAUTHORIZED,
        "Outsider add_link should be rejected, got {}",
        response.status
    );
}

/// P-108: `link-discovered` and `link-demo` took only `AuthenticatedUser`, so ANY
/// logged-in user could attach demo evidence to ANY match. Evidence feeds result
/// review and dispute resolution, so this is an integrity surface. Their sibling
/// `validate_demo` on the same surface already called
/// `require_match_participant_or_admin` — the gate existed and was never applied
/// here. Same shape as P-24 and P-59.
///
/// The assertion is on FORBIDDEN specifically, not "any non-2xx". Both endpoints
/// fail for unrelated reasons deeper in (CS2 discovery returns nothing; no
/// demo-stats service in the test env), so a laxer assertion would pass with no
/// gate at all — exactly the vacuous-guard failure this suite exists to prevent.
/// Verified to fail with NOT_FOUND before the fix.
#[tokio::test]
async fn test_demo_link_endpoints_by_nonparticipant_rejected() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "demo-link-authz")
            .await;

    let outsider = UserBuilder::new()
        .username("demo_link_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = evidence_outsider_token(outsider.id, "demo_link_outsider");

    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/link-discovered"),
            &json!({ "external_id": "catalog:00000000-0000-0000-0000-000000000000" }),
            &outsider_token,
        )
        .await;
    assert_eq!(
        response.status,
        StatusCode::FORBIDDEN,
        "Outsider link-discovered must be rejected by the participant gate, got {}",
        response.status
    );

    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/link-demo"),
            &json!({ "demo_name": "someone-elses-match.dem" }),
            &outsider_token,
        )
        .await;
    assert_eq!(
        response.status,
        StatusCode::FORBIDDEN,
        "Outsider link-demo must be rejected by the participant gate, got {}",
        response.status
    );
}

/// Participants can attach link evidence; only the uploader (or a match
/// participant / admin) can delete it — an outsider cannot.
#[tokio::test]
async fn test_evidence_delete_authorization() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "evidence-delete")
            .await;

    // Participant (player 2) adds link evidence.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/link"),
            &json!({
                "evidence_type": "video",
                "url": "https://example.com/match-vod",
                "name": "Match VOD"
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["id"].as_str().unwrap().to_string();

    // An outsider cannot delete it.
    let outsider = UserBuilder::new()
        .username("evidence_deleter")
        .build_persisted(app.pool())
        .await;
    let outsider_token = evidence_outsider_token(outsider.id, "evidence_deleter");
    let response = app
        .delete_with_token(
            &format!("/v1/matches/{match_id}/evidence/{evidence_id}"),
            &outsider_token,
        )
        .await;
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::UNAUTHORIZED,
        "Outsider delete should be rejected, got {}",
        response.status
    );

    // The uploader can delete their own evidence.
    let response = app
        .delete_with_token(
            &format!("/v1/matches/{match_id}/evidence/{evidence_id}"),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

/// A (non-participant) tournament admin may attach evidence on behalf of a
/// match; the evidence is then attributed to no registration.
#[tokio::test]
async fn test_evidence_add_link_as_admin_nonparticipant() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "evidence-admin")
            .await;

    let admin = UserBuilder::new()
        .username("evidence_admin")
        .build_persisted(app.pool())
        .await;
    assign_role_to_user(app.pool(), admin.id, "platform_admin").await;
    let admin_token = evidence_outsider_token(admin.id, "evidence_admin");

    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/link"),
            &json!({
                "evidence_type": "video",
                "url": "https://example.com/admin-vod",
                "name": "Admin-attached VOD"
            }),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert!(
        body["data"]["uploaded_by_registration_id"].is_null(),
        "Admin-attached evidence should not be attributed to a registration"
    );
}

/// Presigned evidence downloads are uploader/participant/admin-only.
#[tokio::test]
async fn test_access_url_denied_for_non_participants() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _, _) =
        crate::tournaments::create_tournament_with_matches(&app, "evidence-access-authz").await;

    // Dev user (a participant's registrant) attaches link evidence.
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/evidence/link"),
            &json!({
                "name": "Access Test VOD",
                "url": "https://youtube.com/watch?v=access-authz",
                "evidence_type": "video"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["id"].as_str().unwrap().to_string();

    // The uploader can mint an access URL.
    let response = app
        .get_auth(&format!(
            "/v1/matches/{match_id}/evidence/{evidence_id}/access"
        ))
        .await;
    response.assert_status(StatusCode::OK);

    // An unrelated authenticated user cannot.
    let outsider = UserBuilder::new()
        .username("evidence_access_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = create_test_token(
        outsider.id,
        outsider.id,
        "evidence_access_outsider",
        TEST_JWT_SECRET,
    );
    let response = app
        .get_with_token(
            &format!("/v1/matches/{match_id}/evidence/{evidence_id}/access"),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// VALIDATION / COMPLETE-UPLOAD PARTICIPANT BINDING
// ============================================================================

/// `validate_evidence` persists its outcome (`evidence_repo.mark_validated`),
/// so it must be participant-or-admin bound like every other evidence
/// mutation — previously any authenticated user could stamp attacker-chosen
/// validation results onto any match.
#[tokio::test]
async fn test_validate_evidence_requires_participant() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "evidence-val-authz")
            .await;

    // A participant attaches link evidence to validate against.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/link"),
            &json!({
                "evidence_type": "video",
                "url": "https://example.com/validate-vod",
                "name": "Validate VOD"
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["id"].as_str().unwrap().to_string();

    let payload = json!({
        "evidence_ids": [evidence_id],
        "expected_participant1_score": 16,
        "expected_participant2_score": 0
    });

    // Outsider is rejected before any persistence happens.
    let outsider = UserBuilder::new()
        .username("evidence_validator_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = evidence_outsider_token(outsider.id, "evidence_validator_outsider");
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/validate"),
            &payload,
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // A participant passes the gate. The plugin outcome itself is not
    // asserted here (it depends on the CS2 validator and the demo service);
    // what matters is that authorization no longer blocks them.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/validate"),
            &payload,
            &player2_token,
        )
        .await;
    assert!(
        response.status != StatusCode::FORBIDDEN && response.status != StatusCode::UNAUTHORIZED,
        "Participant validate_evidence should pass the authz gate, got {}: {}",
        response.status,
        response.text()
    );
}

/// `validate_demo` reaches the CS2 plugin with caller-chosen scores; it is
/// participant-or-admin bound too.
#[tokio::test]
async fn test_validate_demo_requires_participant() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(
            &app,
            "evidence-demo-authz",
        )
        .await;

    let payload = json!({
        "demo_name": "some_match.dem",
        "map_id": "de_dust2",
        "participant1_score": 16,
        "participant2_score": 0
    });

    let outsider = UserBuilder::new()
        .username("evidence_demo_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = evidence_outsider_token(outsider.id, "evidence_demo_outsider");
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/validate-demo"),
            &payload,
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // A participant gets past authz (the demo itself does not exist, so the
    // plugin fails downstream — that is a 400, not a 403).
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/validate-demo"),
            &payload,
            &player2_token,
        )
        .await;
    assert!(
        response.status != StatusCode::FORBIDDEN && response.status != StatusCode::UNAUTHORIZED,
        "Participant validate_demo should pass the authz gate, got {}: {}",
        response.status,
        response.text()
    );
}

/// P-137. With no demo service configured, the demo-service routes must refuse
/// and say why — not resolve to a hardcoded live third party.
///
/// `Cs2DemoClient::default()` returned a client pointed at
/// `https://demos.cs210mans.uk`, and `create_cs2_plugin` fell back to it
/// whenever `CS2_DEMO_SERVICE_URL` was unset — which it is in every `TestApp`
/// and in the e2e stack. `test_validate_demo_requires_participant` directly
/// above therefore issued a live outbound request to that host on every run,
/// and its comment ("the plugin fails downstream") described a network 404
/// from a stranger's server as expected behaviour. Misconfiguration now names
/// itself instead.
#[tokio::test]
async fn test_demo_service_routes_refuse_when_unconfigured() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "evidence-p137")
            .await;

    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/validate-demo"),
            &json!({
                "demo_name": "some_match.dem",
                "map_id": "de_dust2",
                "participant1_score": 16,
                "participant2_score": 0
            }),
            &player2_token,
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    let text = response.text();
    assert!(
        text.contains("CS2_DEMO_SERVICE_URL"),
        "the refusal must name the missing setting rather than report a \
         network failure from whatever host was guessed: {text}"
    );

    // Same for the stats proxy, the other route that leaves the process.
    let response = app
        .get_with_token(
            &format!("/v1/matches/{match_id}/evidence/demo-stats/some_match.dem"),
            &player2_token,
        )
        .await;
    assert!(
        response.text().contains("CS2_DEMO_SERVICE_URL"),
        "the stats proxy must refuse the same way: {} {}",
        response.status,
        response.text()
    );
}

/// `complete_upload` flips evidence Pending → Active, so it is bound to the
/// user who initiated the upload — not merely to any authenticated caller.
#[tokio::test]
async fn test_complete_upload_bound_to_uploader() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(
            &app,
            "evidence-complete-authz",
        )
        .await;

    // Player 2 (a participant) initiates the upload.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/evidence/upload"),
            &json!({
                "evidence_type": "demo",
                "file_name": "player2.dem",
                "file_size_bytes": 1024,
                "mime_type": "application/octet-stream"
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["evidence_id"].as_str().unwrap().to_string();

    // An unrelated authenticated user cannot complete it.
    let outsider = UserBuilder::new()
        .username("evidence_complete_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = evidence_outsider_token(outsider.id, "evidence_complete_outsider");
    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/evidence/{evidence_id}/complete"),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // The original uploader passes the gate (the file was never actually
    // PUT to storage, so the completion itself fails downstream — 400, not 403).
    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/evidence/{evidence_id}/complete"),
            &player2_token,
        )
        .await;
    assert!(
        response.status != StatusCode::FORBIDDEN && response.status != StatusCode::UNAUTHORIZED,
        "Uploader complete_upload should pass the authz gate, got {}: {}",
        response.status,
        response.text()
    );
}

// ============================================================================
// DEMO EVIDENCE: LINK VISIBILITY (P-109), CATALOG RESOLUTION (P-110),
// VALIDATION (P-111)
// ============================================================================

/// A catalogued, `ready` demo with parsed metadata — the row the demo browser
/// offers and the row `validate_evidence` compares against.
///
/// `raw_stats` deliberately carries no `player_summaries`: `try_auto_link`
/// (`services/demo.rs`) reads its keys as the demo's Steam-ID set, and with it
/// present stats ingestion would auto-link the demo, which is a different
/// code path from the one under test.
async fn seed_catalog_demo(
    app: &TestApp,
    file_name: &str,
    map_name: &str,
    team1_score: i32,
    team2_score: i32,
) -> String {
    // The catalog is an admin surface; the dev user is not one by default.
    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;

    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/admin/demos",
            &json!({
                "game_id": game_id.to_string(),
                "file_name": file_name,
                "s3_bucket": "portal-demos-test",
                "s3_key": format!("demos/{file_name}"),
                "file_size_bytes": 1_234_567,
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["id"].as_str().unwrap().to_string();

    let response = app
        .post_json(
            &format!("/v1/admin/demos/{demo_id}/stats"),
            &json!({
                "map_name": map_name,
                "team1_name": "Alpha",
                "team2_name": "Bravo",
                "team1_score": team1_score,
                "team2_score": team2_score,
                "total_rounds": team1_score + team2_score,
                "duration_seconds": 2100,
                "raw_stats": { "source": "evidence-integration-test" },
                "players": [
                    {
                        "steam_id": "76561198000000901",
                        "player_name": "alpha_one",
                        "team_name": "Alpha",
                        "stats": { "kills": 20, "deaths": 15, "assists": 4 }
                    },
                    {
                        "steam_id": "76561198000000902",
                        "player_name": "bravo_one",
                        "team_name": "Bravo",
                        "stats": { "kills": 14, "deaths": 19, "assists": 6 }
                    }
                ]
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "ready", "stats submission: {body}");

    demo_id
}

/// The default evidence listing — no `include_discovered`, exactly what
/// `stores/evidence.ts fetchEvidence` sends.
async fn list_evidence_default(app: &TestApp, match_id: &str) -> Vec<serde_json::Value> {
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/evidence"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    body["data"].as_array().cloned().unwrap_or_default()
}

async fn list_linked_demos(app: &TestApp, match_id: &str) -> Vec<serde_json::Value> {
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/demos?include_stats=true"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    body["data"].as_array().cloned().unwrap_or_default()
}

/// P-109. Linking a catalog demo wrote a row stamped `plugin_discovery`, and
/// `list_evidence` drops those from its default listing — so the evidence a
/// human deliberately attached was invisible on every evidence surface,
/// including the admin tab used to resolve the dispute it is evidence for.
#[tokio::test]
async fn test_linked_demo_evidence_appears_in_the_default_listing() {
    let app = TestApp::new().await;
    let info = create_cs2_tournament_with_match(&app, "ev-p109-vis").await;
    let demo_id = seed_catalog_demo(&app, "p109-visible.dem", "de_nuke", 13, 7).await;

    // Nothing attached yet: the listing is the honest empty state, not a
    // filter that happens to hide everything.
    assert!(list_evidence_default(&app, &info.match_id).await.is_empty());

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-discovered", info.match_id),
            &json!({ "external_id": format!("catalog:{demo_id}"), "game_number": 1 }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let evidence = list_evidence_default(&app, &info.match_id).await;
    assert_eq!(
        evidence.len(),
        1,
        "a human-linked demo must be listed by default: {evidence:?}"
    );
    assert_eq!(evidence[0]["evidence_type"], "demo");
    assert_eq!(evidence[0]["name"], "p109-visible.dem");

    // And asking for discovered rows explicitly does not double-count it or
    // reveal anything the default listing hid.
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence?include_discovered=true",
            info.match_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
}

/// P-110. `link_demo` resolved the demo by **file name against the external
/// stats service** and 404'd when it was absent — while holding `demo_id`,
/// which it then used anyway to create the link. The list the button acts on
/// is the catalog, so any catalogued demo without a `.stats.json` was offered
/// and refused. The unreachable stats URL here is the point: the catalog has
/// everything this handler needs.
#[tokio::test]
async fn test_link_demo_by_catalog_id_does_not_need_the_stats_service() {
    // Nothing listens on port 1 — every stats fetch fails, immediately.
    let app = TestApp::new_with_demo_service("http://127.0.0.1:1").await;
    let info = create_cs2_tournament_with_match(&app, "ev-p110-cat").await;
    let demo_id = seed_catalog_demo(&app, "p110-catalog.dem", "de_mirage", 16, 14).await;

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-demo", info.match_id),
            &json!({
                "demo_name": "p110-catalog.dem",
                "demo_id": demo_id,
                "game_number": 1
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // The demo_match_link the browser's Linked Demos list reads...
    let linked = list_linked_demos(&app, &info.match_id).await;
    assert_eq!(linked.len(), 1, "expected one demo link: {linked:?}");
    assert_eq!(linked[0]["link"]["demo_id"], demo_id.as_str());
    assert_eq!(linked[0]["link"]["game_number"], 1);
    assert_eq!(linked[0]["link"]["link_type"], "evidence");

    // ...and the evidence row, visible by default (P-109 on this path too),
    // named after the catalog row rather than after a stats-service lookup.
    let evidence = list_evidence_default(&app, &info.match_id).await;
    assert_eq!(evidence.len(), 1, "expected one evidence row: {evidence:?}");
    assert_eq!(evidence[0]["name"], "p110-catalog.dem");
}

/// The stats-service route is still the route for a caller that has only a
/// file name. P-110 narrowed when it is used; it did not delete it.
#[tokio::test]
async fn test_link_demo_without_catalog_id_still_resolves_via_the_stats_service() {
    let app = TestApp::new_with_demo_service("http://127.0.0.1:1").await;
    let info = create_cs2_tournament_with_match(&app, "ev-p110-noid").await;

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-demo", info.match_id),
            &json!({ "demo_name": "never-catalogued.dem", "game_number": 1 }),
        )
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
    assert!(list_linked_demos(&app, &info.match_id).await.is_empty());
}

/// P-111. `DemoMatchLinkRepository::mark_validated` had no caller anywhere in
/// the workspace, so `demo_match_links.validated` was `false` for every row
/// that has ever existed and the "Validated" chip was dead template. This
/// drives the endpoint the frontend now calls and asserts BOTH rows the
/// verdict has to reach.
#[tokio::test]
async fn test_validate_evidence_marks_the_demo_link_validated() {
    let app = TestApp::new_with_demo_service("http://127.0.0.1:1").await;
    let info = create_cs2_tournament_with_match(&app, "ev-p111-ok").await;
    let demo_id = seed_catalog_demo(&app, "p111-ok.dem", "de_ancient", 13, 7).await;

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-demo", info.match_id),
            &json!({ "demo_name": "p111-ok.dem", "demo_id": demo_id, "game_number": 1 }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["id"].as_str().unwrap().to_string();

    // Precondition, stated rather than assumed: nothing is validated yet.
    let linked = list_linked_demos(&app, &info.match_id).await;
    assert_eq!(linked[0]["link"]["validated"], false);
    assert_eq!(
        list_evidence_default(&app, &info.match_id).await[0]["validated"],
        false
    );

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/validate", info.match_id),
            &json!({
                "evidence_ids": [evidence_id],
                "expected_participant1_score": 13,
                "expected_participant2_score": 7
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["is_valid"], true,
        "a claim equal to the demo's recorded score must validate: {body}"
    );
    assert_eq!(body["data"]["extracted_result"]["map_id"], "de_ancient");
    assert_eq!(body["data"]["errors"].as_array().unwrap().len(), 0);

    // The link row — what every "Validated" chip in the frontend reads.
    let linked = list_linked_demos(&app, &info.match_id).await;
    assert_eq!(
        linked[0]["link"]["validated"], true,
        "demo_match_links.validated must be written: {linked:?}"
    );
    assert!(!linked[0]["link"]["validated_at"].is_null());

    // ...and the evidence row, which the same verdict also stamps.
    let evidence = list_evidence_default(&app, &info.match_id).await;
    assert_eq!(evidence[0]["validated"], true);
}

/// A failing verdict must be recorded as a failure. `mark_validated` used to
/// set `validated = true` unconditionally, which would have lit a green
/// "Validated" chip on a demo that contradicts the claim — worse than the
/// dead chip it replaced.
#[tokio::test]
async fn test_validate_evidence_records_a_contradiction_without_claiming_validation() {
    let app = TestApp::new_with_demo_service("http://127.0.0.1:1").await;
    let info = create_cs2_tournament_with_match(&app, "ev-p111-bad").await;
    let demo_id = seed_catalog_demo(&app, "p111-bad.dem", "de_overpass", 13, 7).await;

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-demo", info.match_id),
            &json!({ "demo_name": "p111-bad.dem", "demo_id": demo_id, "game_number": 1 }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["id"].as_str().unwrap().to_string();

    // Claim a scoreline the demo does not support, in either orientation.
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/validate", info.match_id),
            &json!({
                "evidence_ids": [evidence_id],
                "expected_participant1_score": 16,
                "expected_participant2_score": 14
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["is_valid"], false,
        "13-7 must not validate a 16-14 claim: {body}"
    );
    let errors = body["data"]["errors"].as_array().unwrap();
    assert_eq!(
        errors.len(),
        1,
        "the contradiction must be reported: {body}"
    );
    assert!(
        errors[0].as_str().unwrap().contains("13 - 7"),
        "the error should name what the demo actually records: {body}"
    );

    // The verdict is stored, but it does not claim the demo corroborates.
    let linked = list_linked_demos(&app, &info.match_id).await;
    assert_eq!(
        linked[0]["link"]["validated"], false,
        "a contradicted demo must not read as validated: {linked:?}"
    );
    assert!(
        !linked[0]["link"]["validation_result"].is_null(),
        "the failing verdict must still be recorded for the operator: {linked:?}"
    );

    // P-138: and the EVIDENCE row, which P-111 left behind. `mark_validated`
    // on the evidence repository wrote `validated = true` unconditionally, so
    // this row claimed the demo corroborated the result while the link row two
    // assertions up said it contradicted it — on the very surface an admin uses
    // to resolve the dispute.
    let evidence = list_evidence_default(&app, &info.match_id).await;
    assert_eq!(
        evidence[0]["validated"], false,
        "a contradicted demo's evidence row must not read as validated: {evidence:?}"
    );
    assert!(
        !evidence[0]["validated_at"].is_null(),
        "a failed validation must stay distinguishable from one that never ran: {evidence:?}"
    );
    let evidence_errors = evidence[0]["validation_errors"].as_array().unwrap();
    assert_eq!(
        evidence_errors.len(),
        1,
        "the reason must reach the evidence listing, not only the POST response: {evidence:?}"
    );
    assert!(
        evidence_errors[0].as_str().unwrap().contains("13 - 7"),
        "the evidence row should name what the demo actually records: {evidence:?}"
    );
}

/// P-138's other half: an evidence row that has never been validated must be
/// distinguishable from one whose validation FAILED. `validated: false` alone
/// cannot tell them apart, and the operator has to.
#[tokio::test]
async fn test_unvalidated_evidence_is_distinguishable_from_a_failed_validation() {
    let app = TestApp::new_with_demo_service("http://127.0.0.1:1").await;
    let info = create_cs2_tournament_with_match(&app, "ev-p138-never").await;
    let demo_id = seed_catalog_demo(&app, "p138-never.dem", "de_nuke", 13, 4).await;

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-demo", info.match_id),
            &json!({ "demo_name": "p138-never.dem", "demo_id": demo_id, "game_number": 1 }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let evidence = list_evidence_default(&app, &info.match_id).await;
    assert_eq!(evidence[0]["validated"], false);
    assert!(
        evidence[0]["validated_at"].is_null(),
        "nothing has validated this: {evidence:?}"
    );
    assert_eq!(
        evidence[0]["validation_errors"].as_array().unwrap().len(),
        0,
        "no validation ran, so there is nothing to report: {evidence:?}"
    );
}

/// P-135: the evidence row behind a demo link is named in the link listing.
///
/// Unlinking a demo is `DELETE .../evidence/{evidence_id}`. The pairing used to
/// exist only in the frontend's memory from the link call, so after a reload
/// the unlink had no id, sent nothing, and still reported success. The server
/// has always known the pairing; it just never said it.
#[tokio::test]
async fn test_linked_demo_names_its_evidence_row() {
    let app = TestApp::new_with_demo_service("http://127.0.0.1:1").await;
    let info = create_cs2_tournament_with_match(&app, "ev-p135-evid").await;
    let demo_id = seed_catalog_demo(&app, "p135-link.dem", "de_mirage", 13, 9).await;

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-demo", info.match_id),
            &json!({ "demo_name": "p135-link.dem", "demo_id": demo_id, "game_number": 1 }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["id"].as_str().unwrap().to_string();

    // A *fresh* read — carrying no memory of the link call — resolves it.
    let linked = list_linked_demos(&app, &info.match_id).await;
    assert_eq!(
        linked[0]["evidence_id"].as_str(),
        Some(evidence_id.as_str()),
        "the link listing must name the evidence row that backs it: {linked:?}"
    );

    // ...and that id is the one the DELETE route takes, which removes both rows.
    let response = app
        .delete_auth(&format!(
            "/v1/matches/{}/evidence/{evidence_id}",
            info.match_id
        ))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
    assert!(list_linked_demos(&app, &info.match_id).await.is_empty());
    assert!(list_evidence_default(&app, &info.match_id).await.is_empty());
}

/// Omitted scores are refused, not defaulted.
///
/// They used to default to 0, so an omitted score validated the demo against
/// 0-0 — a scoreline no game can have, which the plugin's own sanity check
/// then reported as "invalid". They cannot be defaulted from the match row
/// either: that carries the *series* score (maps won), while a demo records
/// one map's rounds.
#[tokio::test]
async fn test_validate_evidence_requires_the_claimed_game_scores() {
    let app = TestApp::new_with_demo_service("http://127.0.0.1:1").await;
    let info = create_cs2_tournament_with_match(&app, "ev-p111-req").await;
    let demo_id = seed_catalog_demo(&app, "p111-required.dem", "de_train", 13, 11).await;

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link-demo", info.match_id),
            &json!({ "demo_name": "p111-required.dem", "demo_id": demo_id, "game_number": 1 }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["id"].as_str().unwrap().to_string();

    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/validate", info.match_id),
            &json!({ "evidence_ids": [evidence_id] }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        response.text().contains("expected_participant1_score"),
        "the refusal should name what is missing: {}",
        response.text()
    );

    // Nothing was recorded off a request that could not be answered.
    let linked = list_linked_demos(&app, &info.match_id).await;
    assert_eq!(linked[0]["link"]["validated"], false);
    assert!(linked[0]["link"]["validation_result"].is_null());
}
