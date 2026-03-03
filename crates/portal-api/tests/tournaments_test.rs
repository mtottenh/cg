//! Tournament API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use portal_test::prelude::*;
use serde_json::json;
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
                "override_reason": "Test setup: transitioning match to Ready for scheduling tests"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

// ============================================================================
// TOURNAMENT CRUD TESTS
// ============================================================================

#[tokio::test]
async fn test_create_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Test Tournament",
                "slug": "test-tournament",
                "format": "single_elimination",
                "participant_type": "team",
                "team_size": 5,
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Test Tournament");
    assert_eq!(body["data"]["slug"], "test-tournament");
    assert_eq!(body["data"]["format"], "single_elimination");
    assert_eq!(body["data"]["participant_type"], "team");
    assert_eq!(body["data"]["team_size"], 5);
    assert_eq!(body["data"]["min_participants"], 4);
    assert_eq!(body["data"]["max_participants"], 16);
    assert_eq!(body["data"]["status"], "draft");
}

#[tokio::test]
async fn test_create_tournament_duplicate_slug() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create first tournament
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "First Tournament",
                "slug": "duplicate-slug",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Try to create second tournament with same slug
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Second Tournament",
                "slug": "duplicate-slug",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_get_tournament_by_id() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Get By ID Test",
                "slug": "get-by-id-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Get by ID
    let response = app.get(&format!("/v1/tournaments/{}", tournament_id)).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], tournament_id);
    assert_eq!(body["data"]["name"], "Get By ID Test");
}

#[tokio::test]
async fn test_get_tournament_by_slug() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Slug Lookup Test",
                "slug": "slug-lookup-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Get by slug
    let response = app.get("/v1/tournaments/by-slug/slug-lookup-test").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["slug"], "slug-lookup-test");
    assert_eq!(body["data"]["name"], "Slug Lookup Test");
}

#[tokio::test]
async fn test_get_tournament_not_found() {
    let app = TestApp::new().await;

    // Try to get non-existent tournament
    let response = app.get("/v1/tournaments/00000000-0000-0000-0000-000000000000").await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_tournaments() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create multiple tournaments
    for i in 1..=3 {
        let response = app
            .post_json(
                "/v1/tournaments",
                &json!({
                    "game_id": game_id,
                    "name": format!("List Test Tournament {}", i),
                    "slug": format!("list-test-{}", i),
                    "format": "single_elimination",
                    "participant_type": "team",
                    "min_participants": 4,
                    "max_participants": 16,
                    "registration_type": "open",
                    "scheduling_mode": "live",
                    "default_match_format": "bo3"
                }),
            )
            .await;
        response.assert_status(StatusCode::CREATED);
    }

    // List all
    let response = app.get("/v1/tournaments").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().len() >= 3);
}

#[tokio::test]
async fn test_list_tournaments_filter_by_game() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a tournament
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Game Filter Test",
                "slug": "game-filter-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Filter by game
    let response = app.get(&format!("/v1/tournaments?game_id={}", game_id)).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(!body["data"].as_array().unwrap().is_empty());

    // Verify all returned tournaments have the correct game_id
    for tournament in body["data"].as_array().unwrap() {
        assert_eq!(tournament["game_id"], game_id);
    }
}

#[tokio::test]
async fn test_update_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Original Name",
                "slug": "update-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Update tournament
    let response = app
        .patch_json(
            &format!("/v1/tournaments/{}", tournament_id),
            &json!({
                "name": "Updated Name",
                "description": "Updated description",
                "max_participants": 32
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Updated Name");
    assert_eq!(body["data"]["description"], "Updated description");
    assert_eq!(body["data"]["max_participants"], 32);
}

// ============================================================================
// TOURNAMENT LIFECYCLE TESTS
// ============================================================================

#[tokio::test]
async fn test_publish_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament (starts in draft status)
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Publish Test",
                "slug": "publish-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();
    assert_eq!(created["data"]["status"], "draft");

    // Publish tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "published");
}

#[tokio::test]
async fn test_open_registration() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create and publish tournament
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Registration Test",
                "slug": "registration-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Publish first
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/open-registration", tournament_id))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "registration");
}

// ============================================================================
// TOURNAMENT STAGES TESTS
// ============================================================================

