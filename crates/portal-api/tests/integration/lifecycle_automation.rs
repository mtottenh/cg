//! Lifecycle automation integration tests.
//!
//! Drives `portal_api::background::run_lifecycle_pass` directly against the
//! same database a `TestApp` uses, so each stage of the automation (check-in
//! window opening, veto auto-setup, no-show forfeits, evidence sweeps) is
//! asserted deterministically without waiting on wall-clock intervals.

use crate::common::TestApp;
use crate::tournaments::create_tournament_with_matches;
use axum::http::StatusCode;
use portal_api::background::{LifecycleConfig, run_lifecycle_pass};
use portal_api::state::AppState;
use portal_test::prelude::*;
use serde_json::json;

/// Build an `AppState` sharing the given TestApp's database, for driving
/// background passes directly.
async fn state_for(app: &TestApp) -> AppState {
    AppState::new(app.pool().clone(), "test-jwt-secret").await
}

fn test_config() -> LifecycleConfig {
    LifecycleConfig {
        tick_interval: std::time::Duration::from_secs(30),
        check_in_lead: chrono::Duration::minutes(15),
        check_in_grace: chrono::Duration::minutes(10),
        evidence_stale_max_age: chrono::Duration::hours(24),
        evidence_sweep_every: 20,
        saga_stuck_after: chrono::Duration::minutes(10),
        batch_limit: 100,
        veto_coin_flip_grace: chrono::Duration::seconds(15),
    }
}

/// Admin-schedule the match at the given offset from now.
async fn schedule_at(app: &TestApp, tournament_id: &str, match_id: &str, offset_minutes: i64) {
    let at = chrono::Utc::now() + chrono::Duration::minutes(offset_minutes);
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": at.to_rfc3339(),
                "reason": "lifecycle automation test"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

async fn get_match_status(app: &TestApp, tournament_id: &str, match_id: &str) -> String {
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    body["data"]["status"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_pass_opens_check_in_window_for_due_matches() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "lifecycle-window-test").await;

    // Scheduled 5 minutes out — inside the 15-minute lead window.
    schedule_at(&app, &tournament_id, &match_id, 5).await;
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "scheduled"
    );

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert!(summary.check_in_windows_opened >= 1);

    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "checking_in"
    );

    // Deadline stamped.
    let (deadline,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT check_in_deadline FROM tournament_matches WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert!(deadline.is_some(), "check_in_deadline should be set");
    assert!(deadline.unwrap() > chrono::Utc::now());

    // A second pass is a no-op for this match (no longer scheduled).
    let summary2 = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary2.errors, 0);
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "checking_in"
    );
}

#[tokio::test]
async fn test_pass_ignores_matches_outside_lead_window() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "lifecycle-future-test").await;

    // Scheduled 2 hours out — outside the lead window.
    schedule_at(&app, &tournament_id, &match_id, 120).await;

    run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "scheduled"
    );
}

