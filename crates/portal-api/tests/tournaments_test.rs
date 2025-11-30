//! Tournament API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;
use sqlx::Row;

/// Helper to get a game's UUID by slug.
async fn get_game_uuid(app: &TestApp, slug: &str) -> String {
    let row = sqlx::query("SELECT id FROM games WHERE slug = $1")
        .bind(slug)
        .fetch_one(app.pool())
        .await
        .expect("Game should exist");
    let id: uuid::Uuid = row.get("id");
    id.to_string()
}

// ============================================================================
// TOURNAMENT CRUD TESTS
// ============================================================================

#[tokio::test]
async fn test_create_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
    let game_id = get_game_uuid(&app, "cs2").await;

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