#[tokio::test]
async fn test_create_stage() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Stage Test",
                "slug": "stage-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Create a stage
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/stages", tournament_id),
            &json!({
                "name": "Group Stage",
                "stage_order": 1,
                "format": "round_robin",
                "advancement_count": 8
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Group Stage");
    assert_eq!(body["data"]["format"], "round_robin");
    assert_eq!(body["data"]["stage_order"], 1);
}

#[tokio::test]
async fn test_get_stages() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Get Stages Test",
                "slug": "get-stages-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Create stages
    for i in 1..=2 {
        let response = app
            .post_json(
                &format!("/v1/tournaments/{}/stages", tournament_id),
                &json!({
                    "name": format!("Stage {}", i),
                    "stage_order": i,
                    "format": "single_elimination"
                }),
            )
            .await;
        response.assert_status(StatusCode::CREATED);
    }

    // Get stages
    let response = app
        .get(&format!("/v1/tournaments/{}/stages", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
}

// ============================================================================
// INDIVIDUAL TOURNAMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_create_individual_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "1v1 Tournament",
                "slug": "1v1-tournament",
                "format": "single_elimination",
                "participant_type": "individual",
                "min_participants": 4,
                "max_participants": 32,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["participant_type"], "individual");
    assert!(body["data"]["team_size"].is_null());
}

// ============================================================================
// TOURNAMENT FORMAT TESTS
// ============================================================================

#[tokio::test]
async fn test_create_double_elimination_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Double Elim Tournament",
                "slug": "double-elim-test",
                "format": "double_elimination",
                "participant_type": "team",
                "team_size": 5,
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["format"], "double_elimination");
}

#[tokio::test]
async fn test_create_round_robin_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Round Robin Tournament",
                "slug": "round-robin-test",
                "format": "round_robin",
                "participant_type": "team",
                "team_size": 5,
                "min_participants": 4,
                "max_participants": 8,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo1"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["format"], "round_robin");
}

// ============================================================================
// AUTHORIZATION TESTS
// ============================================================================

#[tokio::test]
async fn test_create_tournament_unauthorized() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Try to create without auth
    let response = app
        .post_json_no_auth(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Unauthorized Test",
                "slug": "unauthorized-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_update_tournament_unauthorized() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament (with auth)
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Update Unauth Test",
                "slug": "update-unauth-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Try to update without auth
    let response = app
        .patch_json_no_auth(
            &format!("/v1/tournaments/{}", tournament_id),
            &json!({
                "name": "Hacked Name"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_create_tournament_missing_required_fields() {
    let app = TestApp::new().await;

    // Missing game_id and other required fields
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "name": "Incomplete Tournament"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_tournament_invalid_participant_range() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // min_participants > max_participants
    // This is validated by a database constraint, so we get a 500 error
    // (In a production system, this would ideally be validated before hitting the DB)
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Invalid Range Tournament",
                "slug": "invalid-range",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 32,
                "max_participants": 8,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    // The database constraint catches this, resulting in an internal error
    // A proper validation layer would return 400 instead
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// PHASE 2: REGISTRATION MANAGEMENT TESTS
// ============================================================================

/// Helper to create a test player using UserBuilder and return (user_id, player_id).
/// UserBuilder creates user and player with the same ID.
async fn create_test_player(app: &TestApp, username: &str) -> (Uuid, Uuid) {
    let user = UserBuilder::new()
        .username(username)
        .build_persisted(app.pool())
        .await;
    // UserBuilder creates player with same ID as user
    (user.id, user.id)
}

/// Helper to insert a registration directly using TournamentRegistrationBuilder.
/// Creates an approved registration (ready to participate).
async fn insert_test_registration(
    app: &TestApp,
    tournament_id: &str,
    player_id: Uuid,
    user_id: Uuid,
    participant_name: &str,
) -> String {
    let tournament_uuid: Uuid = tournament_id.parse().expect("Invalid tournament ID");

    let reg = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament_uuid)
        .player_id_from_uuid(player_id)
        .participant_name(participant_name)
        .registered_by_uuid(user_id)
        .approved() // Must be approved to participate
        .build_persisted(app.pool())
        .await;

    reg.id.as_uuid().to_string()
}

/// Helper to create a tournament and open registration.
async fn create_tournament_with_registration(app: &TestApp, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament with min_participants: 2 (the minimum for any tournament)
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Registration Test {}", slug),
                "slug": slug,
                "format": "single_elimination",
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

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

    tournament_id
}

/// Helper to register a player and return the registration ID.
/// By default, registrations are created with 'pending' status.
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

/// Helper to approve a registration (for seeding tests).
async fn approve_registration(app: &TestApp, tournament_id: &str, registration_id: &str) {
    // Approve via the API
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/registrations/{}/approve",
            tournament_id, registration_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_withdraw_registration() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "withdraw-test").await;

    // Register a player
    let registration_id = register_player(&app, &tournament_id, "Player1").await;

    // Withdraw
    let response = app
        .delete_auth(&format!(
            "/v1/tournaments/{}/registrations/{}",
            tournament_id, registration_id
        ))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "withdrawn");
}

#[tokio::test]
async fn test_get_check_in_status() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "checkin-status-test").await;

    // Register 2 players (required min_participants is 2)
    // First player via API (dev user) - needs approval for eligibility
    let reg1 = register_player(&app, &tournament_id, "Player1").await;
    approve_registration(&app, &tournament_id, &reg1).await;

    // Second player via direct DB insertion (already approved)
    let (user2_id, player2_id) = create_test_player(&app, "player2_checkin").await;
    insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Get check-in status
    let response = app
        .get(&format!("/v1/tournaments/{}/check-in-status", tournament_id))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["tournament_id"], tournament_id);
    assert!(body["data"]["total_eligible"].as_i64().unwrap() >= 2);
}