#[tokio::test]
async fn test_pass_auto_creates_veto_session_when_tournament_configures_format() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;

    // Tournament with a default veto format — the automation's opt-in.
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Lifecycle Veto Test",
                "slug": "lifecycle-veto-test",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 4,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo1",
                "default_map_veto_format": "bo1_standard"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let tournament_id = body["data"]["id"].as_str().unwrap().to_string();

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Two participants, approved + seeded + started (mirrors the shared
    // helper, which we can't use because we need the custom create payload).
    let reg1 = crate::tournaments::register_player(&app, &tournament_id, "P1").await;
    crate::tournaments::approve_registration(&app, &tournament_id, &reg1).await;
    let (u2, p2) = crate::tournaments::create_test_player(&app, "lifecycle_veto_p2").await;
    crate::tournaments::insert_test_registration(&app, &tournament_id, p2, u2, "P2").await;
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/matches"))
        .await;
    let body: serde_json::Value = response.json();
    let match_id = body["data"][0]["id"].as_str().unwrap().to_string();

    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
    schedule_at(&app, &tournament_id, &match_id, 5).await;

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(summary.veto_sessions_created, 1);

    // Session exists but is NOT started: the veto waits for both sides, so
    // no 30-second turn clock runs while the match is still checking in.
    let session_status = |body: &serde_json::Value| body["data"]["session"]["status"].clone();
    let response = app.get_auth(&format!("/v1/matches/{match_id}/veto")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(session_status(&body), "pending");

    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["veto_required"], true);
    assert_eq!(body["data"]["status"], "checking_in");

    // Second pass: no duplicate session, nothing started, no timeouts.
    let summary2 = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary2.errors, 0);
    assert_eq!(summary2.veto_sessions_created, 0);
    assert_eq!(summary2.veto_sessions_started, 0);
    assert_eq!(summary2.veto_timeouts_processed, 0);
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/matches/{match_id}/veto"))
        .await
        .json();
    assert_eq!(session_status(&body), "pending");

    // Setup failing when the window opened (no session, flag still false)
    // used to skip the veto for good; the next pass sets it up again.
    let match_uuid = uuid::Uuid::parse_str(&match_id).unwrap();
    sqlx::query("DELETE FROM veto_sessions WHERE match_id = $1")
        .bind(match_uuid)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE tournament_matches SET veto_required = false WHERE id = $1")
        .bind(match_uuid)
        .execute(app.pool())
        .await
        .unwrap();
    let repaired = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(repaired.errors, 0, "pass should not error: {repaired:?}");
    assert_eq!(repaired.veto_sessions_created, 1);
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/matches/{match_id}/veto"))
        .await
        .json();
    assert_eq!(session_status(&body), "pending");

    // The match reaches pick/ban (an admin override here; both-checked-in
    // takes the same hook): the pre-created session is started.
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
        &json!({
            "to_status": "pick_ban",
            "override_reason": "lifecycle test: both sides are present"
        }),
    )
    .await
    .assert_status(StatusCode::OK);
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/matches/{match_id}/veto"))
        .await
        .json();
    assert_eq!(session_status(&body), "coin_flip");

    // Nobody flips within the grace: the next pass does, and the veto is live.
    sqlx::query(
        "UPDATE veto_sessions SET started_at = NOW() - INTERVAL '1 minute' WHERE match_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    let summary3 = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary3.errors, 0, "pass should not error: {summary3:?}");
    assert_eq!(summary3.veto_coin_flips_auto, 1);
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/matches/{match_id}/veto"))
        .await
        .json();
    assert_eq!(session_status(&body), "in_progress");
    assert!(body["data"]["session"]["first_action_registration_id"].is_string());
}

/// The newest status-log row of a match: (from, to, system-triggered).
async fn last_logged_transition(app: &TestApp, match_id: &str) -> (String, String, bool) {
    sqlx::query_as(
        "SELECT from_status, to_status, triggered_by_system FROM match_status_log \
         WHERE match_id = $1::uuid ORDER BY transitioned_at DESC LIMIT 1",
    )
    .bind(match_id)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn test_pass_forfeits_no_show_after_deadline() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lifecycle-noshow-test").await;

    schedule_at(&app, &tournament_id, &match_id, 5).await;
    run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "checking_in"
    );

    // Only participant 1 (the dev user) checks in.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
            &json!({ "registration_id": reg1 }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Force the deadline into the past.
    sqlx::query(
        "UPDATE tournament_matches SET check_in_deadline = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(summary.no_shows_forfeited, 1);

    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "forfeit"
    );
    assert_eq!(
        last_logged_transition(&app, &match_id).await,
        ("checking_in".into(), "forfeit".into(), true)
    );
}

#[tokio::test]
async fn test_pass_double_forfeits_when_nobody_checks_in() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "lc-doubleff-test").await;

    schedule_at(&app, &tournament_id, &match_id, 5).await;
    run_lifecycle_pass(&state, &test_config(), false).await;

    sqlx::query(
        "UPDATE tournament_matches SET check_in_deadline = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(summary.double_forfeits, 1);

    let status = get_match_status(&app, &tournament_id, &match_id).await;
    assert!(
        status == "cancelled" || status == "forfeit",
        "double no-show should terminate the match, got {status}"
    );
    assert_eq!(
        last_logged_transition(&app, &match_id).await,
        ("checking_in".into(), "cancelled".into(), true)
    );
    // Recorded as two no-shows, not as disqualifications.
    let types: Vec<String> =
        sqlx::query_scalar("SELECT forfeit_type FROM forfeit_records WHERE match_id = $1::uuid")
            .bind(&match_id)
            .fetch_all(app.pool())
            .await
            .unwrap();
    assert_eq!(types, vec!["no_show".to_string(), "no_show".to_string()]);
}

#[tokio::test]
async fn test_evidence_sweep_runs_clean() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;

    let summary = run_lifecycle_pass(&state, &test_config(), true).await;
    assert_eq!(
        summary.errors, 0,
        "evidence sweep should not error: {summary:?}"
    );
}

// ===========================================================================
// RESULT-CLAIM AUTO-CONFIRMATION (launch blocker #8 regression)
// ===========================================================================

