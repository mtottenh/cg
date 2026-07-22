use super::*;

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
    assert_eq!(created["data"]["status"], "draft");

    // Publish tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
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

    // Publish first
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/open-registration"
        ))
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

    // Create a stage
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/stages"),
            &json!({
                "name": "Group Stage",
                "stage_order": 1,
                "format": "round_robin",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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

    // Create stages
    for i in 1..=2 {
        let response = app
            .post_json(
                &format!("/v1/tournaments/{tournament_id}/stages"),
                &json!({
                    "name": format!("Stage {}", i),
                    "stage_order": i,
                    "format": "single_elimination",
                    "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                }),
            )
            .await;
        response.assert_status(StatusCode::CREATED);
    }

    // Get stages
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/stages"))
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
// LIFECYCLE END: close/reopen registration, cancel, complete, finalize
// ============================================================================

/// Create a tournament via the API and walk it to the given lifecycle stage.
/// Returns the tournament ID.
async fn create_tournament_in_registration(app: &TestApp, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Lifecycle {}", slug),
                "slug": slug,
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    tournament_id
}

#[tokio::test]
async fn test_close_and_reopen_registration() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_in_registration(&app, "close-reopen-test").await;

    // Close registration → scheduled
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/close-registration"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "scheduled");

    // Reopen → back to registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/reopen-registration"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "registration");
}

#[tokio::test]
async fn test_cancel_tournament() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_in_registration(&app, "cancel-test").await;

    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/cancel"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "cancelled");

    // Cancelled is terminal — restarting registration must fail.
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/open-registration"
        ))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_complete_and_finalize_tournament() {
    let app = TestApp::new().await;
    // create_tournament_with_matches drives the tournament to in_progress
    // (registrations approved, seeded, started, bracket generated).
    let (tournament_id, _match_id, _reg1, _reg2) =
        create_tournament_with_matches(&app, "complete-finalize-test").await;

    // Complete
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/complete"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "completed");

    // Finalize
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/finalize"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "finalized");

    // Finalized is terminal — completing again must fail.
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/complete"))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_complete_from_draft_rejected() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Draft Complete Test",
                "slug": "draft-complete-test",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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

    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/complete"))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}