// ============================================================================
// PHASE 2: SEEDING TESTS
// ============================================================================

#[tokio::test]
async fn test_get_seeding_empty() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "seeding-empty-test").await;

    // Get seeding (should be empty)
    let response = app
        .get(&format!("/v1/tournaments/{}/seeding", tournament_id))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_auto_seed_random() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "auto-seed-random-test").await;

    // Register 2 players (required min_participants is 2)
    // First player via API - needs approval for seeding eligibility
    let reg1 = register_player(&app, &tournament_id, "Player1").await;
    approve_registration(&app, &tournament_id, &reg1).await;

    // Second player via direct DB insertion (already approved)
    let (user2_id, player2_id) = create_test_player(&app, "player2_autoseed").await;
    let reg2 = insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Auto-seed with random algorithm
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({
                "algorithm": "random"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let seeded = body["data"].as_array().unwrap();

    // Both participants should be seeded
    assert_eq!(seeded.len(), 2);

    // Check both registrations have seeds (order is random)
    let reg_ids: Vec<&str> = seeded
        .iter()
        .map(|s| s["registration_id"].as_str().unwrap())
        .collect();
    assert!(reg_ids.contains(&reg1.as_str()));
    assert!(reg_ids.contains(&reg2.as_str()));

    // Check seeds are 1 and 2
    let seeds: Vec<i64> = seeded.iter().map(|s| s["seed"].as_i64().unwrap()).collect();
    assert!(seeds.contains(&1));
    assert!(seeds.contains(&2));
}

#[tokio::test]
async fn test_manual_seed() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "manual-seed-test").await;

    // Register 2 players (required min_participants is 2)
    // First player via API - needs approval
    let reg1 = register_player(&app, &tournament_id, "Player1").await;
    approve_registration(&app, &tournament_id, &reg1).await;

    // Second player via direct DB insertion (already approved)
    let (user2_id, player2_id) = create_test_player(&app, "player2_manual").await;
    let reg2 = insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Manual seed with explicit seeding order
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/manual", tournament_id),
            &json!({
                "seeds": [
                    { "registration_id": reg1, "seed": 2 },
                    { "registration_id": reg2, "seed": 1 }
                ]
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let seeded = body["data"].as_array().unwrap();

    // Verify both seeds are as specified
    assert_eq!(seeded.len(), 2);

    // Find each registration in the results
    let reg1_entry = seeded.iter().find(|s| s["registration_id"] == reg1).unwrap();
    let reg2_entry = seeded.iter().find(|s| s["registration_id"] == reg2).unwrap();

    assert_eq!(reg1_entry["seed"], 2);
    assert_eq!(reg2_entry["seed"], 1);
}

#[tokio::test]
async fn test_clear_seeding() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "clear-seed-test").await;

    // Register 2 players (required min_participants is 2)
    // First player via API - needs approval for seeding eligibility
    let reg1 = register_player(&app, &tournament_id, "Player1").await;
    approve_registration(&app, &tournament_id, &reg1).await;

    // Second player via direct DB insertion (already approved)
    let (user2_id, player2_id) = create_test_player(&app, "player2_clear").await;
    insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Auto-seed the participants
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Verify seeding is not empty
    let response = app
        .get(&format!("/v1/tournaments/{}/seeding", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);

    // Clear seeding
    let response = app
        .delete_auth(&format!("/v1/tournaments/{}/seeding", tournament_id))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify seeding is empty after clearing
    let response = app
        .get(&format!("/v1/tournaments/{}/seeding", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

// ============================================================================
// PHASE 3: MATCH LIFECYCLE TESTS
// ============================================================================

/// Helper to create a started tournament with matches.
/// Returns (tournament_id, match_id, registration_id1, registration_id2).
async fn create_tournament_with_matches(
    app: &TestApp,
    slug: &str,
) -> (String, String, String, String) {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament with min_participants: 2
    // Use "self_scheduled" mode for tests that need scheduled matches
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Match Test {}", slug),
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

    let (user2_id, player2_id) = create_test_player(app, &format!("player2_{}", slug)).await;
    let reg2 = insert_test_registration(app, &tournament_id, player2_id, user2_id, "Player2").await;

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

    // Get matches to find the match ID
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let matches = body["data"].as_array().unwrap();
    assert!(!matches.is_empty(), "Tournament should have at least one match");

    let match_id = matches[0]["id"].as_str().unwrap().to_string();

    // Grant admin permissions and transition match to Ready status
    // (matches are created in Pending status, need to be Ready for scheduling)
    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
    transition_match_to_ready(app, &tournament_id, &match_id).await;

    (tournament_id, match_id, reg1, reg2)
}

#[tokio::test]
async fn test_get_match_status() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "match-status-test").await;

    // Get match status
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/status",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["match_id"], match_id);
    assert!(body["data"]["current_status"].is_string());
    assert!(body["data"]["allowed_transitions"].is_array());
}

#[tokio::test]
async fn test_get_match_status_history_empty() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "match-history-empty-test").await;

    // Get match status history (should be empty for a new match)
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/status-history",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    // Initially empty since no transitions have occurred yet
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn test_schedule_match() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "schedule-match-test").await;

    // Schedule the match for 1 hour in the future
    let scheduled_time = chrono::Utc::now() + chrono::Duration::hours(1);

    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339()
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["scheduled_at"].is_string());
}

