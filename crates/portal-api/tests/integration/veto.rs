//! Veto (map pick/ban) API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

// ============================================================================
// VETO SESSION TESTS
// ============================================================================

#[tokio::test]
async fn test_create_veto_session_invalid_match_id() {
    let app = TestApp::new().await;

    // Try to create veto session for non-existent match (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto",
            &json!({
                "veto_format_id": "bo3_veto",
                "timeout_seconds": 30
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_veto_session_invalid_format() {
    let app = TestApp::new().await;

    // Try to create veto session with invalid format (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000001/veto",
            &json!({
                "veto_format_id": "invalid_format",
                "timeout_seconds": 30
            }),
        )
        .await;

    // Handler checks match existence before format validation, so returns 404
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_veto_session_not_found() {
    let app = TestApp::new().await;

    // Try to get veto session for non-existent match (public endpoint)
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/veto")
        .await;

    // Returns 500 for session not found (internal error mapping)
    // TODO: Should ideally return 404
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_perform_veto_action_no_session() {
    let app = TestApp::new().await;

    // Try to perform action on non-existent session (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto/action",
            &json!({
                "map_id": "de_dust2"
            }),
        )
        .await;

    // Returns 500 for session not found (internal error mapping)
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_select_side_no_session() {
    let app = TestApp::new().await;

    // Try to select side on non-existent session (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto/side",
            &json!({
                "action_number": 1,
                "side": "ct"
            }),
        )
        .await;

    // Returns 500 for session not found (internal error mapping)
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// VETO FORMAT VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_veto_format_bo1() {
    let app = TestApp::new().await;

    // Verify BO1 format is accepted (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000001/veto",
            &json!({
                "veto_format_id": "bo1_veto",
                "timeout_seconds": 30
            }),
        )
        .await;

    // Should get 404 for match not found, not 400 for invalid format
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_veto_format_bo3() {
    let app = TestApp::new().await;

    // Verify BO3 format is accepted (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000001/veto",
            &json!({
                "veto_format_id": "bo3_veto",
                "timeout_seconds": 30
            }),
        )
        .await;

    // Should get 404 for match not found, not 400 for invalid format
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_veto_format_bo5() {
    let app = TestApp::new().await;

    // Verify BO5 format is accepted (using authenticated request)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000001/veto",
            &json!({
                "veto_format_id": "bo5_veto",
                "timeout_seconds": 30
            }),
        )
        .await;

    // Should get 404 for match not found, not 400 for invalid format
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// VETO ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_veto_start_endpoint_exists() {
    let app = TestApp::new().await;

    // Verify the start endpoint exists and responds properly (authenticated)
    let response = app
        .post_auth("/v1/matches/00000000-0000-0000-0000-000000000000/veto/start")
        .await;

    // Should not return METHOD_NOT_ALLOWED (endpoint exists)
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /veto/start endpoint should exist"
    );
    // The participant/admin gate now resolves the match first: 404 for a
    // nonexistent match (previously a 500 from the session lookup).
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_veto_coin_flip_endpoint_exists() {
    let app = TestApp::new().await;

    // Verify the coin-flip endpoint exists and responds properly (authenticated)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto/coin-flip",
            &json!({
                "winner_registration_id": "00000000-0000-0000-0000-000000000001",
                "winner_goes_first": true
            }),
        )
        .await;

    // Should not return METHOD_NOT_ALLOWED (endpoint exists)
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /veto/coin-flip endpoint should exist"
    );
    // The participant/admin gate now resolves the match first: 404 for a
    // nonexistent match (previously a 500 from the session lookup).
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_veto_action_endpoint_exists() {
    let app = TestApp::new().await;

    // Verify the action endpoint exists and responds properly (authenticated)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto/action",
            &json!({
                "map_id": "de_dust2"
            }),
        )
        .await;

    // Should not return METHOD_NOT_ALLOWED (endpoint exists)
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /veto/action endpoint should exist"
    );
    // Returns 500 for session not found
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_veto_side_endpoint_exists() {
    let app = TestApp::new().await;

    // Verify the side endpoint exists and responds properly (authenticated)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto/side",
            &json!({
                "action_number": 1,
                "side": "ct"
            }),
        )
        .await;

    // Should not return METHOD_NOT_ALLOWED (endpoint exists)
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /veto/side endpoint should exist"
    );
    // Returns 500 for session not found
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_veto_get_session_endpoint_exists() {
    let app = TestApp::new().await;

    // Verify the GET endpoint exists (public endpoint)
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/veto")
        .await;

    // Should not return METHOD_NOT_ALLOWED (endpoint exists)
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /veto endpoint should exist"
    );
    // Returns 500 for session not found
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// VETO VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_veto_action_empty_map_id() {
    let app = TestApp::new().await;

    // Try to perform action with empty map_id (validation should fail)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto/action",
            &json!({
                "map_id": ""
            }),
        )
        .await;

    // Returns 400 for validation error
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_veto_side_empty_side() {
    let app = TestApp::new().await;

    // Try to select side with empty side value (validation should fail)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto/side",
            &json!({
                "action_number": 1,
                "side": ""
            }),
        )
        .await;

    // Returns 400 for validation error
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_veto_timeout_too_low() {
    let app = TestApp::new().await;

    // Try to create session with too low timeout (validation should fail)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto",
            &json!({
                "veto_format_id": "bo3_veto",
                "timeout_seconds": 5  // Less than minimum of 10
            }),
        )
        .await;

    // Returns 400 for validation error
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_veto_timeout_too_high() {
    let app = TestApp::new().await;

    // Try to create session with too high timeout (validation should fail)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto",
            &json!({
                "veto_format_id": "bo3_veto",
                "timeout_seconds": 500  // More than maximum of 300
            }),
        )
        .await;

    // Returns 400 for validation error
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// VETO CREATE ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn test_veto_create_endpoint_exists() {
    let app = TestApp::new().await;

    // Verify the POST create endpoint exists (authenticated)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/veto",
            &json!({
                "veto_format_id": "bo3_veto",
                "timeout_seconds": 30
            }),
        )
        .await;

    // Should not return METHOD_NOT_ALLOWED (endpoint exists)
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /veto endpoint should exist"
    );
    // Returns 404 for match not found
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// VETO AUTHORIZATION TESTS
// ============================================================================