/// Transition a match Ready → Scheduled → InProgress via admin endpoints.
async fn transition_to_in_progress(app: &TestApp, tournament_id: &str, match_id: &str) {
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339(),
                "reason": "auto-confirm lifecycle test"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
            &json!({
                "to_status": "in_progress",
                "override_reason": "auto-confirm lifecycle test"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// A pending claim whose `auto_confirm_at` deadline has passed must be
/// auto-confirmed by the lifecycle pass, completing the match and running
/// the completion saga (bracket progression) — exactly as a manual
/// opponent confirm would.
#[tokio::test]
async fn test_pass_auto_confirms_overdue_result_claim_and_progresses() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _reg1, _reg2) =
        create_tournament_with_matches(&app, "lifecycle-auto-confirm").await;

    transition_to_in_progress(&app, &tournament_id, &match_id).await;

    // Dev (registrant of participant 1's registration) claims a 2-0 win
    // for whichever participant sits in slot 1.
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let match_body: serde_json::Value = response.json();
    let p1_reg = match_body["data"]["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1_reg,
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let claim_id = response.json::<serde_json::Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let claim_uuid = uuid::Uuid::parse_str(&claim_id).unwrap();

    // The opponent ignores the claim; its deadline passes.
    sqlx::query(
        "UPDATE result_claims SET auto_confirm_at = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(claim_uuid)
    .execute(app.pool())
    .await
    .unwrap();

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(
        summary.claims_auto_confirmed, 1,
        "the overdue claim must be auto-confirmed: {summary:?}"
    );

    // Claim confirmed, flagged as auto.
    let (status, was_auto): (String, bool) =
        sqlx::query_as("SELECT status, was_auto_confirmed FROM result_claims WHERE id = $1")
            .bind(claim_uuid)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(status, "confirmed");
    assert!(was_auto, "claim must be flagged as auto-confirmed");

    // Match completed with the claimed winner recorded.
    let (match_status, winner): (String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT status, winner_registration_id FROM tournament_matches WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(match_status, "completed");
    assert_eq!(winner, Some(uuid::Uuid::parse_str(&p1_reg).unwrap()));
    assert_eq!(
        last_logged_transition(&app, &match_id).await,
        ("awaiting_result".into(), "completed".into(), true)
    );

    // The completion saga ran (bracket progression happened).
    let (sagas,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM saga_executions
         WHERE match_id = $1 AND saga_type = 'match_completion' AND status = 'completed'",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(sagas, 1, "completion saga must have run exactly once");

    // A second pass is a no-op: nothing left to confirm.
    let summary2 = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary2.claims_auto_confirmed, 0);
    assert_eq!(summary2.errors, 0);
}

/// Claims whose deadline has not passed are left alone.
#[tokio::test]
async fn test_pass_leaves_unexpired_claims_pending() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _reg1, _reg2) =
        create_tournament_with_matches(&app, "lifecycle-claim-pending").await;

    transition_to_in_progress(&app, &tournament_id, &match_id).await;

    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let match_body: serde_json::Value = response.json();
    let p1_reg = match_body["data"]["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1_reg,
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let claim_id = response.json::<serde_json::Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.claims_auto_confirmed, 0);

    let (status,): (String,) = sqlx::query_as("SELECT status FROM result_claims WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&claim_id).unwrap())
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(
        status, "pending",
        "claim inside its window must stay pending"
    );
}

/// Open the check-in window, then push its deadline into the past.
async fn open_window_then_expire(
    app: &TestApp,
    state: &AppState,
    slug: &str,
) -> (String, String, String) {
    let (tournament_id, match_id, reg1, _reg2) = create_tournament_with_matches(app, slug).await;
    schedule_at(app, &tournament_id, &match_id, 5).await;
    run_lifecycle_pass(state, &test_config(), false).await;
    assert_eq!(
        get_match_status(app, &tournament_id, &match_id).await,
        "checking_in"
    );
    sqlx::query(
        "UPDATE tournament_matches SET check_in_deadline = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    (tournament_id, match_id, reg1)
}

/// A deploy or crash that spans a check-in deadline must not forfeit teams
/// that had no site to check in on: at startup, windows that have already
/// run out get a fresh grace period before the first pass.
#[tokio::test]
async fn test_startup_extends_check_in_deadlines_that_passed_while_down() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _reg1) =
        open_window_then_expire(&app, &state, "lc-restart-grace").await;

    let extended =
        portal_api::background::extend_check_in_deadlines_after_restart(&state, &test_config())
            .await;
    assert_eq!(extended, 1);

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(summary.double_forfeits, 0);
    assert_eq!(summary.no_shows_forfeited, 0);
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "checking_in"
    );
}

async fn mark_both_checked_in(app: &TestApp, match_id: &str) {
    sqlx::query(
        "UPDATE tournament_matches \
         SET participant1_checked_in_at = NOW(), participant2_checked_in_at = NOW() \
         WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
}

/// Both sides checked in but the advance never landed: the pass used to log
/// a warning every tick forever. It now finishes the advance — and never
/// forfeits anyone.
#[tokio::test]
async fn test_pass_advances_a_match_stuck_in_checking_in_with_both_sides_in() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _reg1) =
        open_window_then_expire(&app, &state, "lc-both-in-pass").await;
    mark_both_checked_in(&app, &match_id).await;

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(summary.double_forfeits, 0);
    assert_eq!(summary.no_shows_forfeited, 0);
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "in_progress"
    );
}