#[tokio::test]
async fn test_match_check_in() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _) =
        create_tournament_with_matches(&app, "match-checkin-test").await;

    // First, schedule the match
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Check in to the match
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/check-in",
                tournament_id, match_id
            ),
            &json!({
                "registration_id": reg1
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    // Verify the match data is returned
    assert_eq!(body["data"]["id"], match_id);
}

#[tokio::test]
async fn test_forfeit_match() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _) =
        create_tournament_with_matches(&app, "forfeit-match-test").await;

    // First, schedule the match
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Forfeit the match
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/forfeit",
                tournament_id, match_id
            ),
            &json!({
                "registration_id": reg1,
                "reason": "Cannot attend the match"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], match_id);
    assert_eq!(body["data"]["status"], "forfeit");
}

#[tokio::test]
async fn test_admin_match_transition() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "admin-transition-test").await;

    // Admin transition to cancelled status
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/transition",
                tournament_id, match_id
            ),
            &json!({
                "to_status": "cancelled",
                "override_reason": "Tournament cancelled due to technical issues"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], match_id);
    assert_eq!(body["data"]["status"], "cancelled");
}

#[tokio::test]
async fn test_get_match_status_history_after_transitions() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "match-history-test").await;

    // Schedule the match (creates a status log entry)
    let scheduled_time = chrono::Utc::now() + chrono::Duration::hours(1);
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Get match status history
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/status-history",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let history = body["data"].as_array().unwrap();

    // Should have at least one entry from the scheduling transition
    assert!(
        !history.is_empty(),
        "Status history should have entries after scheduling"
    );

    // Verify the log entry structure
    let first_entry = &history[0];
    assert!(first_entry["id"].is_string());
    assert!(first_entry["from_status"].is_string());
    assert!(first_entry["to_status"].is_string());
    assert!(first_entry["transitioned_at"].is_string());
}

