use super::*;

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
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
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
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].is_null());

    // Create a proposal
    let proposed_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({
                "proposed_times": [proposed_time.to_rfc3339()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Now should have an active proposal
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["id"].is_string());
    assert_eq!(body["data"]["status"], "pending");
}

// (The old test_accept_schedule_proposal admin-scheduled as a workaround for
// both participants being the dev user; the real accept-endpoint test at the
// bottom of this file replaces it, acting as the opponent via a second token.)

#[tokio::test]
async fn test_reject_schedule_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "reject-proposal-test").await;

    // Create a proposal
    let proposed_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
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
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/reject"),
            &json!({
                "proposal_id": proposal_id
            }),
        )
        .await;

    // Should return 403 because you cannot respond to your own proposal
    response.assert_status(StatusCode::FORBIDDEN);

    let body: serde_json::Value = response.json();
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("Cannot respond to your own proposal")
    );
}

#[tokio::test]
async fn test_get_proposal_history() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "proposal-history-test").await;

    // Initially empty history
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/history"
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());

    // Create a proposal
    let proposed_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({
                "proposed_times": [proposed_time.to_rfc3339()]
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Now history should have one entry
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/history"
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
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
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
            &format!("/v1/players/me/availability/windows/{window_id}"),
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
        .delete_auth(&format!("/v1/players/me/availability/windows/{window_id}"))
        .await;

    response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone by trying to delete again (should get 404)
    let response = app
        .delete_auth(&format!("/v1/players/me/availability/windows/{window_id}"))
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
            "/v1/players/me/availability/overrides/{override_id}"
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
            "/v1/players/{player_id}/availability/date?date=2025-01-15"
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
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/suggestions/generate"),
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

/// Generating suggestions writes rows, so only match participants (or
/// tournament admins) may call it. A participant succeeds; an unrelated
/// authenticated user gets 403.
#[tokio::test]
async fn test_generate_suggestions_requires_participant() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "suggestions-authz-test").await;

    let body = json!({
        "start_date": "2025-01-13",
        "end_date": "2025-01-20",
        "min_duration_minutes": 60
    });
    let url = format!("/v1/tournaments/{tournament_id}/matches/{match_id}/suggestions/generate");

    // A random authenticated user who is not in the match is forbidden.
    let (outsider_user, outsider_player) = create_test_player(&app, "suggestions_outsider").await;
    let outsider_token = create_test_token(
        outsider_user,
        outsider_player,
        "suggestions_outsider",
        TEST_JWT_SECRET,
    );
    let response = app.post_json_with_token(&url, &body, &outsider_token).await;
    response.assert_status(StatusCode::FORBIDDEN);

    // A match participant may generate suggestions.
    let response = app.post_json_with_token(&url, &body, &player2_token).await;
    response.assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn test_get_match_suggestions() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "get-suggestions-test").await;

    // Initially should be empty (no auth needed for read)
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/suggestions"
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
// PROPOSAL NEGOTIATION COMPLETION: accept + counter-propose
// ============================================================================

#[tokio::test]
async fn test_accept_schedule_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "accept-proposal-test").await;

    // Dev user (participant 1) proposes two times
    let time1 = chrono::Utc::now() + chrono::Duration::hours(24);
    let time2 = chrono::Utc::now() + chrono::Duration::hours(48);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [time1.to_rfc3339(), time2.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let proposal_id = body["data"]["id"].as_str().unwrap().to_string();
    // Echo the stored time back — the DB truncates sub-microsecond precision
    // and accept requires an exact match against a proposed time.
    let stored_time = body["data"]["proposed_times"][0]
        .as_str()
        .unwrap()
        .to_string();

    // Player 2 (the opponent) accepts one of the proposed times
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
            &json!({
                "proposal_id": proposal_id,
                "selected_time": stored_time
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Acceptance schedules the match at the selected time
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "scheduled");
    assert!(body["data"]["scheduled_at"].is_string());

    // The proposal is no longer active
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].is_null());
}

#[tokio::test]
async fn test_counter_propose_schedule() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "counter-proposal-test").await;

    // Dev user proposes
    let time1 = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [time1.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let original_id = body["data"]["id"].as_str().unwrap().to_string();

    // Player 2 counters with a different time
    let counter_time = chrono::Utc::now() + chrono::Duration::hours(72);
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/counter"),
            &json!({
                "original_proposal_id": original_id,
                "proposed_times": [counter_time.to_rfc3339()]
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let counter_id = body["data"]["id"].as_str().unwrap().to_string();
    assert_ne!(counter_id, original_id);
    assert_eq!(body["data"]["status"], "pending");
    let stored_counter_time = body["data"]["proposed_times"][0]
        .as_str()
        .unwrap()
        .to_string();

    // The counter is now the active proposal
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"].as_str().unwrap(), counter_id);

    // Dev user accepts the counter — negotiation completes
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
            &json!({
                "proposal_id": counter_id,
                "selected_time": stored_counter_time
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "scheduled");
}