/// Same stuck state, repaired by a captain pressing Check In again: it used
/// to return "already checked in" without advancing.
#[tokio::test]
async fn test_repeat_check_in_advances_a_match_with_both_sides_in() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, reg1) =
        open_window_then_expire(&app, &state, "lc-both-in-retry").await;
    mark_both_checked_in(&app, &match_id).await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
            &json!({ "registration_id": reg1 }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "in_progress");
}

/// A match that is `scheduled` with no time is invisible to the check-in
/// sweep. Matches reached that state through the old bare admin transition;
/// an admin setting the time must bring them back into the automation.
#[tokio::test]
async fn test_admin_can_set_a_time_on_a_scheduled_match_that_has_none() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "lifecycle-stranded-test").await;

    sqlx::query(
        "UPDATE tournament_matches SET status = 'scheduled', scheduled_at = NULL WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    // Nothing to key off: the pass leaves the match where it is.
    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "scheduled"
    );

    // 5 minutes out is inside the 15-minute lead window.
    schedule_at(&app, &tournament_id, &match_id, 5).await;
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "scheduled"
    );

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary.errors, 0, "pass should not error: {summary:?}");
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "checking_in"
    );
}

/// A match whose time is already past when it is set still gets a whole
/// check-in window: lead plus grace from the moment the window opens, not a
/// few minutes before a no-show forfeit.
#[tokio::test]
async fn test_a_match_scheduled_in_the_past_gets_a_full_check_in_window() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "lc-past-full-window").await;

    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339(),
                "allow_past": true
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let cfg = test_config();
    run_lifecycle_pass(&state, &cfg, false).await;
    assert_eq!(
        get_match_status(&app, &tournament_id, &match_id).await,
        "checking_in"
    );
    let (deadline,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT check_in_deadline FROM tournament_matches WHERE id = $1::uuid")
            .bind(&match_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    let left = deadline.expect("deadline stamped") - chrono::Utc::now();
    let full = cfg.check_in_lead + cfg.check_in_grace;
    assert!(
        left > full - chrono::Duration::minutes(1) && left <= full,
        "expected about {full} left, got {left}"
    );
}

/// "Check in" is only asked of someone while they can do it: the item
/// appears when the window opens, carrying the deadline and the match time,
/// not from the moment a time is agreed.
#[tokio::test]
async fn test_check_in_action_item_appears_only_while_the_window_is_open() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _, _) =
        create_tournament_with_matches(&app, "lc-checkin-item").await;

    let check_in_items = || async {
        let body: serde_json::Value = app.get_auth("/v1/users/me/action-items").await.json();
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["action_type"] == "check_in" && i["match_id"] == match_id.as_str())
            .cloned()
            .collect::<Vec<_>>()
    };

    // A day out: scheduled, nothing to do yet.
    schedule_at(&app, &tournament_id, &match_id, 24 * 60).await;
    run_lifecycle_pass(&state, &test_config(), false).await;
    assert!(check_in_items().await.is_empty());

    // Inside the lead: the window opens and the item appears.
    sqlx::query(
        "UPDATE tournament_matches SET scheduled_at = NOW() + INTERVAL '5 minutes' \
         WHERE id = $1::uuid",
    )
    .bind(&match_id)
    .execute(app.pool())
    .await
    .unwrap();
    run_lifecycle_pass(&state, &test_config(), false).await;
    let items = check_in_items().await;
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(items[0]["deadline"].is_string(), "{items:?}");
    assert!(items[0]["scheduled_at"].is_string(), "{items:?}");
}
