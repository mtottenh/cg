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

// ============================================================================
// P-9: PROPOSER CAN WITHDRAW THEIR OWN PENDING PROPOSAL
// ============================================================================

/// A mistyped proposal used to block the match for the full 48h TTL —
/// `ProposalStatus::Cancelled` was reachable only through `admin_schedule`.
/// The proposer withdraws, the match frees up immediately, and they can
/// post a corrected proposal.
#[tokio::test]
async fn test_proposer_can_cancel_own_pending_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "cancel-proposal-test").await;

    let wrong_time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [wrong_time.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let proposal_id = body["data"]["id"].as_str().unwrap().to_string();

    // A second proposal is blocked while one is pending.
    let other_time = chrono::Utc::now() + chrono::Duration::hours(30);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [other_time.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);

    // The proposer withdraws it.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/cancel"),
            &json!({ "proposal_id": proposal_id }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], proposal_id);
    assert_eq!(body["data"]["status"], "cancelled");

    // No active proposal any more...
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].is_null());

    // ...so a corrected proposal goes through.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [other_time.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // The withdrawn proposal is still in the history (audit trail).
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/history"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let history = body["data"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        history
            .iter()
            .any(|p| p["id"] == proposal_id.as_str() && p["status"] == "cancelled")
    );
}

/// The opponent has accept / reject / counter; they must not be able to
/// withdraw the proposal on the proposer's behalf.
#[tokio::test]
async fn test_opponent_cannot_cancel_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "cancel-proposal-authz").await;

    let time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [time.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let proposal_id = body["data"]["id"].as_str().unwrap().to_string();

    let url = format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/cancel");
    let payload = json!({ "proposal_id": proposal_id });

    // The opponent is forbidden.
    let response = app
        .post_json_with_token(&url, &payload, &player2_token)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // So is an unrelated authenticated user.
    let (outsider_user, outsider_player) = create_test_player(&app, "cancel_outsider").await;
    let outsider_token = create_test_token(
        outsider_user,
        outsider_player,
        "cancel_outsider",
        TEST_JWT_SECRET,
    );
    let response = app
        .post_json_with_token(&url, &payload, &outsider_token)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // And an anonymous caller gets 401.
    let response = app.post_json_no_auth(&url, &payload).await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // The proposal is untouched.
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"].as_str().unwrap(), proposal_id);
    assert_eq!(body["data"]["status"], "pending");
}

/// Only a *pending* proposal can be withdrawn: once the opponent has
/// responded the outcome stands, and a second cancel is rejected too.
#[tokio::test]
async fn test_cannot_cancel_non_pending_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "cancel-proposal-state").await;

    let time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [time.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let proposal_id = body["data"]["id"].as_str().unwrap().to_string();

    // Opponent rejects it.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/reject"),
            &json!({ "proposal_id": proposal_id, "reason": "no good" }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    // The proposer can no longer withdraw a rejected proposal.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/cancel"),
            &json!({ "proposal_id": proposal_id }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

/// A proposal ID that belongs to a different match is not addressable
/// through this match's URL.
#[tokio::test]
async fn test_cancel_proposal_rejects_foreign_match_id() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "cancel-prop-fgn-a").await;
    let (other_tournament_id, other_match_id, _, _) =
        create_tournament_with_matches(&app, "cancel-prop-fgn-b").await;

    let time = chrono::Utc::now() + chrono::Duration::hours(24);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
            &json!({ "proposed_times": [time.to_rfc3339()] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let proposal_id = body["data"]["id"].as_str().unwrap().to_string();

    // Same (authorised) user, but the proposal is not on that match.
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{other_tournament_id}/matches/{other_match_id}/schedule/cancel"
            ),
            &json!({ "proposal_id": proposal_id }),
        )
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

/// P-84: an admin who sets a match time directly must leave an audit trail, and
/// the reason they typed must be part of it.
///
/// `notes` was collected by MatchAdminActionsTab, forwarded by the store, and
/// accepted by `AdminScheduleRequest` — then dropped, because `admin_schedule`
/// did not take it. The status change also went straight to the repository,
/// bypassing the lifecycle service, so the override left NO `match_status_log`
/// row at all. Scheduling drives check-in windows and no-show forfeits (the
/// P-59 attack surface), so an admin override of it is exactly the event that
/// should be auditable.
#[tokio::test]
async fn test_admin_schedule_records_an_audit_row_with_the_admin_notes() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, _reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "p84-audit").await;

    let scheduled_at = chrono::Utc::now() + chrono::Duration::hours(3);
    let notes = "Both captains agreed on a new slot in Discord.";

    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": scheduled_at.to_rfc3339(), "notes": notes }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let row: Option<(String, Option<String>, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT to_status, transition_reason, triggered_by_user_id
         FROM match_status_log
         WHERE match_id = $1
         ORDER BY transitioned_at DESC
         LIMIT 1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .fetch_optional(app.pool())
    .await
    .expect("query the status log");

    let (to_status, reason, actor) =
        row.expect("P-84: an admin schedule must write a match_status_log row");

    assert_eq!(to_status, "scheduled");
    assert_eq!(
        reason.as_deref(),
        Some(notes),
        "P-84: the admin's typed rationale must reach the audit trail, not be discarded"
    );
    assert!(
        actor.is_some(),
        "P-84: the audit row must name the admin who forced the schedule"
    );
}

