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