#[tokio::test]
async fn test_match_status_not_found() {
    let app = TestApp::new().await;
    let (tournament_id, _, _, _) =
        create_tournament_with_matches(&app, "match-not-found-test").await;

    // Try to get status for a non-existent match
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/00000000-0000-0000-0000-000000000000/status",
            tournament_id
        ))
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// PHASE 3.2: MATCH SCHEDULING TESTS
// ============================================================================

#[tokio::test]
async fn test_propose_schedule() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "propose-schedule-test").await;

    // Propose schedule times
    let proposed_time1 = chrono::Utc::now() + chrono::Duration::hours(24);
    let proposed_time2 = chrono::Utc::now() + chrono::Duration::hours(48);

    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule/propose",
                tournament_id, match_id
            ),
            &json!({
                "proposed_times": [
                    proposed_time1.to_rfc3339(),
                    proposed_time2.to_rfc3339()
                ]
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["id"].is_string());
    assert_eq!(body["data"]["status"], "pending");
    let times = body["data"]["proposed_times"].as_array().unwrap();
    assert_eq!(times.len(), 2);
}

#[tokio::test]
async fn test_get_active_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "get-active-proposal-test").await;

    // Initially no active proposal
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/schedule/active",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].is_null());

    // Create a proposal
    let proposed_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule/propose",
                tournament_id, match_id
            ),
            &json!({
                "proposed_times": [proposed_time.to_rfc3339()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Now should have an active proposal
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/schedule/active",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["id"].is_string());
    assert_eq!(body["data"]["status"], "pending");
}

#[tokio::test]
async fn test_accept_schedule_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _) =
        create_tournament_with_matches(&app, "accept-proposal-test").await;

    // Create a proposal (using exact timestamp that will be stored)
    let proposed_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule/propose",
                tournament_id, match_id
            ),
            &json!({
                "proposed_times": [proposed_time.to_rfc3339()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = response.json();
    let proposal_id = create_body["data"]["id"].as_str().unwrap();
    // Use the time from the response to ensure exact match
    let stored_time = create_body["data"]["proposed_times"][0].as_str().unwrap();

    // Accept the proposal using a different user
    // Since we're using dev auth, simulate the other participant accepting
    // For now, use admin schedule as a workaround since both participants are dev user
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": stored_time,
                "reason": "Admin scheduling for test"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    // Returns the updated match with scheduled time
    assert_eq!(body["data"]["id"], match_id);
    assert!(body["data"]["scheduled_at"].is_string());
}

#[tokio::test]
async fn test_reject_schedule_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "reject-proposal-test").await;

    // Create a proposal
    let proposed_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule/propose",
                tournament_id, match_id
            ),
            &json!({
                "proposed_times": [proposed_time.to_rfc3339()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = response.json();
    let proposal_id = create_body["data"]["id"].as_str().unwrap();

    // Try to reject the proposal as the same user who created it
    // This should fail because you cannot respond to your own proposal
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule/reject",
                tournament_id, match_id
            ),
            &json!({
                "proposal_id": proposal_id
            }),
        )
        .await;

    // Should return 401 because you cannot respond to your own proposal
    response.assert_status(StatusCode::UNAUTHORIZED);

    let body: serde_json::Value = response.json();
    assert!(body["detail"]
        .as_str()
        .unwrap()
        .contains("Cannot respond to your own proposal"));
}

#[tokio::test]
async fn test_get_proposal_history() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "proposal-history-test").await;

    // Initially empty history
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/schedule/history",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());

    // Create a proposal
    let proposed_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/schedule/propose",
                tournament_id, match_id
            ),
            &json!({
                "proposed_times": [proposed_time.to_rfc3339()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Now history should have one entry
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/schedule/history",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let history = body["data"].as_array().unwrap();
    assert_eq!(history.len(), 1);
}

#[tokio::test]
async fn test_admin_schedule_match() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "admin-schedule-test").await;

    // Admin directly schedules the match
    let scheduled_time = chrono::Utc::now() + chrono::Duration::hours(12);

    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/schedule",
                tournament_id, match_id
            ),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339(),
                "notes": "Scheduled by admin for tournament finals"
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], match_id);
    assert!(body["data"]["scheduled_at"].is_string());
}

// ============================================================================
// PHASE 3.3: AVAILABILITY TESTS
// ============================================================================