/// Propose two future slots as the dev user; returns the proposal id.
async fn propose_two_slots(app: &TestApp, tournament_id: &str, match_id: &str) -> String {
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
    body["data"]["id"].as_str().unwrap().to_string()
}

async fn match_status(app: &TestApp, match_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM tournament_matches WHERE id = $1::uuid")
        .bind(match_id)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

/// A proposal outlives its earlier slots. Accepting one that has gone by
/// used to schedule the match in the past, open check-in at once and forfeit
/// whoever was not online.
#[tokio::test]
async fn test_accept_rejects_a_slot_that_has_already_passed() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "accept-past-slot").await;
    let proposal_id = propose_two_slots(&app, &tournament_id, &match_id).await;

    // Time passes: the first slot is now an hour ago.
    sqlx::query(
        "UPDATE schedule_proposals \
         SET proposed_times = ARRAY[NOW() - INTERVAL '1 hour', proposed_times[2]] \
         WHERE id = $1::uuid",
    )
    .bind(&proposal_id)
    .execute(app.pool())
    .await
    .unwrap();
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;
    let body: serde_json::Value = response.json();
    let past = body["data"]["proposed_times"][0]
        .as_str()
        .unwrap()
        .to_string();
    let future = body["data"]["proposed_times"][1]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
            &json!({ "proposal_id": proposal_id, "selected_time": past }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(match_status(&app, &match_id).await, "ready");

    // The proposal is still open, and its remaining slot still works.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
            &json!({ "proposal_id": proposal_id, "selected_time": future }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    assert_eq!(match_status(&app, &match_id).await, "scheduled");

    // The acceptance is on the match's status history.
    let logged: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM match_status_log \
         WHERE match_id = $1::uuid AND from_status = 'ready' AND to_status = 'scheduled'",
    )
    .bind(&match_id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(logged, 1);
}

/// A proposal left pending after its match moved on must not drag the match
/// back to `scheduled`.
#[tokio::test]
async fn test_accept_rejects_a_match_that_is_no_longer_ready() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "accept-not-ready").await;
    let proposal_id = propose_two_slots(&app, &tournament_id, &match_id).await;
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await;
    let body: serde_json::Value = response.json();
    let slot = body["data"]["proposed_times"][0]
        .as_str()
        .unwrap()
        .to_string();

    sqlx::query("UPDATE tournament_matches SET status = 'in_progress' WHERE id = $1::uuid")
        .bind(&match_id)
        .execute(app.pool())
        .await
        .unwrap();

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
            &json!({ "proposal_id": proposal_id, "selected_time": slot }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(match_status(&app, &match_id).await, "in_progress");
}