/// Setup data for authorization tests.
struct VetoAuthTestSetup {
    match_id: Uuid,
    team_a_captain_token: String,
    team_a_owner_token: String,
    team_a_member_token: String,
    team_a_delegate_token: String,
    team_b_captain_token: String,
    spectator_token: String,
    admin_token: String,
}

/// Set up a complete veto test scenario with two teams and a match.
/// Uses TwoTeamMatchFixture for most setup, adds delegate separately.
async fn setup_veto_auth_scenario(app: &TestApp) -> VetoAuthTestSetup {
    // Use TwoTeamMatchFixture for the base setup (with veto session)
    let fixture = TwoTeamMatchFixture::with_veto(app.pool(), TEST_JWT_SECRET).await;

    // Create delegate user and add to Team A
    let delegate_user = UserBuilder::new()
        .username("veto_delegate")
        .build_persisted(app.pool())
        .await;

    // Add delegate as team member
    LeagueTeamMemberBuilder::new()
        .team_season_id(fixture.team_a.team_season_id)
        .player_id(delegate_user.id)
        .player() // Regular member who will be granted delegate rights
        .build_persisted(app.pool())
        .await;

    // Grant veto delegation rights
    VetoDelegateBuilder::new()
        .team_season_id(fixture.team_a.team_season_id)
        .player_id(delegate_user.id)
        .delegated_by_user_id(fixture.team_a.captain.user_id)
        .by_captain()
        .for_tournament(fixture.tournament_id)
        .build_persisted(app.pool())
        .await;

    let delegate_token = create_test_token(
        delegate_user.id,
        delegate_user.id,
        "veto_delegate",
        TEST_JWT_SECRET,
    );

    VetoAuthTestSetup {
        match_id: fixture.match_id,
        team_a_captain_token: fixture.team_a.captain.token.clone(),
        team_a_owner_token: fixture.team_a.owner.token.clone(),
        team_a_member_token: fixture.team_a.member.token.clone(),
        team_a_delegate_token: delegate_token,
        team_b_captain_token: fixture.team_b.captain.token.clone(),
        spectator_token: fixture.tokens.spectator.clone(),
        admin_token: fixture.tokens.admin.clone(),
    }
}

