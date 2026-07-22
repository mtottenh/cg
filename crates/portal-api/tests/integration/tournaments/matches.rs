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

// =============================================================================
// P-24 — check-in / forfeit authorization
// =============================================================================

/// The raw check-in columns for a match, read straight from the DB so
/// authorization assertions can't be satisfied by a handler that returns
/// the right status code while still writing.
#[derive(sqlx::FromRow)]
struct MatchCheckInRow {
    status: String,
    participant1_registration_id: Option<Uuid>,
    participant1_checked_in_at: Option<chrono::DateTime<chrono::Utc>>,
    participant2_checked_in_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn read_match_check_in(app: &TestApp, match_id: &str) -> MatchCheckInRow {
    sqlx::query_as::<_, MatchCheckInRow>(
        "SELECT status, participant1_registration_id, participant1_checked_in_at, \
         participant2_checked_in_at FROM tournament_matches WHERE id = $1",
    )
    .bind(Uuid::parse_str(match_id).expect("Invalid match ID"))
    .fetch_one(app.pool())
    .await
    .expect("match should exist")
}

/// P-24: `POST /matches/{id}/check-in` took an `AuthenticatedUser` but no
/// authority check, so *any* logged-in account could check in *any* team.
/// Since both-checked-in auto-advances the match, that let a stranger
/// force someone else's match to start.
#[tokio::test]
async fn test_match_check_in_requires_authority_over_registration() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "checkin-authz").await;

    // Put the match in `scheduled` so check-in is otherwise possible —
    // a 403 here must be about authority, not about match state.
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": scheduled_time.to_rfc3339() }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let url = format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in");
    let payload = json!({ "registration_id": reg2 });

    // An unrelated authenticated user is forbidden. NB: this must be a
    // real JWT — the shared `dev-token` fixture bypasses permission
    // checks entirely, so an attacker built from it would "pass" for
    // the wrong reason.
    let (outsider_user, outsider_player) = create_test_player(&app, "checkin_outsider").await;
    let outsider_token = create_test_token(
        outsider_user,
        outsider_player,
        "checkin_outsider",
        TEST_JWT_SECRET,
    );
    let response = app
        .post_json_with_token(&url, &payload, &outsider_token)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // An anonymous caller gets 401.
    let response = app.post_json_no_auth(&url, &payload).await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // The match is untouched: no status drift, no check-in timestamps.
    let row = read_match_check_in(&app, &match_id).await;
    assert_eq!(
        row.status, "scheduled",
        "a denied check-in must not move the match's status"
    );
    assert!(
        row.participant1_checked_in_at.is_none() && row.participant2_checked_in_at.is_none(),
        "a denied check-in must not record a check-in timestamp"
    );

    // The legitimate participant still gets through (individual
    // tournament: the registered player acts for themself).
    let response = app
        .post_json_with_token(&url, &payload, &player2_token)
        .await;
    response.assert_status(StatusCode::OK);

    let row = read_match_check_in(&app, &match_id).await;
    assert_eq!(row.status, "checking_in");
    let reg2_uuid = Uuid::parse_str(&reg2).unwrap();
    let reg2_is_participant1 = row.participant1_registration_id == Some(reg2_uuid);
    let (checked, other) = if reg2_is_participant1 {
        (
            row.participant1_checked_in_at,
            row.participant2_checked_in_at,
        )
    } else {
        (
            row.participant2_checked_in_at,
            row.participant1_checked_in_at,
        )
    };
    assert!(
        checked.is_some(),
        "the authorized participant's check-in must be recorded"
    );
    assert!(other.is_none(), "the opponent must not be checked in");
}

/// Tournament staff keep an override — the same scoped
/// `tournament.participants.manage` that gates `admin-check-in` next
/// door — so an admin can still unstick a match.
#[tokio::test]
async fn test_tournament_staff_can_check_in_on_behalf() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, reg2, _player2_token) =
        create_tournament_with_matches_and_opponent(&app, "checkin-authz-staff").await;

    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": scheduled_time.to_rfc3339() }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let (staff_user, staff_player) = create_test_player(&app, "checkin_staff").await;
    assign_scoped_role_to_user(
        app.pool(),
        staff_user,
        "tournament_admin",
        portal_core::ScopeType::Tournament,
        Uuid::parse_str(&tournament_id).unwrap(),
    )
    .await;
    let staff_token = create_test_token(staff_user, staff_player, "checkin_staff", TEST_JWT_SECRET);

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
            &json!({ "registration_id": reg2 }),
            &staff_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Forfeit carries the same authority model: conceding on someone
/// else's behalf is at least as damaging as checking them in.
#[tokio::test]
async fn test_match_forfeit_requires_authority_over_registration() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "forfeit-authz").await;

    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": scheduled_time.to_rfc3339() }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let url = format!("/v1/tournaments/{tournament_id}/matches/{match_id}/forfeit");
    let payload = json!({ "registration_id": reg2, "reason": "not my match to give away" });

    let (outsider_user, outsider_player) = create_test_player(&app, "forfeit_outsider").await;
    let outsider_token = create_test_token(
        outsider_user,
        outsider_player,
        "forfeit_outsider",
        TEST_JWT_SECRET,
    );
    let response = app
        .post_json_with_token(&url, &payload, &outsider_token)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    let response = app.post_json_no_auth(&url, &payload).await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    let row = read_match_check_in(&app, &match_id).await;
    assert_eq!(
        row.status, "scheduled",
        "a denied forfeit must not move the match's status"
    );

    // The participant themself may still concede.
    let response = app
        .post_json_with_token(&url, &payload, &player2_token)
        .await;
    response.assert_status(StatusCode::OK);
    let row = read_match_check_in(&app, &match_id).await;
    assert_eq!(row.status, "forfeit");
}