/// An admin can move a match that already has a time. Rescheduling used to be
/// refused outright: `admin_schedule` only accepted a `ready` match, so once a
/// time was set there was no way to change it.
#[tokio::test]
async fn test_admin_can_reschedule_a_scheduled_match() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "admin-reschedule-test").await;
    let url = format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule");

    let first = chrono::Utc::now() + chrono::Duration::hours(12);
    let response = app
        .post_json(&url, &json!({ "scheduled_at": first.to_rfc3339() }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "scheduled");

    let second = chrono::Utc::now() + chrono::Duration::hours(36);
    let response = app
        .post_json(&url, &json!({ "scheduled_at": second.to_rfc3339() }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "scheduled");

    let (stored,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT scheduled_at FROM tournament_matches WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    let stored = stored.expect("the match keeps a scheduled time");
    assert!(
        (stored - second).num_seconds().abs() < 1,
        "the second time must replace the first: stored {stored}, wanted {second}"
    );
}

/// Marking a match `scheduled` is not the same as scheduling it. The admin
/// transition used to accept `ready -> scheduled` with no time, which left the
/// match waiting for a check-in window that could never open.
#[tokio::test]
async fn test_admin_transition_to_scheduled_requires_a_time() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "transition-needs-time").await;

    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
            &json!({ "to_status": "scheduled", "override_reason": "Admin action" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "ready");
}

// ============================================================================
// ONE PENDING PROPOSAL PER MATCH, ANSWERED ONCE
// ============================================================================

/// The database refuses a second pending proposal on a match. The service's
/// "is one pending?" read cannot: two captains proposing at the same moment
/// both passed it.
#[tokio::test]
async fn test_a_match_cannot_hold_two_pending_proposals() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, _) =
        create_tournament_with_matches_and_opponent(&app, "one-pending").await;
    propose_two_slots(&app, &tournament_id, &match_id).await;

    let duplicate = sqlx::query(
        "INSERT INTO schedule_proposals \
             (match_id, proposed_by_registration_id, proposed_by_user_id, proposed_times, \
              expires_at, status) \
         SELECT match_id, proposed_by_registration_id, proposed_by_user_id, proposed_times, \
                expires_at, 'pending' \
         FROM schedule_proposals WHERE match_id = $1::uuid",
    )
    .bind(&match_id)
    .execute(app.pool())
    .await;
    let err = duplicate.expect_err("a second pending proposal must be refused");
    assert!(
        err.as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_unique_violation),
        "{err}"
    );

    // Through the API the same collision is a conflict, not a server error.
    let time = chrono::Utc::now() + chrono::Duration::hours(30);
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose"),
        &json!({ "proposed_times": [time.to_rfc3339()] }),
    )
    .await
    .assert_status(StatusCode::CONFLICT);
}

