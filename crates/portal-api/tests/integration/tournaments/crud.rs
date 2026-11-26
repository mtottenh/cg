use super::*;

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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
    let response = app.get(&format!("/v1/tournaments/{tournament_id}")).await;
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
    let response = app
        .get("/v1/tournaments/00000000-0000-0000-0000-000000000000")
        .await;
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
                    "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
    let response = app.get(&format!("/v1/tournaments?game_id={game_id}")).await;
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
            &format!("/v1/tournaments/{tournament_id}"),
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

#[tokio::test]
async fn test_update_tournament_after_start_rejected() {
    let app = TestApp::new().await;

    // Helper leaves the tournament in_progress (started, matches created)
    let (tournament_id, _match_id, _reg1, _reg2) =
        create_tournament_with_matches(&app, "update-started-test").await;

    // The service rejects any update once the tournament has started —
    // including participant-limit changes.
    let response = app
        .patch_json(
            &format!("/v1/tournaments/{tournament_id}"),
            &json!({ "max_participants": 32 }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Name/description changes are rejected for the same reason.
    let response = app
        .patch_json(
            &format!("/v1/tournaments/{tournament_id}"),
            &json!({ "name": "Too Late Rename" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // The tournament is unchanged.
    let response = app.get(&format!("/v1/tournaments/{tournament_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["max_participants"], 16);
    assert_eq!(body["data"]["status"], "in_progress");
}

/// Prod incident 2026-11-19: moving `registration_end` past the stored
/// `starts_at` violated `tournaments_check_dates` and surfaced as a 500.
/// It must be a field-scoped validation error, and moving both dates
/// together (the actual fix for a late entrant) must succeed.
#[tokio::test]
async fn test_update_tournament_registration_end_after_start() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Dates Test",
                "slug": "dates-test",
                "format": "round_robin",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3",
                "registration_start": "2030-01-01T00:00:00Z",
                "registration_end": "2030-01-05T00:00:00Z",
                "starts_at": "2030-01-05T00:00:00Z"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Only registration_end moves, past the stored starts_at.
    let response = app
        .patch_json(
            &format!("/v1/tournaments/{tournament_id}"),
            &json!({ "registration_end": "2030-01-07T00:00:00Z" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body = response.text();
    assert!(
        body.contains("registration_end"),
        "field-scoped error: {body}"
    );

    // Moving both together is allowed.
    let response = app
        .patch_json(
            &format!("/v1/tournaments/{tournament_id}"),
            &json!({
                "registration_end": "2030-01-07T00:00:00Z",
                "starts_at": "2030-01-08T00:00:00Z"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["registration_end"], "2030-01-07T00:00:00Z");

    // Create is guarded the same way.
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Bad Dates",
                "slug": "bad-dates",
                "format": "round_robin",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3",
                "registration_end": "2030-01-07T00:00:00Z",
                "starts_at": "2030-01-05T00:00:00Z"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

/// The scheduling mode is create-time state everywhere else, but an organiser
/// has to be able to change who sets match times on a cup that is already
/// running. It is the one field a started tournament still accepts, and only
/// on its own.
#[tokio::test]
async fn test_update_tournament_scheduling_mode() {
    let app = TestApp::new().await;

    // Helper leaves the tournament in_progress, created `self_scheduled`.
    let (tournament_id, _match_id, _reg1, _reg2) =
        create_tournament_with_matches(&app, "update-sched-mode-test").await;
    let url = format!("/v1/tournaments/{tournament_id}");

    let response = app.get(&url).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["scheduling_mode"], "self_scheduled");

    let response = app
        .patch_json(&url, &json!({ "scheduling_mode": "live" }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["scheduling_mode"], "live");

    let response = app
        .patch_json(&url, &json!({ "scheduling_mode": "whenever" }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Bundling a frozen field with it is still refused, and changes nothing.
    let response = app
        .patch_json(
            &url,
            &json!({ "scheduling_mode": "self_scheduled", "name": "Too Late Rename" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    let response = app.get(&url).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["scheduling_mode"], "live");
}
