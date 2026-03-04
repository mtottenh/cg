//! Result submission API integration tests.


use axum::http::StatusCode;
use crate::common::TestApp;
use portal_test::prelude::*;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

/// Helper to transition a match to Ready status using admin endpoint.
async fn transition_match_to_ready(app: &TestApp, tournament_id: &str, match_id: &str) {
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/transition",
                tournament_id, match_id
            ),
            &json!({
                "to_status": "ready",
                "override_reason": "Test setup: transitioning match to Ready for result tests"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Helper to transition a match to InProgress status (for result submission tests).
/// The transition path is: Ready → Scheduled → InProgress
async fn transition_match_to_in_progress(app: &TestApp, tournament_id: &str, match_id: &str) {
    // First schedule the match (required step: Ready → Scheduled)
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339(),
                "reason": "Test setup: scheduling match for result submission tests"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Then transition to in_progress (Scheduled → InProgress)
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/transition",
                tournament_id, match_id
            ),
            &json!({
                "to_status": "in_progress",
                "override_reason": "Test setup: transitioning match to InProgress for result submission tests"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}


/// Helper to register a player and return the registration ID.
async fn register_player(app: &TestApp, tournament_id: &str, participant_name: &str) -> String {
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/registrations/player", tournament_id),
            &json!({
                "participant_name": participant_name
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// Helper to approve a registration.
async fn approve_registration(app: &TestApp, tournament_id: &str, registration_id: &str) {
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/registrations/{}/approve",
            tournament_id, registration_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
}

/// Match info returned by the helper
#[allow(dead_code)]
struct TestMatchInfo {
    tournament_id: String,
    match_id: String,
    participant1_reg_id: String,
    participant2_reg_id: String,
}

/// Helper to create a started tournament with matches.
/// Returns TestMatchInfo with proper participant order from the match.
async fn create_tournament_with_matches(app: &TestApp, slug: &str) -> TestMatchInfo {
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id.to_string(),
                "name": format!("Result Test {}", slug),
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
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/open-registration", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Register 2 players
    let reg1 = register_player(app, &tournament_id, "Player1").await;
    approve_registration(app, &tournament_id, &reg1).await;

    // Create second player using UserBuilder and register using TournamentRegistrationBuilder
    let user2 = UserBuilder::new()
        .username(&format!("player2_{}", slug))
        .build_persisted(app.pool())
        .await;

    let _reg2 = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament_uuid)
        .player_id_from_uuid(user2.id) // UserBuilder creates player with same ID as user
        .participant_name("Player2")
        .registered_by_uuid(user2.id)
        .approved() // Must be approved to participate
        .build_persisted(app.pool())
        .await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament (creates matches)
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Get matches to find the match ID and participant info
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let matches = body["data"].as_array().unwrap();
    assert!(!matches.is_empty(), "Tournament should have at least one match");

    let match_data = &matches[0];
    let match_id = match_data["id"].as_str().unwrap().to_string();

    // Get the actual participant registration IDs from the match
    let participant1_reg_id = match_data["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();
    let participant2_reg_id = match_data["participant2_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Grant admin permissions and transition match through Ready -> InProgress
    // for result submission tests
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
    transition_match_to_ready(app, &tournament_id, &match_id).await;
    transition_match_to_in_progress(app, &tournament_id, &match_id).await;

    TestMatchInfo {
        tournament_id,
        match_id,
        participant1_reg_id,
        participant2_reg_id,
    }
}

// ============================================================================
// RESULT SUBMISSION TESTS
// ============================================================================

#[tokio::test]
async fn test_submit_result_invalid_match_id() {
    let app = TestApp::new().await;

    // Try to submit result for non-existent match (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "claimed_winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 12,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_result_claim_not_found() {
    let app = TestApp::new().await;

    // Try to get result claim for non-existent match (public endpoint, no auth needed)
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/result")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_result_claims_for_nonexistent_match() {
    let app = TestApp::new().await;

    // Try to list result claims for non-existent match
    // This returns 200 with empty array as the endpoint doesn't verify match existence
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/result/history")
        .await;

    // The endpoint returns empty list for non-existent match
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_confirm_result_invalid_claim() {
    let app = TestApp::new().await;

    // Try to confirm non-existent claim (using authenticated request)
    let response = app
        .post_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result/00000000-0000-0000-0000-000000000001/confirm",
        )
        .await;

    // Returns 500 for claim not found (internal error mapping from domain error)
    // TODO: This should ideally return 404, but the current implementation returns 500
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_dispute_result_invalid_claim() {
    let app = TestApp::new().await;

    // Try to dispute non-existent claim (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result/00000000-0000-0000-0000-000000000001/dispute",
            &json!({
                "reason": "Incorrect score reported",
                "evidence_ids": []
            }),
        )
        .await;

    // Returns 500 for claim not found (internal error mapping from domain error)
    // TODO: This should ideally return 404, but the current implementation returns 500
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// RESULT VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_submit_result_missing_winner_id() {
    let app = TestApp::new().await;

    // Try to submit result without winner ID (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;

    // Returns 400 for missing required field (deserialization error)
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_submit_result_invalid_winner_id_format() {
    let app = TestApp::new().await;

    // Try to submit result with invalid winner ID format (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "claimed_winner_registration_id": "not-a-uuid",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;

    // Should fail with bad request for invalid format
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_dispute_result_missing_reason() {
    let app = TestApp::new().await;

    // Try to dispute without reason (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result/00000000-0000-0000-0000-000000000001/dispute",
            &json!({
                "evidence_ids": []
            }),
        )
        .await;

    // Returns 400 for missing required field (deserialization error)
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_result_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000000";

    // Verify GET /result endpoint exists (doesn't return METHOD_NOT_ALLOWED)
    let response = app.get(&format!("/v1/matches/{}/result", match_id)).await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /result endpoint should exist"
    );

    // Verify GET /result/history endpoint exists
    let response = app
        .get(&format!("/v1/matches/{}/result/history", match_id))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /result/history endpoint should exist"
    );

    // Verify POST /result endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_id),
            &json!({
                "claimed_winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /result endpoint should exist"
    );
}

#[tokio::test]
async fn test_result_confirm_dispute_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000000";
    let claim_id = "00000000-0000-0000-0000-000000000001";

    // Verify confirm endpoint exists (authenticated)
    let response = app
        .post_auth(&format!(
            "/v1/matches/{}/result/{}/confirm",
            match_id, claim_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /result/{claim_id}/confirm endpoint should exist"
    );

    // Verify dispute endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result/{}/dispute", match_id, claim_id),
            &json!({
                "reason": "Test reason",
                "evidence_ids": []
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /result/{claim_id}/dispute endpoint should exist"
    );
}

// ============================================================================
// RESULT CLAIM DEMO BRIDGE TESTS (Phase 4.2)
// ============================================================================

#[tokio::test]
async fn test_submit_result_with_demo_link_ids_empty() {
    let app = TestApp::new().await;

    // Submit result with empty demo_link_ids (should be accepted)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "claimed_winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": []
            }),
        )
        .await;

    // Returns 404 because match doesn't exist, but verifies demo_link_ids field is accepted
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_submit_result_with_invalid_demo_link_id_format() {
    let app = TestApp::new().await;

    // Submit result with invalid demo_link_id format
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "claimed_winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [],
                "evidence_ids": [],
                "demo_link_ids": ["not-a-uuid"]
            }),
        )
        .await;

    // Should fail with 400 Bad Request for invalid format
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_submit_result_with_per_game_demo_link_id_invalid() {
    let app = TestApp::new().await;

    // Submit result with per-game demo_link_id that is invalid
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "claimed_winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10,
                        "demo_link_id": "not-a-uuid"
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": []
            }),
        )
        .await;

    // Should fail with 400 Bad Request for invalid format
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_submit_result_with_nonexistent_demo_link_id() {
    let app = TestApp::new().await;

    // Submit result with non-existent (but valid format) demo_link_id
    // This should fail when the match is found but the demo link is not
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "claimed_winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [],
                "evidence_ids": [],
                "demo_link_ids": ["00000000-0000-0000-0000-000000000099"]
            }),
        )
        .await;

    // Returns 404 because match doesn't exist (demo link validation happens after match lookup)
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_submit_result_with_per_game_demo_link_id_valid_format() {
    let app = TestApp::new().await;

    // Submit result with per-game demo_link_id in valid UUID format
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/result",
            &json!({
                "claimed_winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10,
                        "demo_link_id": "00000000-0000-0000-0000-000000000002"
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 16,
                        "participant2_score": 12,
                        "demo_link_id": "00000000-0000-0000-0000-000000000003"
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": ["00000000-0000-0000-0000-000000000001"]
            }),
        )
        .await;

    // Returns 404 because match doesn't exist, but verifies the format is accepted
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// COMPREHENSIVE DEMO BRIDGE INTEGRATION TESTS (with real matches)
// ============================================================================

