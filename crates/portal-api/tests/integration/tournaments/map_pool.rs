//! Tournament map-pool override tests (GET / PUT / DELETE).

use super::*;

/// Helper to create a draft tournament and grant the dev user the
/// platform-admin role required by the map-pool write endpoints.
async fn create_tournament_for_map_pool(app: &TestApp, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Map Pool Test {}", slug),
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

    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;

    tournament_id
}

#[tokio::test]
async fn test_map_pool_set_get_delete_roundtrip() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_for_map_pool(&app, "map-pool-roundtrip").await;

    // Tournament creation always writes an explicit pool, so the effective
    // pool is the tournament's own from the start.
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/map-pool"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["source"], "tournament");
    assert_eq!(
        body["data"]["maps"],
        json!(portal_test::builders::DEFAULT_CS2_MAP_POOL)
    );

    // PUT a tournament-specific pool
    let response = app
        .put_json(
            &format!("/v1/tournaments/{tournament_id}/map-pool"),
            &json!({ "map_ids": ["de_dust2", "de_mirage", "de_inferno"] }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["source"], "tournament");
    assert_eq!(
        body["data"]["maps"],
        json!(["de_dust2", "de_mirage", "de_inferno"])
    );

    // GET now returns the override
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/map-pool"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["source"], "tournament");
    assert_eq!(
        body["data"]["maps"],
        json!(["de_dust2", "de_mirage", "de_inferno"])
    );

    // DELETE clears the override
    let response = app
        .delete_auth(&format!("/v1/tournaments/{tournament_id}/map-pool"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // GET falls back to the game default
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/map-pool"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["source"], "game");
    assert!(
        !body["data"]["maps"].as_array().unwrap().is_empty(),
        "CS2 game should ship a default map pool"
    );
}

#[tokio::test]
async fn test_set_map_pool_unknown_map_rejected() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_for_map_pool(&app, "map-pool-unknown").await;

    let response = app
        .put_json(
            &format!("/v1/tournaments/{tournament_id}/map-pool"),
            &json!({ "map_ids": ["de_dust2", "de_not_a_real_map"] }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

/// Every tournament is created with an explicit pool, so the first DELETE
/// always succeeds (reverting the tournament to the game default) and only a
/// second DELETE hits the "no override" 404 branch.
#[tokio::test]
async fn test_delete_map_pool_twice_second_is_not_found() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_for_map_pool(&app, "map-pool-no-override").await;

    let response = app
        .delete_auth(&format!("/v1/tournaments/{tournament_id}/map-pool"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    let response = app
        .delete_auth(&format!("/v1/tournaments/{tournament_id}/map-pool"))
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}
