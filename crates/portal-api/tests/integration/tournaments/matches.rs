use super::*;

#[tokio::test]
async fn test_get_match_status() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "match-status-test").await;

    // Get match status
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/status"
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
            "/v1/tournaments/{tournament_id}/matches/{match_id}/status-history"
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
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
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
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Check in to the match
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
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
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Forfeit the match
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/forfeit"),
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
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
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
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Get match status history
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/status-history"
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
            "/v1/tournaments/{tournament_id}/matches/00000000-0000-0000-0000-000000000000/status"
        ))
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}