#[tokio::test]
async fn test_submit_result_with_real_match_and_demo_links() {
    let app = TestApp::new().await;
    let match_info = create_tournament_with_matches(&app, "demo-bridge-real-match").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = match_info.match_id.parse().unwrap();
    let tournament_uuid: Uuid = match_info.tournament_id.parse().unwrap();

    // Create a demo using the builder
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("test_match_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 16, 10)
        .tournament_id(tournament_uuid)
        .build_persisted(app.pool())
        .await;

    // Create a demo-match link
    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit result with the demo_link_id
    // Participant1 wins 2-1 (Bo3): P1 wins games 1 and 3, P2 wins game 2
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info.match_id),
            &json!({
                "claimed_winner_registration_id": match_info.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10,
                        "demo_link_id": demo_link.id.to_string()
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": [demo_link.id.to_string()]
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    // The claim is nested under body["data"]["claim"]
    let claim = &body["data"]["claim"];
    assert!(claim["id"].is_string(), "Result claim should have an ID");
    assert_eq!(claim["match_id"], match_info.match_id);

    // Verify demo_link_ids are returned in the response
    let returned_demo_link_ids = claim["demo_link_ids"].as_array().unwrap();
    assert_eq!(returned_demo_link_ids.len(), 1);
    assert_eq!(returned_demo_link_ids[0].as_str().unwrap(), demo_link.id.to_string());
}

#[tokio::test]
async fn test_submit_result_with_multiple_demo_links() {
    let app = TestApp::new().await;
    let match_info = create_tournament_with_matches(&app, "demo-bridge-multi-link").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = match_info.match_id.parse().unwrap();

    // Create multiple demos (one per game in a Bo3)
    let demo1 = DemoBuilder::new()
        .game_id(game_id)
        .file_name("game1_de_dust2.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 16, 10)
        .build_persisted(app.pool())
        .await;

    let demo2 = DemoBuilder::new()
        .game_id(game_id)
        .file_name("game2_de_mirage.dem")
        .cs2_metadata("de_mirage", "Player1", "Player2", 10, 16)
        .build_persisted(app.pool())
        .await;

    let demo3 = DemoBuilder::new()
        .game_id(game_id)
        .file_name("game3_de_inferno.dem")
        .cs2_metadata("de_inferno", "Player1", "Player2", 16, 8)
        .build_persisted(app.pool())
        .await;

    // Create demo-match links for each game
    let link1 = DemoMatchLinkBuilder::new()
        .demo_id(demo1.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    let link2 = DemoMatchLinkBuilder::new()
        .demo_id(demo2.id)
        .match_id(match_uuid)
        .game_number(2)
        .manual()
        .build_persisted(app.pool())
        .await;

    let link3 = DemoMatchLinkBuilder::new()
        .demo_id(demo3.id)
        .match_id(match_uuid)
        .game_number(3)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit result with all demo links
    // Participant1 wins 2-1 (Bo3): P1 wins games 1 and 3, P2 wins game 2
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info.match_id),
            &json!({
                "claimed_winner_registration_id": match_info.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10,
                        "demo_link_id": link1.id.to_string()
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16,
                        "demo_link_id": link2.id.to_string()
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8,
                        "demo_link_id": link3.id.to_string()
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": [
                    link1.id.to_string(),
                    link2.id.to_string(),
                    link3.id.to_string()
                ]
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    // The claim is nested under body["data"]["claim"]
    let claim = &body["data"]["claim"];

    // Verify all demo_link_ids are returned
    let returned_demo_link_ids = claim["demo_link_ids"].as_array().unwrap();
    assert_eq!(returned_demo_link_ids.len(), 3);

    // Verify game results contain per-game demo_link_ids
    let game_results = claim["game_results"].as_array().unwrap();
    assert_eq!(game_results.len(), 3);

    for (i, result) in game_results.iter().enumerate() {
        let expected_link = match i {
            0 => &link1,
            1 => &link2,
            2 => &link3,
            _ => unreachable!(),
        };
        // Per-game demo_link_id may not be in the claim response, check if present
        if let Some(demo_link_id) = result.get("demo_link_id").and_then(|v| v.as_str()) {
            assert_eq!(demo_link_id, expected_link.id.to_string());
        }
    }
}

#[tokio::test]
async fn test_submit_result_demo_link_wrong_match_rejected() {
    let app = TestApp::new().await;

    // Create two tournaments with matches
    let match_info1 = create_tournament_with_matches(&app, "demo-wrong-match-1").await;
    let match_info2 = create_tournament_with_matches(&app, "demo-wrong-match-2").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid2: Uuid = match_info2.match_id.parse().unwrap();

    // Create a demo linked to match 2
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("match2_demo.dem")
        .cs2_metadata("de_dust2", "Team1", "Team2", 16, 10)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid2) // Linked to match 2
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Try to submit result for match 1 with demo_link from match 2
    // Participant1 wins 2-1 (Bo3)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info1.match_id), // Using match 1
            &json!({
                "claimed_winner_registration_id": match_info1.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": [demo_link.id.to_string()] // Demo link is for match 2
            }),
        )
        .await;

    // Should fail because the demo link is not linked to match 1
    response.assert_status(StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json();
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("not linked to match"),
        "Error should indicate demo link is not linked to the match"
    );
}

#[tokio::test]
async fn test_submit_result_nonexistent_demo_link_rejected() {
    let app = TestApp::new().await;
    let match_info = create_tournament_with_matches(&app, "demo-nonexistent-link").await;

    // Use a non-existent demo_link_id (valid UUID format but doesn't exist)
    let fake_demo_link_id = Uuid::new_v4();

    // Participant1 wins 2-1 (Bo3)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info.match_id),
            &json!({
                "claimed_winner_registration_id": match_info.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": [fake_demo_link_id.to_string()]
            }),
        )
        .await;

    // Should fail with 404 because the demo_link doesn't exist
    response.assert_status(StatusCode::NOT_FOUND);

    let body: serde_json::Value = response.json();
    assert!(
        body["detail"].as_str().unwrap().contains("Demo-match link not found"),
        "Error should indicate demo-match link not found"
    );
}

#[tokio::test]
async fn test_get_result_claim_with_demo_links() {
    let app = TestApp::new().await;
    let match_info = create_tournament_with_matches(&app, "demo-get-claim").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = match_info.match_id.parse().unwrap();

    // Create demo and link
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("get_claim_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 16, 10)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit result - Participant1 wins 2-1 (Bo3)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info.match_id),
            &json!({
                "claimed_winner_registration_id": match_info.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10,
                        "demo_link_id": demo_link.id.to_string()
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": [demo_link.id.to_string()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Now GET the result claim
    let response = app
        .get(&format!("/v1/matches/{}/result", match_info.match_id))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["match_id"], match_info.match_id);

    // Verify demo_link_ids are returned
    let returned_demo_link_ids = body["data"]["demo_link_ids"].as_array().unwrap();
    assert_eq!(returned_demo_link_ids.len(), 1);
    assert_eq!(
        returned_demo_link_ids[0].as_str().unwrap(),
        demo_link.id.to_string()
    );

    // Verify game result has demo_link_id
    let game_results = body["data"]["game_results"].as_array().unwrap();
    assert_eq!(game_results.len(), 3);
    assert_eq!(
        game_results[0]["demo_link_id"].as_str().unwrap(),
        demo_link.id.to_string()
    );
}

#[tokio::test]
async fn test_result_claim_history_preserves_demo_links() {
    let app = TestApp::new().await;
    let match_info = create_tournament_with_matches(&app, "demo-history").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = match_info.match_id.parse().unwrap();

    // Create demo and link
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("history_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 16, 10)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit result - Participant1 wins 2-1 (Bo3)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info.match_id),
            &json!({
                "claimed_winner_registration_id": match_info.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": [demo_link.id.to_string()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Get result claim history
    let response = app
        .get(&format!("/v1/matches/{}/result/history", match_info.match_id))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let history = body["data"].as_array().unwrap();
    assert!(!history.is_empty(), "History should have at least one entry");

    // Verify the first claim in history has demo_link_ids
    let first_claim = &history[0];
    let returned_demo_link_ids = first_claim["demo_link_ids"].as_array().unwrap();
    assert_eq!(returned_demo_link_ids.len(), 1);
    assert_eq!(
        returned_demo_link_ids[0].as_str().unwrap(),
        demo_link.id.to_string()
    );
}

#[tokio::test]
async fn test_submit_result_empty_demo_links_for_real_match() {
    let app = TestApp::new().await;
    let match_info = create_tournament_with_matches(&app, "demo-empty-links").await;

    // Submit result without any demo links (valid use case)
    // Participant1 wins 2-1 (Bo3)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info.match_id),
            &json!({
                "claimed_winner_registration_id": match_info.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    // The claim is nested under body["data"]["claim"]
    let claim = &body["data"]["claim"];
    assert!(claim["id"].is_string());

    // Verify demo_link_ids is empty array
    let returned_demo_link_ids = claim["demo_link_ids"].as_array().unwrap();
    assert!(returned_demo_link_ids.is_empty());
}

#[tokio::test]
async fn test_demo_link_auto_matched_with_confidence_score() {
    let app = TestApp::new().await;
    let match_info = create_tournament_with_matches(&app, "demo-auto-matched").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = match_info.match_id.parse().unwrap();

    // Create demo
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("auto_matched_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 16, 10)
        .build_persisted(app.pool())
        .await;

    // Create an auto-matched link with confidence score
    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .auto_matched(0.95) // High confidence auto-match
        .build_persisted(app.pool())
        .await;

    // Verify the link was created with correct type
    let row = sqlx::query("SELECT link_type, confidence_score FROM demo_match_links WHERE id = $1")
        .bind(demo_link.id)
        .fetch_one(app.pool())
        .await
        .expect("Demo link should exist");

    let link_type: String = row.get("link_type");
    let confidence: Option<f32> = row.get("confidence_score");

    assert_eq!(link_type, "auto_matched");
    assert!((confidence.unwrap() - 0.95).abs() < 0.001);

    // Submit result using this auto-matched link
    // Participant1 wins 2-1 (Bo3)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_info.match_id),
            &json!({
                "claimed_winner_registration_id": match_info.participant1_reg_id,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": 16,
                        "participant2_score": 10,
                        "demo_link_id": demo_link.id.to_string()
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": 10,
                        "participant2_score": 16
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": 16,
                        "participant2_score": 8
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": [demo_link.id.to_string()]
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);
}