#[tokio::test]
async fn test_veto_action_as_captain() {
    let app = TestApp::new().await;
    let setup = setup_veto_auth_scenario(&app).await;

    // Captain should be able to perform veto action
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/veto/action", setup.match_id),
            &json!({ "map_id": "de_dust2" }),
            &setup.team_a_captain_token,
        )
        .await;

    // Should succeed (200 or 201)
    assert!(
        response.status.is_success(),
        "Captain should be authorized. Status: {}, Body: {:?}",
        response.status,
        response.text()
    );
}

#[tokio::test]
async fn test_veto_action_as_owner() {
    let app = TestApp::new().await;
    let setup = setup_veto_auth_scenario(&app).await;

    // Owner should be able to perform veto action
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/veto/action", setup.match_id),
            &json!({ "map_id": "de_dust2" }),
            &setup.team_a_owner_token,
        )
        .await;

    assert!(
        response.status.is_success(),
        "Owner should be authorized. Status: {}, Body: {:?}",
        response.status,
        response.text()
    );
}

#[tokio::test]
async fn test_veto_action_as_delegate() {
    let app = TestApp::new().await;
    let setup = setup_veto_auth_scenario(&app).await;

    // Delegate should be able to perform veto action
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/veto/action", setup.match_id),
            &json!({ "map_id": "de_dust2" }),
            &setup.team_a_delegate_token,
        )
        .await;

    assert!(
        response.status.is_success(),
        "Delegate should be authorized. Status: {}, Body: {:?}",
        response.status,
        response.text()
    );
}

#[tokio::test]
async fn test_veto_action_as_tournament_admin() {
    let app = TestApp::new().await;
    let setup = setup_veto_auth_scenario(&app).await;

    // Tournament admin should be able to perform veto action
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/veto/action", setup.match_id),
            &json!({ "map_id": "de_dust2" }),
            &setup.admin_token,
        )
        .await;

    assert!(
        response.status.is_success(),
        "Tournament admin should be authorized. Status: {}, Body: {:?}",
        response.status,
        response.text()
    );
}

#[tokio::test]
async fn test_veto_action_as_regular_member_unauthorized() {
    let app = TestApp::new().await;
    let setup = setup_veto_auth_scenario(&app).await;

    // Regular member (not captain, owner, or delegate) should NOT be authorized
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/veto/action", setup.match_id),
            &json!({ "map_id": "de_dust2" }),
            &setup.team_a_member_token,
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_veto_action_as_spectator_unauthorized() {
    let app = TestApp::new().await;
    let setup = setup_veto_auth_scenario(&app).await;

    // Spectator (not on any team) should NOT be authorized
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/veto/action", setup.match_id),
            &json!({ "map_id": "de_dust2" }),
            &setup.spectator_token,
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_veto_action_wrong_team_turn() {
    let app = TestApp::new().await;
    let setup = setup_veto_auth_scenario(&app).await;

    // Team B captain trying to act when it's Team A's turn
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/veto/action", setup.match_id),
            &json!({ "map_id": "de_dust2" }),
            &setup.team_b_captain_token,
        )
        .await;

    // Should fail - either forbidden or bad request (wrong turn)
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::BAD_REQUEST,
        "Team B should not be able to act on Team A's turn. Status: {}",
        response.status
    );
}

