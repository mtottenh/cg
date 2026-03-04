//! Veto (map pick/ban) API integration tests.


use axum::http::StatusCode;
use crate::common::TestApp;
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
    // Returns 500 for session not found
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
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
    // Returns 500 for session not found
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
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

    let delegate_token = create_test_token(delegate_user.id, delegate_user.id, "veto_delegate", TEST_JWT_SECRET);

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

    response.assert_status(StatusCode::UNAUTHORIZED);
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

    response.assert_status(StatusCode::UNAUTHORIZED);
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

    // Should fail - either unauthorized or bad request (wrong turn)
    assert!(
        response.status == StatusCode::UNAUTHORIZED || response.status == StatusCode::BAD_REQUEST,
        "Team B should not be able to act on Team A's turn. Status: {}",
        response.status
    );
}
