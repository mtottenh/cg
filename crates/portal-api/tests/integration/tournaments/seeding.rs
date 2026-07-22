use super::*;

#[tokio::test]
async fn test_get_seeding_empty() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "seeding-empty-test").await;

    // Get seeding (should be empty)
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/seeding"))
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
    let reg2 =
        insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Auto-seed with random algorithm
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
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
    let reg2 =
        insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Manual seed with explicit seeding order
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/seeding/manual"),
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
    let reg1_entry = seeded
        .iter()
        .find(|s| s["registration_id"] == reg1)
        .unwrap();
    let reg2_entry = seeded
        .iter()
        .find(|s| s["registration_id"] == reg2)
        .unwrap();

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
            &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Verify seeding is not empty
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/seeding"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);

    // Clear seeding
    let response = app
        .delete_auth(&format!("/v1/tournaments/{tournament_id}/seeding"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify seeding is empty after clearing
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/seeding"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}
