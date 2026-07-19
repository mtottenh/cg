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
        batch_limit: 100,
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

    // Session exists, is past coin flip (in progress), and the match is
    // veto-gated.
    let response = app.get_auth(&format!("/v1/matches/{match_id}/veto")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let session = &body["data"]["session"];
    assert_eq!(session["status"], "in_progress");
    assert!(session["first_action_registration_id"].is_string());

    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["veto_required"], true);
    assert_eq!(body["data"]["status"], "checking_in");

    // Second pass: no duplicate session, no errors.
    let summary2 = run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(summary2.errors, 0);
    assert_eq!(summary2.veto_sessions_created, 0);
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