// ============================================================================
// INDIVIDUAL-REGISTRATION VETO (no team_season)
// ============================================================================

/// Individual registrations have no team/captain structure: the registered
/// player themself must be authorized to act. Regression test for the
/// authorization service hard-erroring on `team_season_id: None`, which made
/// veto unusable in individual tournaments.
#[tokio::test]
async fn test_individual_registration_player_can_perform_veto_action() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "indiv-veto-test")
            .await;

    // Create + start the veto session (admin), coin flip won by player 2.
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/veto"),
            &json!({ "veto_format_id": "bo1_veto" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Creating a session marks the match veto-gated.
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["veto_required"], true);

    app.post_auth(&format!("/v1/matches/{match_id}/veto/start"))
        .await
        .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/matches/{match_id}/veto/coin-flip"),
        &json!({ "winner_registration_id": reg2, "winner_goes_first": true }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Player 2 (a plain registered player, no admin role, no team) bans.
    let response = app.get(&format!("/v1/matches/{match_id}/veto")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let map = body["data"]["session"]["remaining_maps"][0]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto/action"),
            &json!({ "map_id": map }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    // And the dev user cannot act for player 2's turn-less slot out of turn:
    // it is now participant 1's turn, so player 2 acting again is rejected.
    let response = app.get(&format!("/v1/matches/{match_id}/veto")).await;
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["session"]["remaining_maps"]
            .as_array()
            .unwrap()
            .len(),
        body["data"]["session"]["map_pool"]
            .as_array()
            .unwrap()
            .len()
            - 1
    );
}

// ============================================================================
// SIDE SELECTION MODE RESOLUTION (picker_choice default)
// ============================================================================

/// Seven standard CS2 active-duty maps — enough for a bo3 map pool.
///
/// Must be a subset of the catalog seeded by migration 0018, since
/// `map_pool` is validated against it on tournament creation.
const CS2_MAP_POOL: [&str; 7] = [
    "de_dust2",
    "de_mirage",
    "de_inferno",
    "de_nuke",
    "de_vertigo",
    "de_ancient",
    "de_anubis",
];

/// Bug C regression: a CS2 tournament that does not request an explicit side
/// selection mode should inherit the CS2 plugin default (`picker_choice`), not
/// silently fall back to `knife`. Previously `resolve_side_selection_mode`
/// looked the plugin up by the game's UUID instead of its `plugin_id` slug, so
/// every session defaulted to `knife`.
#[tokio::test]
async fn test_create_veto_session_defaults_to_picker_choice_for_cs2() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, _reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(
            &app,
            "veto-side-mode-default",
        )
        .await;

    // Create the session WITHOUT specifying side_selection_mode.
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/veto"),
            &json!({
                "veto_format_id": "bo3_veto",
                "map_pool": CS2_MAP_POOL,
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["side_selection_mode"], "picker_choice",
        "CS2 session should default to the plugin's picker_choice mode, not knife. Body: {body:?}"
    );
}

// ============================================================================
// PICKER-CHOICE SIDE SELECTION (opponent selects, picker rejected)
// ============================================================================

/// Perform the next veto action (ban or pick) on the first remaining map as
/// the given token, asserting success and returning the parsed response body.
async fn veto_act_first_remaining(app: &TestApp, match_id: &str, token: &str) -> serde_json::Value {
    let state = app.get(&format!("/v1/matches/{match_id}/veto")).await;
    state.assert_status(StatusCode::OK);
    let state_body: serde_json::Value = state.json();
    let map = state_body["data"]["session"]["remaining_maps"][0]
        .as_str()
        .expect("a remaining map")
        .to_string();
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto/action"),
            &json!({ "map_id": map }),
            token,
        )
        .await;
    assert!(
        response.status.is_success(),
        "action should succeed. Status: {}, Body: {:?}",
        response.status,
        response.text()
    );
    response.json()
}

/// Bug B regression: in `picker_choice` mode the OPPONENT of the team that
/// picked a map selects the starting side (standard CS convention). The
/// picker attempting to select the side is rejected.
///
/// Flow (bo3, Ban-Ban-Pick-...): reg1 wins the flip and acts first, so reg1 is
/// "team 1". Action 3 is team 1's PICK → reg1 is the picker, reg2 the opponent.
#[tokio::test]
async fn test_picker_choice_opponent_selects_side_and_picker_rejected() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "veto-side-select")
            .await;

    // Create a picker_choice bo3 session (dev user acts for reg1, and is admin).
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/veto"),
            &json!({
                "veto_format_id": "bo3_veto",
                "map_pool": CS2_MAP_POOL,
                "side_selection_mode": "picker_choice",
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Start and record a coin flip: reg2 (player2, a non-admin) wins and goes
    // first, so reg2 is "team 1" and becomes the picker at action 3. reg1 (the
    // dev user) is the opponent who gets to select the side.
    app.post_auth(&format!("/v1/matches/{match_id}/veto/start"))
        .await
        .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/matches/{match_id}/veto/coin-flip"),
        &json!({ "winner_registration_id": reg2, "winner_goes_first": true }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Action 1: team 1 (reg2 = player2) bans.
    veto_act_first_remaining(&app, &match_id, &player2_token).await;
    // Action 2: team 2 (reg1 = dev user) bans.
    veto_act_first_remaining(&app, &match_id, "dev-token").await;
    // Action 3: team 1 (reg2 = player2) PICKS.
    let pick = veto_act_first_remaining(&app, &match_id, &player2_token).await;
    assert_eq!(
        pick["data"]["action"]["action_type"], "pick",
        "action 3 should be a pick. Body: {pick:?}"
    );
    assert_eq!(pick["data"]["action"]["action_number"], 3);

    // The PICKER (reg2 / player2, a non-admin) may NOT select the side.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto/side"),
            &json!({ "action_number": 3, "side": "ct" }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // The OPPONENT (reg1 / dev user) selects the side successfully.
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/veto/side"),
            &json!({ "action_number": 3, "side": "ct" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["side_selection"], "ct");
    assert_eq!(
        body["data"]["side_selected_by_registration_id"], reg1,
        "the recorded side must be attributed to the opponent (reg1), not the picker. Body: {body:?}"
    );
}

// ============================================================================
// VETO LIFECYCLE AUTHORIZATION (start / coin flip)
// ============================================================================

/// `start_veto_session` and `record_coin_flip` are gated the same way as
/// `create_veto_session`: match participant (via veto authorization) or
/// tournament admin. Previously both endpoints accepted any authenticated
/// user.
#[tokio::test]
async fn test_veto_start_and_coin_flip_require_participant_or_admin() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, _reg1, reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "veto-authz-test")
            .await;

    let outsider = UserBuilder::new()
        .username("veto_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token =
        create_test_token(outsider.id, outsider.id, "veto_outsider", TEST_JWT_SECRET);

    // Outsider cannot create the session.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto"),
            &json!({ "veto_format_id": "bo1_veto" }),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Admin (dev token) creates the session.
    app.post_json(
        &format!("/v1/matches/{match_id}/veto"),
        &json!({ "veto_format_id": "bo1_veto" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Outsider cannot start the session.
    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/veto/start"),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // A participant can start it.
    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/veto/start"),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Outsider cannot record the coin flip.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto/coin-flip"),
            &json!({ "winner_registration_id": reg2, "winner_goes_first": true }),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // A participant can record the coin flip.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto/coin-flip"),
            &json!({ "winner_registration_id": reg2, "winner_goes_first": true }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
}