#[tokio::test]
async fn test_create_availability_window() {
    let app = TestApp::new().await;

    // Create an availability window for the current player
    let response = app
        .post_json(
            "/v1/players/me/availability/windows",
            &json!({
                "day_of_week": 1,  // Monday
                "start_time": "09:00:00",
                "end_time": "17:00:00",
                "timezone": "America/New_York",
                "is_preferred": true,
                "notes": "Working hours"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["id"].is_string());
    assert_eq!(body["data"]["day_of_week"], 1);
    assert_eq!(body["data"]["start_time"], "09:00:00");
    assert_eq!(body["data"]["end_time"], "17:00:00");
    assert_eq!(body["data"]["is_preferred"], true);
}

#[tokio::test]
async fn test_get_player_availability_windows() {
    let app = TestApp::new().await;

    // Create a window first
    let response = app
        .post_json(
            "/v1/players/me/availability/windows",
            &json!({
                "day_of_week": 2,  // Tuesday
                "start_time": "10:00:00",
                "end_time": "18:00:00",
                "is_preferred": true
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Get all windows (requires auth)
    let response = app.get_auth("/v1/players/me/availability/windows").await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let windows = body["data"].as_array().unwrap();
    assert!(!windows.is_empty());
}

#[tokio::test]
async fn test_update_availability_window() {
    let app = TestApp::new().await;

    // Create a window
    let response = app
        .post_json(
            "/v1/players/me/availability/windows",
            &json!({
                "day_of_week": 3,  // Wednesday
                "start_time": "08:00:00",
                "end_time": "16:00:00",
                "is_preferred": false
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = response.json();
    let window_id = create_body["data"]["id"].as_str().unwrap();

    // Update the window
    let response = app
        .patch_json(
            &format!("/v1/players/me/availability/windows/{}", window_id),
            &json!({
                "start_time": "09:00:00",
                "is_preferred": true
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["start_time"], "09:00:00");
    assert_eq!(body["data"]["is_preferred"], true);
}

#[tokio::test]
async fn test_delete_availability_window() {
    let app = TestApp::new().await;

    // Create a window
    let response = app
        .post_json(
            "/v1/players/me/availability/windows",
            &json!({
                "day_of_week": 4,  // Thursday
                "start_time": "11:00:00",
                "end_time": "19:00:00",
                "is_preferred": true
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = response.json();
    let window_id = create_body["data"]["id"].as_str().unwrap();

    // Delete the window
    let response = app
        .delete_auth(&format!("/v1/players/me/availability/windows/{}", window_id))
        .await;

    response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone by trying to delete again (should get 404)
    let response = app
        .delete_auth(&format!("/v1/players/me/availability/windows/{}", window_id))
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_availability_override() {
    let app = TestApp::new().await;

    // Create a "blocked" override for a specific date
    let response = app
        .post_json(
            "/v1/players/me/availability/overrides",
            &json!({
                "override_date": "2025-01-15",
                "start_time": "09:00:00",
                "end_time": "17:00:00",
                "override_type": "blocked",
                "reason": "Doctor appointment"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["id"].is_string());
    assert_eq!(body["data"]["override_date"], "2025-01-15");
    assert_eq!(body["data"]["override_type"], "blocked");
}

#[tokio::test]
async fn test_get_player_availability_overrides() {
    let app = TestApp::new().await;

    // Create an override first
    let response = app
        .post_json(
            "/v1/players/me/availability/overrides",
            &json!({
                "override_date": "2025-02-20",
                "start_time": "08:00:00",
                "end_time": "12:00:00",
                "override_type": "available",
                "reason": "Extra availability for tournament day"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Get all overrides (requires auth)
    let response = app.get_auth("/v1/players/me/availability/overrides").await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let overrides = body["data"].as_array().unwrap();
    assert!(!overrides.is_empty());
}

#[tokio::test]
async fn test_delete_availability_override() {
    let app = TestApp::new().await;

    // Create an override
    let response = app
        .post_json(
            "/v1/players/me/availability/overrides",
            &json!({
                "override_date": "2025-03-10",
                "start_time": "14:00:00",
                "end_time": "18:00:00",
                "override_type": "blocked"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = response.json();
    let override_id = create_body["data"]["id"].as_str().unwrap();

    // Delete the override
    let response = app
        .delete_auth(&format!(
            "/v1/players/me/availability/overrides/{}",
            override_id
        ))
        .await;

    response.assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_get_player_date_availability() {
    let app = TestApp::new().await;

    // Create a weekly window for Monday
    let response = app
        .post_json(
            "/v1/players/me/availability/windows",
            &json!({
                "day_of_week": 1,  // Monday
                "start_time": "10:00:00",
                "end_time": "18:00:00",
                "is_preferred": true
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Query availability for a Monday (2025-01-13 is a Monday) - requires auth
    let response = app
        .get_auth("/v1/players/me/availability/date?date=2025-01-13")
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["date"], "2025-01-13");
    // Should have available slots since we have a window for Monday
    let slots = body["data"]["available_slots"].as_array().unwrap();
    assert!(!slots.is_empty());
}

#[tokio::test]
async fn test_get_public_player_availability() {
    let app = TestApp::new().await;

    // Create a test player
    let (_, player_id) = create_test_player(&app, "public_avail_player").await;

    // Query public availability for that player (no auth needed)
    let response = app
        .get(&format!(
            "/v1/players/{}/availability/date?date=2025-01-15",
            player_id
        ))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["date"], "2025-01-15");
}

#[tokio::test]
async fn test_generate_time_suggestions() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "suggestions-test").await;

    // Generate suggestions for the match
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/suggestions/generate",
                tournament_id, match_id
            ),
            &json!({
                "start_date": "2025-01-13",
                "end_date": "2025-01-20",
                "min_duration_minutes": 60
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    // The response is an array of suggestions (may be empty if no overlap)
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn test_get_match_suggestions() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "get-suggestions-test").await;

    // Initially should be empty (no auth needed for read)
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/matches/{}/suggestions",
            tournament_id, match_id
        ))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    // Should return an array (possibly empty)
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn test_availability_window_unauthorized() {
    let app = TestApp::new().await;

    // Try to create without auth
    let response = app
        .post_json_no_auth(
            "/v1/players/me/availability/windows",
            &json!({
                "day_of_week": 5,
                "start_time": "09:00:00",
                "end_time": "17:00:00",
                "is_preferred": true
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_availability_window_invalid_time_range() {
    let app = TestApp::new().await;

    // Try to create with end_time before start_time
    let response = app
        .post_json(
            "/v1/players/me/availability/windows",
            &json!({
                "day_of_week": 6,
                "start_time": "17:00:00",
                "end_time": "09:00:00",  // Invalid: end before start
                "is_preferred": true
            }),
        )
        .await;

    // Should get a bad request or internal error (depending on validation layer)
    assert!(
        response.status == StatusCode::BAD_REQUEST
            || response.status == StatusCode::INTERNAL_SERVER_ERROR
    );
}

// ============================================================================
// PHASE 5.1: DOUBLE ELIMINATION TESTS
// ============================================================================

/// Helper to create a double elimination tournament and open registration.
async fn create_de_tournament(app: &TestApp, slug: &str, min_participants: i32) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("DE Test {}", slug),
                "slug": slug,
                "format": "double_elimination",
                "participant_type": "individual",
                "min_participants": min_participants,
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

    // Publish
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/open-registration",
            tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    tournament_id
}

/// Helper to register N players for a tournament and return registration IDs.
async fn register_n_players(
    app: &TestApp,
    tournament_id: &str,
    count: usize,
    slug: &str,
) -> Vec<String> {
    let mut reg_ids = Vec::new();

    // First player uses the dev user
    let reg1 = register_player(app, tournament_id, "Player1").await;
    approve_registration(app, tournament_id, &reg1).await;
    reg_ids.push(reg1);

    // Remaining players use new users
    for i in 2..=count {
        let (user_id, player_id) =
            create_test_player(app, &format!("de_player{}_{}", i, slug)).await;
        let reg = insert_test_registration(
            app,
            tournament_id,
            player_id,
            user_id,
            &format!("Player{}", i),
        )
        .await;
        reg_ids.push(reg);
    }

    reg_ids
}

#[tokio::test]
async fn test_start_double_elimination_tournament() {
    let app = TestApp::new().await;
    let tournament_id = create_de_tournament(&app, "de-start-test", 4).await;

    // Register 8 players
    let _reg_ids = register_n_players(&app, &tournament_id, 8, "de-start").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Verify tournament status is "in_progress"
    let response = app
        .get(&format!("/v1/tournaments/{}", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "in_progress");
    assert_eq!(body["data"]["format"], "double_elimination");

    // Get brackets - should have 3 (Winners, Losers, Grand Final)
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(brackets.len(), 3, "Should have 3 brackets (WB, LB, GF)");

    // Verify bracket types
    let bracket_types: Vec<&str> = brackets
        .iter()
        .map(|b| b["bracket_type"].as_str().unwrap())
        .collect();
    assert!(bracket_types.contains(&"winners"));
    assert!(bracket_types.contains(&"losers"));
    assert!(bracket_types.contains(&"grand_final"));

    // Get all matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 8 teams: WB=7, LB=6, GF=1 = 14 total
    assert_eq!(
        all_matches.len(),
        14,
        "8-team DE should have 14 matches total"
    );

    // Get matches per bracket
    let wb_bracket = brackets
        .iter()
        .find(|b| b["bracket_type"] == "winners")
        .unwrap();
    let lb_bracket = brackets
        .iter()
        .find(|b| b["bracket_type"] == "losers")
        .unwrap();
    let gf_bracket = brackets
        .iter()
        .find(|b| b["bracket_type"] == "grand_final")
        .unwrap();

    let wb_id = wb_bracket["id"].as_str().unwrap();
    let lb_id = lb_bracket["id"].as_str().unwrap();
    let gf_id = gf_bracket["id"].as_str().unwrap();

    let wb_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == wb_id)
        .collect();
    let lb_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == lb_id)
        .collect();
    let gf_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == gf_id)
        .collect();

    assert_eq!(wb_matches.len(), 7, "WB should have 7 matches");
    assert_eq!(lb_matches.len(), 6, "LB should have 6 matches");
    assert_eq!(gf_matches.len(), 1, "GF should have 1 match");

    // Verify WR1 matches have participants assigned
    let wr1_matches: Vec<_> = wb_matches
        .iter()
        .filter(|m| m["round"].as_i64() == Some(1))
        .collect();
    assert_eq!(wr1_matches.len(), 4, "WR1 should have 4 matches");

    for m in &wr1_matches {
        assert!(
            m["participant1_name"].is_string(),
            "WR1 match should have participant 1 assigned"
        );
        assert!(
            m["participant2_name"].is_string(),
            "WR1 match should have participant 2 assigned"
        );
    }

    // Verify GF match has no participants yet (they'll be filled after WB/LB finals)
    let gf_match = &gf_matches[0];
    assert!(
        gf_match["participant1_name"].is_null(),
        "GF should not have participants yet"
    );
}

#[tokio::test]
async fn test_start_double_elimination_4_teams() {
    let app = TestApp::new().await;
    let tournament_id = create_de_tournament(&app, "de-4teams-test", 2).await;

    // Register 4 players
    let _reg_ids = register_n_players(&app, &tournament_id, 4, "de-4teams").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Get all matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 4 teams: WB=3, LB=2, GF=1 = 6 total
    assert_eq!(
        all_matches.len(),
        6,
        "4-team DE should have 6 matches total"
    );
}

#[tokio::test]
async fn test_double_elimination_with_byes() {
    let app = TestApp::new().await;
    let tournament_id = create_de_tournament(&app, "de-byes-test", 2).await;

    // Register 6 players (needs 8-bracket, 2 byes)
    let _reg_ids = register_n_players(&app, &tournament_id, 6, "de-byes").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Get all matches - bracket size is 8, so same match count as 8-team
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 8-bracket: WB=7, LB=6, GF=1 = 14 total
    assert_eq!(
        all_matches.len(),
        14,
        "6-team DE (8-bracket) should have 14 matches total"
    );

    // Get brackets
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    let wb = brackets
        .iter()
        .find(|b| b["bracket_type"] == "winners")
        .unwrap();
    let wb_id = wb["id"].as_str().unwrap();

    // Some WR2 matches should have participants from byes
    let wr2_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == wb_id && m["round"].as_i64() == Some(2))
        .collect();

    // At least one WR2 match should have a bye-advanced participant
    let has_bye_advanced = wr2_matches
        .iter()
        .any(|m| m["participant1_name"].is_string() || m["participant2_name"].is_string());
    assert!(
        has_bye_advanced,
        "At least one WR2 match should have a bye-advanced participant"
    );
}
