//! Match completion saga end-to-end integration tests.
//!
//! Tests the full workflow: result confirmation → demo validation → review creation →
//! review resolution → bracket progression.


use axum::http::StatusCode;
use crate::common::TestApp;
use portal_test::prelude::*;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

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
                "override_reason": "Test setup"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Helper to transition a match from Ready → Scheduled → InProgress.
async fn transition_match_to_in_progress(app: &TestApp, tournament_id: &str, match_id: &str) {
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339(),
                "reason": "Test setup"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/transition",
                tournament_id, match_id
            ),
            &json!({
                "to_status": "in_progress",
                "override_reason": "Test setup"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Register a player and return the registration ID.
async fn register_player(app: &TestApp, tournament_id: &str, participant_name: &str) -> String {
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/registrations/player", tournament_id),
            &json!({ "participant_name": participant_name }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// Approve a registration.
async fn approve_registration(app: &TestApp, tournament_id: &str, registration_id: &str) {
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/registrations/{}/approve",
            tournament_id, registration_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
}

/// Info about a 4-player tournament (single elimination: 2 semis + 1 final).
struct FourPlayerTournament {
    tournament_id: String,
    /// The semifinal match where the dev user is a participant (transitioned to InProgress).
    test_match_id: String,
    /// The other semifinal match.
    #[allow(dead_code)]
    other_semi_match_id: String,
    /// Final match (winner of each semi).
    final_match_id: String,
    /// Dev user's registration ID in the test match.
    dev_reg_id: String,
    /// Opponent's registration ID in the test match.
    #[allow(dead_code)]
    opponent_reg_id: String,
    /// Opponent's user ID (for creating auth tokens to confirm results).
    opponent_user_id: Uuid,
    /// Whether dev user is participant1 (true) or participant2 (false) in the test match.
    dev_is_p1: bool,
}

/// Create a 4-player single-elimination tournament.
///
/// Returns info about the bracket: 2 semifinals + 1 final.
/// The semifinal containing the dev user is transitioned to InProgress, ready for result submission.
async fn create_4player_tournament(app: &TestApp, slug: &str) -> FourPlayerTournament {
    let game_id = get_game_id(app.pool(), "cs2").await;

    // Create tournament
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id.to_string(),
                "name": format!("Saga Test {}", slug),
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

    // Register 4 players:
    // Player 1 = dev user (registered via API, so registered_by = dev_user_id)
    let dev_reg_id = register_player(app, &tournament_id, "Player1").await;
    approve_registration(app, &tournament_id, &dev_reg_id).await;

    // Players 2-4 via builders (UserBuilder creates both user + player with same UUID)
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

    let user3 = UserBuilder::new()
        .username(&format!("player3_{}", slug))
        .build_persisted(app.pool())
        .await;
    let _reg3 = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament_uuid)
        .player_id_from_uuid(user3.id)
        .participant_name("Player3")
        .registered_by_uuid(user3.id)
        .approved()
        .build_persisted(app.pool())
        .await;

    let user4 = UserBuilder::new()
        .username(&format!("player4_{}", slug))
        .build_persisted(app.pool())
        .await;
    let _reg4 = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament_uuid)
        .player_id_from_uuid(user4.id)
        .participant_name("Player4")
        .registered_by_uuid(user4.id)
        .approved()
        .build_persisted(app.pool())
        .await;

    // Auto-seed
    app.post_json(
        &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Start tournament (creates bracket + matches)
    app.post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Get matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let matches = body["data"].as_array().unwrap();
    assert!(
        matches.len() >= 3,
        "4-player SE bracket should have at least 3 matches, got {}",
        matches.len()
    );

    // Identify semifinals (round 1) and final (round 2)
    let mut semis = Vec::new();
    let mut final_match = None;

    for m in matches {
        let round = m["round"].as_i64().unwrap();
        if round == 1 {
            semis.push(m.clone());
        } else if round == 2 {
            final_match = Some(m.clone());
        }
    }

    assert_eq!(semis.len(), 2, "Should have 2 semifinal matches");
    let final_match = final_match.expect("Should have a final match");
    let final_match_id = final_match["id"].as_str().unwrap().to_string();

    // Find which semifinal contains the dev user's registration.
    // Seeding is random, so we must check both.
    let mut test_match = None;
    let mut other_semi = None;

    for m in &semis {
        let p1 = m["participant1_registration_id"].as_str().unwrap_or("");
        let p2 = m["participant2_registration_id"].as_str().unwrap_or("");

        if p1 == dev_reg_id || p2 == dev_reg_id {
            test_match = Some(m.clone());
        } else {
            other_semi = Some(m.clone());
        }
    }

    let test_match = test_match.expect("Dev user should be in one of the semifinals");
    let other_semi = other_semi.expect("Other semifinal should exist");

    let test_match_id = test_match["id"].as_str().unwrap().to_string();
    let other_semi_match_id = other_semi["id"].as_str().unwrap().to_string();

    // Find the opponent's registration ID and dev user's position in the test match
    let p1 = test_match["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();
    let p2 = test_match["participant2_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    let dev_is_p1 = p1 == dev_reg_id;
    let opponent_reg_id = if dev_is_p1 {
        p2.clone()
    } else {
        p1.clone()
    };

    // Query the opponent's user_id from their registration (column is `registered_by`)
    let opponent_reg_uuid: Uuid = opponent_reg_id.parse().unwrap();
    let row = sqlx::query("SELECT registered_by FROM tournament_registrations WHERE id = $1")
        .bind(opponent_reg_uuid)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let opponent_user_id: Uuid = row.get("registered_by");

    // Grant admin permissions for match transitions
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;

    // Transition the test match to InProgress
    transition_match_to_ready(app, &tournament_id, &test_match_id).await;
    transition_match_to_in_progress(app, &tournament_id, &test_match_id).await;

    FourPlayerTournament {
        tournament_id,
        test_match_id,
        other_semi_match_id,
        final_match_id,
        dev_reg_id,
        opponent_reg_id,
        opponent_user_id,
        dev_is_p1,
    }
}

/// Submit a result claim (BO3, winner takes 2-1) and return the claim ID.
///
/// `winner_is_p1` determines score orientation:
/// - true: p1_score=2, p2_score=1, p1 wins games 1&3
/// - false: p1_score=1, p2_score=2, p2 wins games 1&3
async fn submit_claim(
    app: &TestApp,
    match_id: &str,
    winner_reg_id: &str,
    winner_is_p1: bool,
    demo_link_ids: &[String],
) -> String {
    let (p1_score, p2_score) = if winner_is_p1 { (2, 1) } else { (1, 2) };

    // Game results: winner takes games 1 & 3, loser takes game 2
    let (g1_p1, g1_p2) = if winner_is_p1 { (16, 10) } else { (10, 16) };
    let (g2_p1, g2_p2) = if winner_is_p1 { (12, 16) } else { (16, 12) };
    let (g3_p1, g3_p2) = if winner_is_p1 { (16, 8) } else { (8, 16) };

    let response = app
        .post_json(
            &format!("/v1/matches/{}/result", match_id),
            &json!({
                "claimed_winner_registration_id": winner_reg_id,
                "participant1_score": p1_score,
                "participant2_score": p2_score,
                "game_results": [
                    {
                        "game_number": 1,
                        "map_id": "de_dust2",
                        "participant1_score": g1_p1,
                        "participant2_score": g1_p2
                    },
                    {
                        "game_number": 2,
                        "map_id": "de_mirage",
                        "participant1_score": g2_p1,
                        "participant2_score": g2_p2
                    },
                    {
                        "game_number": 3,
                        "map_id": "de_inferno",
                        "participant1_score": g3_p1,
                        "participant2_score": g3_p2
                    }
                ],
                "evidence_ids": [],
                "demo_link_ids": demo_link_ids
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    body["data"]["claim"]["id"].as_str().unwrap().to_string()
}

/// Confirm a claim using a custom user token. Returns the response body.
async fn confirm_claim_as_user(
    app: &TestApp,
    match_id: &str,
    claim_id: &str,
    user_id: Uuid,
    player_id: Uuid,
) -> serde_json::Value {
    let token = create_test_token(user_id, player_id, "confirmer", TEST_JWT_SECRET);
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result/{}/confirm", match_id, claim_id),
            &json!({}),
            &token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    response.json()
}

// ============================================================================
// TEST: Confirm result without demos triggers bracket progression
// ============================================================================

#[tokio::test]
async fn test_confirm_result_triggers_progression() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-progression").await;

    // Submit result claim (dev user wins 2-1)
    let claim_id = submit_claim(
        &app,
        &t.test_match_id,
        &t.dev_reg_id,
        t.dev_is_p1,
        &[],
    )
    .await;

    // Confirm as opponent (different user)
    let body = confirm_claim_as_user(
        &app,
        &t.test_match_id,
        &claim_id,
        t.opponent_user_id,
        t.opponent_user_id, // player_id == user_id in test builders
    )
    .await;

    // Check response
    let data = &body["data"];
    assert_eq!(data["match_status"], "completed");
    assert_eq!(data["bracket_advanced"], true, "Bracket should have advanced");
    assert!(
        data["review_pending"].is_null(),
        "No review should be pending"
    );

    // Verify the winner was placed in the final match
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches",
            t.tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    let matches_body: serde_json::Value = response.json();
    let matches = matches_body["data"].as_array().unwrap();

    let final_match = matches
        .iter()
        .find(|m| m["id"].as_str().unwrap() == t.final_match_id)
        .expect("Final match should exist");

    // The winner should be placed in the final
    let final_p1 = final_match["participant1_registration_id"]
        .as_str()
        .unwrap_or("");
    let final_p2 = final_match["participant2_registration_id"]
        .as_str()
        .unwrap_or("");

    let winner_in_final =
        final_p1 == t.dev_reg_id || final_p2 == t.dev_reg_id;
    assert!(
        winner_in_final,
        "Winner ({}) should be placed in the final. Final has p1={}, p2={}",
        t.dev_reg_id, final_p1, final_p2
    );
}

// ============================================================================
// TEST: Confirm result with valid demos still progresses
// ============================================================================

#[tokio::test]
async fn test_confirm_result_with_valid_demos_progresses() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-valid-demo").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = t.test_match_id.parse().unwrap();
    let tournament_uuid: Uuid = t.tournament_id.parse().unwrap();

    // Create a demo with scores that MATCH the claim's game 1 scores.
    // When dev is p1, game1 = (16, 10); when dev is p2, game1 = (10, 16).
    let (demo_t1_score, demo_t2_score) = if t.dev_is_p1 { (16, 10) } else { (10, 16) };
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("valid_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", demo_t1_score, demo_t2_score)
        .tournament_id(tournament_uuid)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit result with matching demo
    let claim_id = submit_claim(
        &app,
        &t.test_match_id,
        &t.dev_reg_id,
        t.dev_is_p1,
        &[demo_link.id.to_string()],
    )
    .await;

    // Confirm as opponent
    let body = confirm_claim_as_user(
        &app,
        &t.test_match_id,
        &claim_id,
        t.opponent_user_id,
        t.opponent_user_id,
    )
    .await;

    // Demo scores match → no review → bracket advances
    let data = &body["data"];
    assert_eq!(data["bracket_advanced"], true, "Should advance with valid demo");
    assert!(
        data["review_pending"].is_null(),
        "No review needed for valid demo"
    );
}

// ============================================================================
// TEST: Confirm result with mismatched demo creates review, pauses progression
// ============================================================================

#[tokio::test]
async fn test_confirm_with_mismatched_demo_creates_review() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-mismatch").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = t.test_match_id.parse().unwrap();
    let tournament_uuid: Uuid = t.tournament_id.parse().unwrap();

    // Create a demo with scores that DON'T match the claim
    // Claim will say 16-10, but demo says 13-16 (different scores)
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("mismatched_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 13, 16)
        .tournament_id(tournament_uuid)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit result claiming 16-10 for game 1, but demo shows 13-16
    let claim_id = submit_claim(
        &app,
        &t.test_match_id,
        &t.dev_reg_id,
        t.dev_is_p1,
        &[demo_link.id.to_string()],
    )
    .await;

    // Confirm as opponent
    let body = confirm_claim_as_user(
        &app,
        &t.test_match_id,
        &claim_id,
        t.opponent_user_id,
        t.opponent_user_id,
    )
    .await;

    // Mismatched demo → review created → progression paused
    let data = &body["data"];
    assert_eq!(
        data["bracket_advanced"], false,
        "Should NOT advance with mismatched demo"
    );
    assert_eq!(
        data["review_pending"],
        Some(true).map(serde_json::Value::from).unwrap(),
        "Review should be pending"
    );

    // Verify a review exists for this match
    let review_response = app
        .get_auth(&format!(
            "/v1/matches/{}/result-review",
            t.test_match_id
        ))
        .await;
    review_response.assert_status(StatusCode::OK);

    let review_body: serde_json::Value = review_response.json();
    assert_eq!(
        review_body["data"]["status"], "pending_admin_review",
        "Review should be in pending_admin_review status (score mismatch)"
    );

    // Verify the winner was NOT placed in the final
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches",
            t.tournament_id
        ))
        .await;
    let matches_body: serde_json::Value = response.json();
    let matches = matches_body["data"].as_array().unwrap();

    let final_match = matches
        .iter()
        .find(|m| m["id"].as_str().unwrap() == t.final_match_id)
        .unwrap();

    let final_p1 = final_match["participant1_registration_id"]
        .as_str()
        .unwrap_or("");
    let final_p2 = final_match["participant2_registration_id"]
        .as_str()
        .unwrap_or("");

    let winner_in_final =
        final_p1 == t.dev_reg_id || final_p2 == t.dev_reg_id;
    assert!(
        !winner_in_final,
        "Winner should NOT be in the final yet (progression paused)"
    );
}

// ============================================================================
// TEST: Approve review resumes progression
// ============================================================================

#[tokio::test]
async fn test_approve_review_resumes_progression() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-approve").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = t.test_match_id.parse().unwrap();
    let tournament_uuid: Uuid = t.tournament_id.parse().unwrap();

    // Create mismatched demo to trigger review
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("approve_test_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 13, 16)
        .tournament_id(tournament_uuid)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit + confirm → creates review, pauses progression
    let claim_id = submit_claim(
        &app,
        &t.test_match_id,
        &t.dev_reg_id,
        t.dev_is_p1,
        &[demo_link.id.to_string()],
    )
    .await;

    confirm_claim_as_user(
        &app,
        &t.test_match_id,
        &claim_id,
        t.opponent_user_id,
        t.opponent_user_id,
    )
    .await;

    // Get the review ID
    let review_response = app
        .get_auth(&format!(
            "/v1/matches/{}/result-review",
            t.test_match_id
        ))
        .await;
    review_response.assert_status(StatusCode::OK);
    let review_body: serde_json::Value = review_response.json();
    let review_id = review_body["data"]["id"].as_str().unwrap();

    // Admin approves the review → should resume progression
    let approve_response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{}/approve", review_id),
            &json!({ "notes": "Scores verified manually, approving" }),
        )
        .await;
    approve_response.assert_status(StatusCode::OK);

    // Verify the winner is now placed in the final
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches",
            t.tournament_id
        ))
        .await;
    let matches_body: serde_json::Value = response.json();
    let matches = matches_body["data"].as_array().unwrap();

    let final_match = matches
        .iter()
        .find(|m| m["id"].as_str().unwrap() == t.final_match_id)
        .unwrap();

    let final_p1 = final_match["participant1_registration_id"]
        .as_str()
        .unwrap_or("");
    let final_p2 = final_match["participant2_registration_id"]
        .as_str()
        .unwrap_or("");

    let winner_in_final =
        final_p1 == t.dev_reg_id || final_p2 == t.dev_reg_id;
    assert!(
        winner_in_final,
        "After approval, winner ({}) should be placed in the final. Final has p1={}, p2={}",
        t.dev_reg_id, final_p1, final_p2
    );
}

// ============================================================================
// TEST: Reject review reverts match
// ============================================================================

#[tokio::test]
async fn test_reject_review_reverts_match() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-reject").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = t.test_match_id.parse().unwrap();
    let tournament_uuid: Uuid = t.tournament_id.parse().unwrap();

    // Create mismatched demo to trigger review
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("reject_test_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 13, 16)
        .tournament_id(tournament_uuid)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Submit + confirm → creates review
    let claim_id = submit_claim(
        &app,
        &t.test_match_id,
        &t.dev_reg_id,
        t.dev_is_p1,
        &[demo_link.id.to_string()],
    )
    .await;

    confirm_claim_as_user(
        &app,
        &t.test_match_id,
        &claim_id,
        t.opponent_user_id,
        t.opponent_user_id,
    )
    .await;

    // Get the review ID
    let review_response = app
        .get_auth(&format!(
            "/v1/matches/{}/result-review",
            t.test_match_id
        ))
        .await;
    review_response.assert_status(StatusCode::OK);
    let review_body: serde_json::Value = review_response.json();
    let review_id = review_body["data"]["id"].as_str().unwrap();

    // Admin rejects the review → match should revert to in_progress
    let reject_response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{}/reject", review_id),
            &json!({ "notes": "Demo evidence shows different result" }),
        )
        .await;
    reject_response.assert_status(StatusCode::OK);

    // Verify match status reverted to in_progress
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches",
            t.tournament_id
        ))
        .await;
    let matches_body: serde_json::Value = response.json();
    let matches = matches_body["data"].as_array().unwrap();

    let test_match = matches
        .iter()
        .find(|m| m["id"].as_str().unwrap() == t.test_match_id)
        .unwrap();

    assert_eq!(
        test_match["status"], "in_progress",
        "Match should be reverted to in_progress after review rejection"
    );
}

// ============================================================================
// TEST: Verify get_match_demos_with_data works for a real match
// ============================================================================

#[tokio::test]
async fn test_get_match_demos_with_data_returns_linked_demos() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-demos-check").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = t.test_match_id.parse().unwrap();
    let tournament_uuid: Uuid = t.tournament_id.parse().unwrap();

    // Create a demo and link it to the match
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("check_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 16, 10)
        .tournament_id(tournament_uuid)
        .build_persisted(app.pool())
        .await;

    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    // Call the API endpoint that uses get_match_demos_with_data
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/demos?include_stats=true",
            t.test_match_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();

    let demos = body["data"].as_array().expect("data should be an array");
    assert_eq!(
        demos.len(),
        1,
        "Should find 1 demo link. Got: {:?}",
        body["data"]
    );
    assert_eq!(
        demos[0]["link"]["id"].as_str().unwrap(),
        demo_link.id.to_string(),
        "Link ID should match"
    );
}