/// A counter replaces the proposal it answers: the original is no longer
/// pending (so it cannot be accepted behind the counter's back), the two are
/// linked, and the match still holds exactly one pending proposal.
#[tokio::test]
async fn test_counter_replaces_the_original_proposal() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "counter-replaces").await;
    let original_id = propose_two_slots(&app, &tournament_id, &match_id).await;
    let original: serde_json::Value = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await
        .json();
    let original_slot = original["data"]["proposed_times"][0]
        .as_str()
        .unwrap()
        .to_string();

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
    let counter_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, linked): (String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT status, counter_proposal_id FROM schedule_proposals WHERE id = $1::uuid",
    )
    .bind(&original_id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(status, "counter_proposed");
    assert_eq!(linked.map(|id| id.to_string()), Some(counter_id));

    let (pending,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM schedule_proposals WHERE match_id = $1::uuid AND status = 'pending'",
    )
    .bind(&match_id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(pending, 1);

    // Countering the same proposal again, or accepting it, is refused.
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/counter"),
        &json!({
            "original_proposal_id": original_id,
            "proposed_times": [counter_time.to_rfc3339()]
        }),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
        &json!({ "proposal_id": original_id, "selected_time": original_slot }),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(match_status(&app, &match_id).await, "ready");
}

/// Accepting writes the proposal, the match time, the status and the status
/// log together, against the accepting user.
#[tokio::test]
async fn test_accept_schedules_the_match_and_logs_who_accepted() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "accept-logged").await;
    let proposal_id = propose_two_slots(&app, &tournament_id, &match_id).await;
    let active: serde_json::Value = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await
        .json();
    let slot = active["data"]["proposed_times"][1]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
            &json!({ "proposal_id": proposal_id, "selected_time": slot }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "scheduled");
    assert_eq!(body["data"]["scheduled_at"], slot);

    let (from, to, by_user, system): (String, String, Option<uuid::Uuid>, bool) = sqlx::query_as(
        "SELECT from_status, to_status, triggered_by_user_id, triggered_by_system \
         FROM match_status_log WHERE match_id = $1::uuid ORDER BY transitioned_at DESC LIMIT 1",
    )
    .bind(&match_id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!((from.as_str(), to.as_str()), ("ready", "scheduled"));
    assert!(by_user.is_some() && !system);

    // Answered once: a second accept of the same proposal changes nothing.
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
        &json!({ "proposal_id": proposal_id, "selected_time": slot }),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// CLASHES AND PAST TIMES
// ============================================================================

/// Give the tournament a second match between the same two sides that already
/// has a time, so there is something to clash with.
async fn add_scheduled_sibling_match(
    app: &TestApp,
    match_id: &str,
    at: chrono::DateTime<chrono::Utc>,
) {
    sqlx::query(
        "INSERT INTO tournament_matches \
         SELECT (jsonb_populate_record(m, jsonb_build_object( \
             'id', gen_random_uuid(), 'match_number', 99, 'bracket_position', 'X99', \
             'status', 'scheduled', 'scheduled_at', $2::timestamptz))).* \
         FROM tournament_matches m WHERE m.id = $1::uuid",
    )
    .bind(match_id)
    .bind(at)
    .execute(app.pool())
    .await
    .unwrap();
}

/// A team cannot agree a time within two hours of another of its matches;
/// an admin can set one, and is told about the clash.
#[tokio::test]
async fn test_a_team_cannot_be_double_booked() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "double-booked").await;
    let proposal_id = propose_two_slots(&app, &tournament_id, &match_id).await;
    let active: serde_json::Value = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active"
        ))
        .await
        .json();
    let slot = |i: usize| {
        active["data"]["proposed_times"][i]
            .as_str()
            .unwrap()
            .to_string()
    };
    let first: chrono::DateTime<chrono::Utc> = slot(0).parse().unwrap();

    // The same two teams already play 90 minutes after the first slot.
    add_scheduled_sibling_match(&app, &match_id, first + chrono::Duration::minutes(90)).await;

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept"),
            &json!({ "proposal_id": proposal_id, "selected_time": slot(0) }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        response.text().contains("at least two hours apart"),
        "{}",
        response.text()
    );
    assert_eq!(match_status(&app, &match_id).await, "ready");

    // An admin setting the same time is not refused, but is warned.
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": slot(0) }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body["data"]["schedule_warning"]
            .as_str()
            .is_some_and(|w| w.contains("at least two hours apart")),
        "{body}"
    );

    // A time clear of the other match carries no warning.
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": slot(1) }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].get("schedule_warning").is_none(), "{body}");
}

/// An admin-set time that has already gone by has to be asked for: a mistyped
/// date would otherwise open check-in on the spot.
#[tokio::test]
async fn test_admin_schedule_in_the_past_must_be_confirmed() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "admin-past-time").await;
    let url = format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule");
    let an_hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);

    let response = app
        .post_json(&url, &json!({ "scheduled_at": an_hour_ago.to_rfc3339() }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(match_status(&app, &match_id).await, "ready");

    // "Now" is not the past, even after the request's travel time.
    let just_now = chrono::Utc::now() - chrono::Duration::seconds(20);
    app.post_json(&url, &json!({ "scheduled_at": just_now.to_rfc3339() }))
        .await
        .assert_status(StatusCode::OK);

    app.post_json(
        &url,
        &json!({ "scheduled_at": an_hour_ago.to_rfc3339(), "allow_past": true }),
    )
    .await
    .assert_status(StatusCode::OK);
    assert_eq!(match_status(&app, &match_id).await, "scheduled");
}
