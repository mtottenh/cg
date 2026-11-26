//! Admin match rescue: the operations that get a stuck match moving again.
//!
//! Each test puts a match into a state the normal flow cannot leave — a
//! no-show forfeit, a double no-show cancellation, an expired check-in
//! window, a half-run veto, a server holding a stale config — and asserts
//! that the rescue endpoint both reports success and leaves the match
//! genuinely playable again (standings, check-in, veto and the game server
//! all behave as they would for a fresh match).

use crate::common::TestApp;
use crate::common::agent::{FakeAgent, Script, wait_for};
use crate::tournaments::{
    create_test_player, create_tournament_with_matches,
    create_tournament_with_matches_and_opponent, insert_test_registration,
};
use axum::http::StatusCode;
use portal_api::background::{LifecycleConfig, run_lifecycle_pass};
use portal_api::state::AppState;
use portal_test::prelude::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

// =============================================================================
// Helpers
// =============================================================================

async fn state_for(app: &TestApp) -> AppState {
    AppState::new(app.pool().clone(), "test-jwt-secret").await
}

fn lifecycle_config() -> LifecycleConfig {
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

fn uuid_of(id: &str) -> Uuid {
    Uuid::parse_str(id).unwrap()
}

fn admin_url(tournament_id: &str, match_id: &str, action: &str) -> String {
    format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/{action}")
}

async fn get_match(app: &TestApp, tournament_id: &str, match_id: &str) -> Value {
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    response.json::<Value>()["data"].clone()
}

async fn match_status(app: &TestApp, tournament_id: &str, match_id: &str) -> String {
    get_match(app, tournament_id, match_id).await["status"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn diagnostics(app: &TestApp, tournament_id: &str, match_id: &str) -> Value {
    let response = app
        .get_auth(&admin_url(tournament_id, match_id, "diagnostics"))
        .await;
    response.assert_status(StatusCode::OK);
    response.json::<Value>()["data"].clone()
}

fn problem_codes(diagnostics: &Value) -> Vec<String> {
    diagnostics["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["code"].as_str().unwrap().to_string())
        .collect()
}

fn suggested_action(diagnostics: &Value, code: &str) -> String {
    diagnostics["problems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["code"] == code)
        .unwrap_or_else(|| panic!("no problem {code} in {diagnostics}"))["suggested_action"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn schedule_in(app: &TestApp, tournament_id: &str, match_id: &str, minutes: i64) {
    let at = chrono::Utc::now() + chrono::Duration::minutes(minutes);
    app.post_json(
        &admin_url(tournament_id, match_id, "schedule"),
        &json!({ "scheduled_at": at.to_rfc3339(), "notes": "admin rescue test" }),
    )
    .await
    .assert_status(StatusCode::OK);
}

async fn transition(app: &TestApp, tournament_id: &str, match_id: &str, to: &str) {
    app.post_json(
        &admin_url(tournament_id, match_id, "transition"),
        &json!({ "to_status": to, "override_reason": "admin rescue test setup" }),
    )
    .await
    .assert_status(StatusCode::OK);
}

async fn reset(app: &TestApp, tournament_id: &str, match_id: &str) -> crate::common::TestResponse {
    app.post_json(
        &admin_url(tournament_id, match_id, "reset"),
        &json!({ "reason": "teams were present, replaying" }),
    )
    .await
}

async fn expire_check_in(app: &TestApp, match_id: &str) {
    sqlx::query(
        "UPDATE tournament_matches SET check_in_deadline = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(uuid_of(match_id))
    .execute(app.pool())
    .await
    .unwrap();
}

/// A started round robin of individuals, with a token per registration.
struct Cup {
    tournament_id: String,
    tokens: HashMap<String, String>,
}

async fn round_robin(app: &TestApp, slug: &str, players: usize, extra: Value) -> Cup {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let mut body = json!({
        "game_id": game_id,
        "name": format!("Rescue {slug}"),
        "slug": slug,
        "format": "round_robin",
        "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
        "participant_type": "individual",
        "min_participants": 2,
        "max_participants": 16,
        "registration_type": "open",
        "scheduling_mode": "self_scheduled",
        "default_match_format": "bo1"
    });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        body[key] = value;
    }
    let response = app.post_json("/v1/tournaments", &body).await;
    response.assert_status(StatusCode::CREATED);
    let tournament_id = response.json::<Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    let mut tokens = HashMap::new();
    for i in 1..=players {
        let username = format!("{}_{i}", slug.replace('-', "_"));
        let (user_id, player_id) = create_test_player(app, &username).await;
        let reg =
            insert_test_registration(app, &tournament_id, player_id, user_id, &username).await;
        tokens.insert(
            reg,
            create_test_token(user_id, player_id, &username, TEST_JWT_SECRET),
        );
    }
    assign_role_to_user(
        app.pool(),
        get_dev_user_id(app.pool()).await,
        "platform_admin",
    )
    .await;
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    Cup {
        tournament_id,
        tokens,
    }
}

async fn cup_matches(app: &TestApp, tournament_id: &str) -> Vec<Value> {
    app.get(&format!(
        "/v1/tournaments/{tournament_id}/matches?per_page=100"
    ))
    .await
    .json::<Value>()["data"]
        .as_array()
        .unwrap()
        .clone()
}

async fn standings_row(app: &TestApp, tournament_id: &str, bracket_id: &str, reg: &str) -> Value {
    app.get(&format!(
        "/v1/tournaments/{tournament_id}/brackets/{bracket_id}/standings"
    ))
    .await
    .json::<Value>()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["registration_id"] == reg)
        .unwrap_or_else(|| panic!("no standings row for {reg}"))
        .clone()
}

fn str_of(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

// =============================================================================
// Reset
// =============================================================================

/// The launch-night scenario: both teams are in voice, nobody clicks Check
/// In, the lifecycle pass forfeits one match and calls another off. Both
/// must come back to `ready` with the table as it was, and then go through
/// check-in again for real.
#[tokio::test]
async fn reset_reopens_no_show_forfeit_and_double_no_show() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let cup = round_robin(&app, "rescue-noshow", 3, json!({})).await;
    let tid = &cup.tournament_id;
    let matches = cup_matches(&app, tid).await;
    assert_eq!(matches.len(), 3);
    let forfeited = &matches[0];
    let called_off = &matches[1];
    let forfeited_id = str_of(forfeited, "id");
    let called_off_id = str_of(called_off, "id");
    let bracket_id = str_of(forfeited, "bracket_id");
    let present = str_of(forfeited, "participant1_registration_id");

    // One match: participant 1 checked in, the other did not. The other
    // match: nobody did. Both windows have expired.
    sqlx::query(
        "UPDATE tournament_matches SET status = 'checking_in',
                scheduled_at = NOW() - INTERVAL '20 minutes',
                participant1_checked_in_at = NOW() - INTERVAL '5 minutes',
                check_in_deadline = NOW() - INTERVAL '1 minute'
         WHERE id = $1",
    )
    .bind(uuid_of(&forfeited_id))
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE tournament_matches SET status = 'checking_in',
                scheduled_at = NOW() - INTERVAL '20 minutes',
                check_in_deadline = NOW() - INTERVAL '1 minute'
         WHERE id = $1",
    )
    .bind(uuid_of(&called_off_id))
    .execute(app.pool())
    .await
    .unwrap();

    let summary = run_lifecycle_pass(&state, &lifecycle_config(), false).await;
    assert_eq!(summary.no_shows_forfeited, 1, "{summary:?}");
    assert_eq!(summary.double_forfeits, 1, "{summary:?}");
    assert_eq!(match_status(&app, tid, &forfeited_id).await, "forfeit");
    assert_eq!(match_status(&app, tid, &called_off_id).await, "cancelled");
    let row = standings_row(&app, tid, &bracket_id, &present).await;
    assert_eq!(row["points"], 3, "the walkover counted: {row}");

    // The diagnosis names both, and points at reset.
    let d = diagnostics(&app, tid, &forfeited_id).await;
    assert_eq!(suggested_action(&d, "forfeit_by_no_show"), "reset");
    let d = diagnostics(&app, tid, &called_off_id).await;
    assert_eq!(suggested_action(&d, "cancelled_by_no_show"), "reset");

    // Reset the forfeit: ready, no time, no check-ins, no result, and the
    // walkover is out of the table.
    let response = reset(&app, tid, &forfeited_id).await;
    response.assert_status(StatusCode::OK);
    let m = response.json::<Value>()["data"].clone();
    assert_eq!(m["status"], "ready");
    assert!(m.get("scheduled_at").is_none() || m["scheduled_at"].is_null());
    assert!(m.get("winner_registration_id").is_none() || m["winner_registration_id"].is_null());
    assert!(
        m.get("participant1_checked_in_at").is_none() || m["participant1_checked_in_at"].is_null()
    );
    assert_eq!(m["participant1_score"], 0);
    assert_eq!(m["participant2_score"], 0);
    let row = standings_row(&app, tid, &bracket_id, &present).await;
    assert_eq!(row["points"], 0, "the walkover is gone: {row}");
    assert_eq!(row["matches_played"], 0);
    let (active_forfeits,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM forfeit_records WHERE match_id = $1 AND superseded_at IS NULL",
    )
    .bind(uuid_of(&forfeited_id))
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(active_forfeits, 0);

    // Reset the double no-show too.
    let response = reset(&app, tid, &called_off_id).await;
    response.assert_status(StatusCode::OK);
    assert_eq!(response.json::<Value>()["data"]["status"], "ready");

    // The reset is on the match's history, with the admin's reason.
    let history: Value = app
        .get(&format!(
            "/v1/tournaments/{tid}/matches/{forfeited_id}/status-history"
        ))
        .await
        .json();
    let last = history["data"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["from_status"], "forfeit");
    assert_eq!(last["to_status"], "ready");
    assert!(
        last["transition_reason"]
            .as_str()
            .unwrap()
            .contains("teams were present"),
        "{last}"
    );

    // And the reopened match is genuinely playable: open check-in, check
    // both sides in (an admin, on their behalf), and it starts.
    let deadline = chrono::Utc::now() + chrono::Duration::minutes(30);
    let response = app
        .post_json(
            &admin_url(tid, &forfeited_id, "check-in-window"),
            &json!({ "deadline": deadline.to_rfc3339(), "reason": "teams are ready now" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let m = response.json::<Value>()["data"].clone();
    assert_eq!(m["status"], "checking_in");
    assert!(m["scheduled_at"].is_string(), "a time was filled in: {m}");
    assert!(m["check_in_deadline"].is_string(), "deadline exposed: {m}");

    for key in [
        "participant1_registration_id",
        "participant2_registration_id",
    ] {
        app.post_json(
            &format!("/v1/tournaments/{tid}/matches/{forfeited_id}/check-in"),
            &json!({ "registration_id": str_of(forfeited, key) }),
        )
        .await
        .assert_status(StatusCode::OK);
    }
    assert_eq!(match_status(&app, tid, &forfeited_id).await, "in_progress");

    // A later pass leaves the rescued matches alone.
    let summary = run_lifecycle_pass(&state, &lifecycle_config(), false).await;
    assert_eq!(summary.no_shows_forfeited, 0, "{summary:?}");
    assert_eq!(summary.double_forfeits, 0, "{summary:?}");
    assert_eq!(match_status(&app, tid, &called_off_id).await, "ready");
}

/// Reset works from every pre-result status, clears what that status had
/// accumulated, and refuses the statuses it must not touch.
#[tokio::test]
async fn reset_from_each_pre_result_status() {
    let app = TestApp::new().await;
    let (tid, mid, reg1, _reg2) = create_tournament_with_matches(&app, "rescue-reset-each").await;

    // ready: nothing to do.
    reset(&app, &tid, &mid)
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // scheduled
    schedule_in(&app, &tid, &mid, 600).await;
    assert_eq!(match_status(&app, &tid, &mid).await, "scheduled");
    reset(&app, &tid, &mid).await.assert_status(StatusCode::OK);
    let m = get_match(&app, &tid, &mid).await;
    assert_eq!(m["status"], "ready");
    assert!(m.get("scheduled_at").is_none() || m["scheduled_at"].is_null());

    // checking_in, with one side already in
    schedule_in(&app, &tid, &mid, 600).await;
    transition(&app, &tid, &mid, "checking_in").await;
    app.post_json(
        &format!("/v1/tournaments/{tid}/matches/{mid}/check-in"),
        &json!({ "registration_id": reg1 }),
    )
    .await
    .assert_status(StatusCode::OK);
    reset(&app, &tid, &mid).await.assert_status(StatusCode::OK);
    let m = get_match(&app, &tid, &mid).await;
    assert_eq!(m["status"], "ready");
    assert!(
        m.get("participant1_checked_in_at").is_none() || m["participant1_checked_in_at"].is_null()
    );

    // pick_ban
    schedule_in(&app, &tid, &mid, 600).await;
    transition(&app, &tid, &mid, "pick_ban").await;
    reset(&app, &tid, &mid).await.assert_status(StatusCode::OK);
    assert_eq!(match_status(&app, &tid, &mid).await, "ready");

    // in_progress, with a result claim pending: the claim is superseded.
    schedule_in(&app, &tid, &mid, 600).await;
    transition(&app, &tid, &mid, "in_progress").await;
    let (s1, s2) =
        if get_match(&app, &tid, &mid).await["participant1_registration_id"] == reg1.as_str() {
            (2, 0)
        } else {
            (0, 2)
        };
    app.post_json(
        &format!("/v1/matches/{mid}/result"),
        &json!({
            "claimed_winner_registration_id": reg1,
            "participant1_score": s1,
            "participant2_score": s2,
            "game_results": [],
            "evidence_ids": []
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    assert_eq!(match_status(&app, &tid, &mid).await, "awaiting_result");
    reset(&app, &tid, &mid).await.assert_status(StatusCode::OK);
    assert_eq!(match_status(&app, &tid, &mid).await, "ready");
    let (pending,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM result_claims WHERE match_id = $1 AND status = 'pending'",
    )
    .bind(uuid_of(&mid))
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(pending, 0, "the pending claim must not survive the reset");

    // The reset match schedules again like any other.
    schedule_in(&app, &tid, &mid, 600).await;
    assert_eq!(match_status(&app, &tid, &mid).await, "scheduled");

    // A reason is required.
    app.post_json(&admin_url(&tid, &mid, "reset"), &json!({ "reason": "  " }))
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // A completed match is refused: that is what score correction is for.
    sqlx::query("UPDATE tournament_matches SET status = 'completed' WHERE id = $1")
        .bind(uuid_of(&mid))
        .execute(app.pool())
        .await
        .unwrap();
    let response = reset(&app, &tid, &mid).await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(response.text().contains("completed"), "{}", response.text());
}

// =============================================================================
// Check-in window
// =============================================================================

/// An expired window that has not been swept yet can be extended, and the
/// next lifecycle pass then forfeits nobody.
#[tokio::test]
async fn extending_an_expired_check_in_window_prevents_the_forfeit() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tid, mid, reg1, reg2) = create_tournament_with_matches(&app, "rescue-extend").await;

    schedule_in(&app, &tid, &mid, 5).await;
    run_lifecycle_pass(&state, &lifecycle_config(), false).await;
    assert_eq!(match_status(&app, &tid, &mid).await, "checking_in");
    app.post_json(
        &format!("/v1/tournaments/{tid}/matches/{mid}/check-in"),
        &json!({ "registration_id": reg1 }),
    )
    .await
    .assert_status(StatusCode::OK);

    expire_check_in(&app, &mid).await;
    let d = diagnostics(&app, &tid, &mid).await;
    assert_eq!(suggested_action(&d, "checkin_expired"), "extend_checkin");

    // A deadline in the past is refused.
    let past = chrono::Utc::now() - chrono::Duration::minutes(1);
    app.post_json(
        &admin_url(&tid, &mid, "check-in-window"),
        &json!({ "deadline": past.to_rfc3339(), "reason": "typo" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    let deadline = chrono::Utc::now() + chrono::Duration::minutes(30);
    let response = app
        .post_json(
            &admin_url(&tid, &mid, "check-in-window"),
            &json!({ "deadline": deadline.to_rfc3339(), "reason": "captain's PC crashed" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let m = response.json::<Value>()["data"].clone();
    assert_eq!(m["status"], "checking_in");
    // Seeding is random, so the side that checked in may be either slot.
    assert!(
        m["participant1_checked_in_at"].is_string() || m["participant2_checked_in_at"].is_string(),
        "the check-in already made is kept: {m}"
    );

    let summary = run_lifecycle_pass(&state, &lifecycle_config(), false).await;
    assert_eq!(summary.errors, 0, "{summary:?}");
    assert_eq!(summary.no_shows_forfeited, 0, "{summary:?}");
    assert_eq!(summary.double_forfeits, 0, "{summary:?}");
    assert_eq!(match_status(&app, &tid, &mid).await, "checking_in");
    assert!(
        !problem_codes(&diagnostics(&app, &tid, &mid).await).contains(&"checkin_expired".into())
    );

    // The late side checks in inside the extended window and the match starts.
    app.post_json(
        &format!("/v1/tournaments/{tid}/matches/{mid}/check-in"),
        &json!({ "registration_id": reg2 }),
    )
    .await
    .assert_status(StatusCode::OK);
    assert_eq!(match_status(&app, &tid, &mid).await, "in_progress");

    // Past check-in, the window can no longer be set.
    app.post_json(
        &admin_url(&tid, &mid, "check-in-window"),
        &json!({ "deadline": deadline.to_rfc3339(), "reason": "too late" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

/// Opening check-in straight from `ready` sets a time, stamps the deadline
/// and, when the tournament has a veto format, makes the match veto-gated —
/// exactly what the lifecycle pass does when it opens a window.
#[tokio::test]
async fn check_in_window_opens_from_ready_and_sets_up_the_veto() {
    let app = TestApp::new().await;
    let cup = round_robin(
        &app,
        "rescue-open-ready",
        2,
        json!({ "default_map_veto_format": "bo1_standard" }),
    )
    .await;
    let tid = &cup.tournament_id;
    let m = cup_matches(&app, tid).await[0].clone();
    let mid = str_of(&m, "id");
    assert_eq!(m["status"], "ready");

    let deadline = chrono::Utc::now() + chrono::Duration::minutes(20);
    let response = app
        .post_json(
            &admin_url(tid, &mid, "check-in-window"),
            &json!({ "deadline": deadline.to_rfc3339(), "reason": "playing right now" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let m2 = response.json::<Value>()["data"].clone();
    assert_eq!(m2["status"], "checking_in");
    assert!(m2["scheduled_at"].is_string());
    assert_eq!(m2["veto_required"], true, "veto session pre-created: {m2}");
    let veto: Value = app
        .get_auth(&format!("/v1/matches/{mid}/veto"))
        .await
        .json();
    assert_eq!(veto["data"]["session"]["status"], "pending");

    // Both check in (the players themselves): on to pick/ban.
    for key in [
        "participant1_registration_id",
        "participant2_registration_id",
    ] {
        let reg = str_of(&m, key);
        app.post_json_with_token(
            &format!("/v1/tournaments/{tid}/matches/{mid}/check-in"),
            &json!({ "registration_id": reg }),
            &cup.tokens[&reg],
        )
        .await
        .assert_status(StatusCode::OK);
    }
    assert_eq!(match_status(&app, tid, &mid).await, "pick_ban");
    let veto: Value = app
        .get_auth(&format!("/v1/matches/{mid}/veto"))
        .await
        .json();
    assert_eq!(veto["data"]["session"]["status"], "coin_flip");

    // Resetting a match in pick/ban takes its veto with it, so the replay
    // gets a fresh one rather than the old picks.
    reset(&app, tid, &mid).await.assert_status(StatusCode::OK);
    let m3 = get_match(&app, tid, &mid).await;
    assert_eq!(m3["status"], "ready");
    assert_eq!(m3["veto_required"], false);
    app.get_auth(&format!("/v1/matches/{mid}/veto"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

// =============================================================================
// Veto restart
// =============================================================================

/// A veto that has gone wrong mid-way is thrown away and a fresh one runs to
/// completion.
#[tokio::test]
async fn veto_reset_mid_veto_then_a_full_veto_completes() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let cup = round_robin(
        &app,
        "rescue-veto-reset",
        2,
        json!({ "default_map_veto_format": "bo1_standard" }),
    )
    .await;
    let tid = &cup.tournament_id;
    let mid = str_of(&cup_matches(&app, tid).await[0], "id");

    // Open check-in (which sets the veto up) and move on to pick/ban, as
    // both captains checking in would.
    let deadline = chrono::Utc::now() + chrono::Duration::minutes(30);
    app.post_json(
        &admin_url(tid, &mid, "check-in-window"),
        &json!({ "deadline": deadline.to_rfc3339(), "reason": "veto restart test" }),
    )
    .await
    .assert_status(StatusCode::OK);
    transition(&app, tid, &mid, "pick_ban").await;

    // Let the automation flip the coin and time out one turn, so the veto
    // is genuinely part-way through.
    let advance = async || {
        sqlx::query(
            "UPDATE veto_sessions
             SET started_at = NOW() - INTERVAL '2 minutes',
                 action_deadline = CASE WHEN action_deadline IS NULL THEN NULL
                                        ELSE NOW() - INTERVAL '1 minute' END
             WHERE match_id = $1",
        )
        .bind(uuid_of(&mid))
        .execute(app.pool())
        .await
        .unwrap();
        run_lifecycle_pass(&state, &lifecycle_config(), false).await
    };
    advance().await; // coin flip
    advance().await; // first auto-ban
    let veto: Value = app
        .get_auth(&format!("/v1/matches/{mid}/veto"))
        .await
        .json();
    assert_eq!(veto["data"]["session"]["status"], "in_progress");
    let old_session = str_of(&veto["data"]["session"], "id");
    assert!(
        !veto["data"]["actions"].as_array().unwrap().is_empty(),
        "an action was performed: {veto}"
    );

    let response = app
        .post_json(
            &admin_url(tid, &mid, "veto/reset"),
            &json!({ "reason": "wrong captain was acting" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let m = response.json::<Value>()["data"].clone();
    assert_eq!(m["status"], "pick_ban");
    assert_eq!(m["veto_required"], true);

    let veto: Value = app
        .get_auth(&format!("/v1/matches/{mid}/veto"))
        .await
        .json();
    assert_ne!(str_of(&veto["data"]["session"], "id"), old_session);
    assert_eq!(veto["data"]["session"]["status"], "coin_flip");
    assert!(veto["data"]["actions"].as_array().unwrap().is_empty());

    // The fresh veto runs to the end.
    let mut status = String::new();
    for _ in 0..12 {
        advance().await;
        let veto: Value = app
            .get_auth(&format!("/v1/matches/{mid}/veto"))
            .await
            .json();
        status = str_of(&veto["data"]["session"], "status");
        if status == "completed" {
            assert_eq!(
                veto["data"]["session"]["selected_maps"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            break;
        }
    }
    assert_eq!(status, "completed", "the restarted veto must finish");

    // With a result reported, the veto is no longer what needs restarting.
    transition(&app, tid, &mid, "in_progress").await;
    let winner = cup.tokens.keys().next().unwrap().clone();
    let m = get_match(&app, tid, &mid).await;
    let (s1, s2) = if m["participant1_registration_id"] == winner.as_str() {
        (1, 0)
    } else {
        (0, 1)
    };
    app.post_json_with_token(
        &format!("/v1/matches/{mid}/result"),
        &json!({
            "claimed_winner_registration_id": winner,
            "participant1_score": s1,
            "participant2_score": s2,
            "game_results": [],
            "evidence_ids": []
        }),
        &cup.tokens[&winner],
    )
    .await
    .assert_status(StatusCode::CREATED);
    let response = app
        .post_json(
            &admin_url(tid, &mid, "veto/reset"),
            &json!({ "reason": "too late" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

/// A tournament with no veto format cannot have a veto restarted, and the
/// refusal leaves the match untouched.
#[tokio::test]
async fn veto_reset_refuses_a_match_that_is_not_in_the_veto_phase() {
    let app = TestApp::new().await;
    let (tid, mid, _, _) = create_tournament_with_matches(&app, "rescue-veto-refuse").await;

    let response = app
        .post_json(
            &admin_url(&tid, &mid, "veto/reset"),
            &json!({ "reason": "not applicable" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(match_status(&app, &tid, &mid).await, "ready");
}

// =============================================================================
// Game server
// =============================================================================

fn ok_script() -> Script {
    Arc::new(|line: &str| (true, format!("ran: {line}")))
}

async fn create_server(app: &TestApp, name: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/admin/game-servers",
            &json!({
                "name": name,
                "game_id": game_id,
                "ip_address": "203.0.113.10",
                "port": 27015,
                "gotv_port": 27020,
                "region": "eu-west"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    str_of(&response.json::<Value>()["data"], "id")
}

async fn connect_idle_agent(app: &mut TestApp, server_id: &str) -> FakeAgent {
    let addr = app.start_server().await;
    let agent = FakeAgent::connect(addr, server_id, ok_script()).await;
    agent.heartbeat("none", None).await;
    let uri = format!("/v1/admin/game-servers/{server_id}");
    wait_for(10, || async {
        let body: Value = app.get_auth(&uri).await.json();
        let data = &body["data"];
        (data["agent_connected"] == true && data["status"] == "available").then_some(())
    })
    .await;
    agent
}

async fn set_steam_id(app: &TestApp, registration_id: &str, steam_id: Option<&str>) {
    sqlx::query(
        "UPDATE players SET steam_id = $1 WHERE id = \
         (SELECT player_id FROM tournament_registrations WHERE id = $2)",
    )
    .bind(steam_id)
    .bind(uuid_of(registration_id))
    .execute(app.pool())
    .await
    .unwrap();
}

async fn latest_config(app: &TestApp, match_id: &str) -> Value {
    let (config,): (Option<Value>,) = sqlx::query_as(
        "SELECT match_config FROM server_reservations WHERE match_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(uuid_of(match_id))
    .fetch_one(app.pool())
    .await
    .unwrap();
    config.expect("the reservation stored a config")
}

/// The must-have: a match loaded on the server with a roster that is wrong.
/// The admin can see what is wrong, is refused a reload that could not
/// build a config, reloads once it can, and can clear the server outright —
/// with or without a live reservation, and with an honest answer when the
/// server cannot be reached.
#[tokio::test]
async fn an_invalid_matchzy_config_can_be_cleared_and_reloaded() {
    let mut app = TestApp::new_with_gameserver().await;
    let (tid, mid, reg1, reg2) = create_tournament_with_matches(&app, "rescue-server").await;
    schedule_in(&app, &tid, &mid, 600).await;
    transition(&app, &tid, &mid, "in_progress").await;

    // One player has a Steam ID, the other does not.
    set_steam_id(&app, &reg1, Some("76561198000000001")).await;
    set_steam_id(&app, &reg2, None).await;

    let check: Value = app
        .get_auth(&format!("/v1/matches/{mid}/server/roster-check"))
        .await
        .json();
    let check = &check["data"];
    assert_eq!(check["ok"], false, "{check}");
    let flagged: Vec<&Value> = check["teams"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["players"].as_array().unwrap())
        .filter(|p| p["valid"] == false)
        .collect();
    assert_eq!(flagged.len(), 1, "{check}");
    assert_eq!(flagged[0]["problem"], "no linked Steam ID");
    assert!(problem_codes(&diagnostics(&app, &tid, &mid).await).contains(&"roster_invalid".into()));

    let server_id = create_server(&app, "rescue-server-1").await;
    let agent = connect_idle_agent(&mut app, &server_id).await;

    // A reload that cannot build a config is refused before anything is
    // touched: nothing reaches the server.
    let response = app
        .post_json(
            &format!("/v1/matches/{mid}/server/reload"),
            &json!({ "reason": "roster was wrong" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        response.text().contains("no usable Steam ID"),
        "{}",
        response.text()
    );
    assert!(agent.commands("load_match").is_empty());

    // Fix the roster; the reload now loads the match.
    set_steam_id(&app, &reg2, Some("76561198000000002")).await;
    let check: Value = app
        .get_auth(&format!("/v1/matches/{mid}/server/roster-check"))
        .await
        .json();
    assert!(
        check["data"]["teams"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|t| t["players"].as_array().unwrap())
            .all(|p| p["valid"] == true),
        "{check}"
    );
    let response = app
        .post_json(
            &format!("/v1/matches/{mid}/server/reload"),
            &json!({ "reason": "roster fixed" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body = response.json::<Value>()["data"].clone();
    assert_eq!(body["status"], "configuring", "{body}");
    assert_eq!(body["server_name"], "rescue-server-1");
    assert_eq!(body["server_id"], server_id.as_str());
    assert_eq!(agent.commands("load_match").len(), 1);
    assert!(
        latest_config(&app, &mid)
            .await
            .to_string()
            .contains("76561198000000002")
    );

    // The roster changes again (a different account): reload rebuilds the
    // config from the roster as it is now and loads it over the old one.
    set_steam_id(&app, &reg2, Some("76561198000000099")).await;
    let ends_before = agent.commands("end_match").len();
    app.post_json(
        &format!("/v1/matches/{mid}/server/reload"),
        &json!({ "reason": "player switched account" }),
    )
    .await
    .assert_status(StatusCode::OK);
    assert_eq!(agent.commands("load_match").len(), 2);
    assert!(
        agent.commands("end_match").len() > ends_before,
        "the old config was cleared off the server first"
    );
    let config = latest_config(&app, &mid).await.to_string();
    assert!(config.contains("76561198000000099"), "{config}");
    assert!(!config.contains("76561198000000002"), "{config}");
    let (live,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM server_reservations WHERE match_id = $1 \
         AND status IN ('pending', 'configuring', 'ready', 'live')",
    )
    .bind(uuid_of(&mid))
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(live, 1, "exactly one live reservation after a reload");

    // Clear: the reservation is released and the server acknowledges.
    let ends_before = agent.commands("end_match").len();
    let response = app.delete_auth(&format!("/v1/matches/{mid}/server")).await;
    response.assert_status(StatusCode::OK);
    let body = response.json::<Value>()["data"].clone();
    assert_eq!(body["released_reservation"], true, "{body}");
    assert_eq!(body["server_cleared"], true, "{body}");
    assert_eq!(body["server_name"], "rescue-server-1");
    assert_eq!(agent.commands("end_match").len(), ends_before + 1);
    let server: Value = app
        .get_auth(&format!("/v1/admin/game-servers/{server_id}"))
        .await
        .json();
    assert_eq!(server["data"]["status"], "available");

    // Clear again with no live reservation: the server is still told to end
    // the match (a config can outlive its reservation).
    let response = app.delete_auth(&format!("/v1/matches/{mid}/server")).await;
    response.assert_status(StatusCode::OK);
    let body = response.json::<Value>()["data"].clone();
    assert_eq!(body["released_reservation"], false, "{body}");
    assert_eq!(body["server_cleared"], true, "{body}");
    assert_eq!(agent.commands("end_match").len(), ends_before + 2);

    // Agent gone: the answer says the server was NOT cleared, and why.
    agent.close().await;
    wait_for(10, || async {
        let body: Value = app
            .get_auth(&format!("/v1/admin/game-servers/{server_id}"))
            .await
            .json();
        (body["data"]["agent_connected"] == false).then_some(())
    })
    .await;
    let response = app.delete_auth(&format!("/v1/matches/{mid}/server")).await;
    response.assert_status(StatusCode::OK);
    let body = response.json::<Value>()["data"].clone();
    assert_eq!(body["server_cleared"], false, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("could not be reached"),
        "{body}"
    );
}

/// A match that was never loaded anywhere: clearing is a no-op that says so.
#[tokio::test]
async fn clearing_a_match_with_no_reservation_reports_nothing_to_clear() {
    let app = TestApp::new().await;
    let (_tid, mid, _, _) = create_tournament_with_matches(&app, "rescue-clear-none").await;

    let response = app.delete_auth(&format!("/v1/matches/{mid}/server")).await;
    response.assert_status(StatusCode::OK);
    let body = response.json::<Value>()["data"].clone();
    assert_eq!(body["released_reservation"], false);
    assert_eq!(body["server_cleared"], false);
    assert!(body["detail"].is_string());
}

// =============================================================================
// Diagnostics
// =============================================================================

#[tokio::test]
async fn diagnostics_flags_a_timeless_schedule_and_a_failed_reservation() {
    use portal_core::ids::ServerReservationId;
    use portal_domain::repositories::{CreateServerReservation, ServerReservationRepository};
    use portal_domain::services::game_server::hash_token;

    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tid, mid, _, _) = create_tournament_with_matches(&app, "rescue-diag").await;

    // A healthy ready match has no problems.
    let d = diagnostics(&app, &tid, &mid).await;
    assert_eq!(d["status"], "ready");
    assert!(d["problems"].as_array().unwrap().is_empty(), "{d}");
    assert!(d["veto"].is_null());
    assert!(d["reservation"].is_null());

    // The stranded state from the original incident.
    sqlx::query(
        "UPDATE tournament_matches SET status = 'scheduled', scheduled_at = NULL WHERE id = $1",
    )
    .bind(uuid_of(&mid))
    .execute(app.pool())
    .await
    .unwrap();
    let d = diagnostics(&app, &tid, &mid).await;
    assert_eq!(suggested_action(&d, "scheduled_without_time"), "set_time");

    // A server setup that failed.
    let reservation = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id: mid.parse().unwrap(),
            kind: portal_core::types::ReservationKind::Match,
            connect_password: "pw12345678".into(),
            gotv_password: None,
            config_token_hash: hash_token("cfg"),
            event_token_hash: hash_token("evt"),
            config_token_expires_at: chrono::Utc::now(),
        })
        .await
        .unwrap();
    state
        .server_reservation_repo
        .mark_failed(
            reservation.id,
            "players on \"X\" have no linked Steam ID: y",
        )
        .await
        .unwrap();
    let d = diagnostics(&app, &tid, &mid).await;
    assert_eq!(suggested_action(&d, "reservation_failed"), "reload_server");
    assert_eq!(d["reservation"]["status"], "failed");
    assert!(
        d["reservation"]["failure_reason"]
            .as_str()
            .unwrap()
            .contains("no linked Steam ID")
    );
}

// =============================================================================
// Completion retry
// =============================================================================

#[tokio::test]
async fn completion_retry_reruns_settlement_for_a_decided_match_only() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rescue-retry", 3, json!({})).await;
    let tid = &cup.tournament_id;
    let m = cup_matches(&app, tid).await[0].clone();
    let mid = str_of(&m, "id");
    let bracket_id = str_of(&m, "bracket_id");
    let winner = str_of(&m, "participant1_registration_id");
    let loser = str_of(&m, "participant2_registration_id");

    // Not decided yet: refused.
    app.post_json(
        &admin_url(tid, &mid, "completion/retry"),
        &json!({ "reason": "standings look behind" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    app.post_json(
        &admin_url(tid, &mid, "forfeit"),
        &json!({
            "forfeiting_registration_id": loser,
            "forfeit_type": "no_show",
            "reason": "Did not turn up to the match"
        }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Simulate the table having fallen behind the result.
    sqlx::query("UPDATE tournament_standings SET points = 0, matches_played = 0, matches_won = 0 WHERE bracket_id = $1")
        .bind(uuid_of(&bracket_id))
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(
        standings_row(&app, tid, &bracket_id, &winner).await["points"],
        0
    );

    let response = app
        .post_json(
            &admin_url(tid, &mid, "completion/retry"),
            &json!({ "reason": "standings look behind" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body = response.json::<Value>()["data"].clone();
    assert_eq!(body["ran"], true, "{body}");
    assert!(body["outcome"].as_str().unwrap().contains("standings"));
    assert_eq!(
        standings_row(&app, tid, &bracket_id, &winner).await["points"],
        3,
        "the table caught up"
    );

    // Idempotent.
    app.post_json(
        &admin_url(tid, &mid, "completion/retry"),
        &json!({ "reason": "again" }),
    )
    .await
    .assert_status(StatusCode::OK);
    assert_eq!(
        standings_row(&app, tid, &bracket_id, &winner).await["points"],
        3
    );
}

// =============================================================================
// Authorization
// =============================================================================

/// Every rescue endpoint is admin-only. Asserted with a real participant
/// token: the dev token short-circuits permission checks in test builds.
#[tokio::test]
async fn rescue_endpoints_refuse_a_non_admin() {
    let app = TestApp::new().await;
    let (tid, mid, _, _, player_token) =
        create_tournament_with_matches_and_opponent(&app, "rescue-authz").await;
    let deadline = chrono::Utc::now() + chrono::Duration::minutes(30);

    for (action, body) in [
        ("reset", json!({ "reason": "let me replay" })),
        (
            "check-in-window",
            json!({ "deadline": deadline.to_rfc3339(), "reason": "more time please" }),
        ),
        ("veto/reset", json!({ "reason": "redo the veto" })),
        ("completion/retry", json!({ "reason": "retry" })),
    ] {
        app.post_json_with_token(&admin_url(&tid, &mid, action), &body, &player_token)
            .await
            .assert_status(StatusCode::FORBIDDEN);
    }
    app.get_with_token(&admin_url(&tid, &mid, "diagnostics"), &player_token)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    app.get_with_token(
        &format!("/v1/matches/{mid}/server/roster-check"),
        &player_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    app.post_json_with_token(
        &format!("/v1/matches/{mid}/server/reload"),
        &json!({ "reason": "reload" }),
        &player_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    app.delete_with_token(&format!("/v1/matches/{mid}/server"), &player_token)
        .await
        .assert_status(StatusCode::FORBIDDEN);

    assert_eq!(match_status(&app, &tid, &mid).await, "ready");
}

// =============================================================================
// Scheduling policy
// =============================================================================

struct TeamFixture {
    tournament_id: Uuid,
    match_id: Uuid,
    league_id: Uuid,
    team_b_id: Uuid,
    team_b_season_id: Uuid,
    b_member_player_id: Uuid,
    a_captain_token: String,
    a_cocaptain_token: String,
    a_member_token: String,
    b_captain_token: String,
    b_member_token: String,
}

/// Two rostered teams facing each other in a `ready` match.
async fn team_match(app: &TestApp) -> TeamFixture {
    let short = Uuid::new_v4().simple().to_string()[..8].to_string();
    let user = async |name: &str| {
        UserBuilder::new()
            .username(format!("resc_{name}_{short}"))
            .build_persisted(app.pool())
            .await
    };
    let a_captain = user("a_cap").await;
    let a_cocaptain = user("a_co").await;
    let a_member = user("a_mem").await;
    let b_captain = user("b_cap").await;
    let b_member = user("b_mem").await;

    let league = LeagueBuilder::new()
        .name(format!("Rescue League {short}"))
        .slug(format!("rescue-league-{short}"))
        .build_persisted(app.pool())
        .await;
    let season = LeagueSeasonBuilder::new()
        .league_id(league.id)
        .name(format!("Rescue Season {short}"))
        .slug(format!("rescue-season-{short}"))
        .registration()
        .build_persisted(app.pool())
        .await;

    let team_season = async |label: &str, owner: Uuid, members: Vec<(Uuid, &str)>| {
        let team = LeagueTeamBuilder::new()
            .name(format!("Rescue {label} {short}"))
            .tag(label)
            .league_id(league.id)
            .owner(owner)
            .build_persisted(app.pool())
            .await;
        let team_season = LeagueTeamSeasonBuilder::new()
            .team_id(team.id)
            .season_id(season.id)
            .build_persisted(app.pool())
            .await;
        for (player_id, role) in members {
            LeagueTeamMemberBuilder::new()
                .team_season_id(team_season.id)
                .player_id(player_id)
                .role(role)
                .build_persisted(app.pool())
                .await;
        }
        (team.id, team_season.id)
    };
    let (_team_a, team_a_season) = team_season(
        "AAA",
        a_captain.id,
        vec![
            (a_captain.id, "captain"),
            (a_cocaptain.id, "captain"),
            (a_member.id, "player"),
        ],
    )
    .await;
    let (team_b, team_b_season) = team_season(
        "BBB",
        b_captain.id,
        vec![(b_captain.id, "captain"), (b_member.id, "player")],
    )
    .await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let tournament = TournamentBuilder::new()
        .game_id(game_id)
        .created_by(a_captain.id)
        .name(format!("Rescue Tournament {short}"))
        .slug(format!("rescue-tournament-{short}"))
        .single_elimination()
        .self_scheduled()
        .in_progress()
        .build_persisted(app.pool())
        .await;
    let stage = TournamentStageBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .single_elimination()
        .build_persisted(app.pool())
        .await;
    let bracket = TournamentBracketBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .stage_id(stage.id)
        .single_elimination()
        .build_persisted(app.pool())
        .await;
    let reg_a = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .team_season_id_from_uuid(team_a_season)
        .participant_name("Team AAA")
        .registered_by_uuid(a_captain.id)
        .approved()
        .build_persisted(app.pool())
        .await;
    let reg_b = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .team_season_id_from_uuid(team_b_season)
        .participant_name("Team BBB")
        .registered_by_uuid(b_captain.id)
        .approved()
        .build_persisted(app.pool())
        .await;
    let match_ = TournamentMatchBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .stage_id(stage.id)
        .bracket_id(bracket.id)
        .round(1)
        .match_number(1)
        .bo1()
        .participant1(reg_a.id, "Team AAA")
        .participant2(reg_b.id, "Team BBB")
        .build_persisted(app.pool())
        .await;
    sqlx::query("UPDATE tournament_matches SET status = 'ready' WHERE id = $1")
        .bind(match_.id.as_uuid())
        .execute(app.pool())
        .await
        .unwrap();

    let token = |u: &portal_db::entities::UserRow| {
        create_test_token(u.id, u.id, &u.username, TEST_JWT_SECRET)
    };
    TeamFixture {
        tournament_id: tournament.id,
        match_id: match_.id.as_uuid(),
        league_id: league.id,
        team_b_id: team_b,
        team_b_season_id: team_b_season,
        b_member_player_id: b_member.id,
        a_captain_token: token(&a_captain),
        a_cocaptain_token: token(&a_cocaptain),
        a_member_token: token(&a_member),
        b_captain_token: token(&b_captain),
        b_member_token: token(&b_member),
    }
}

/// Agreeing a time commits the team to turning up, so it is the captain's
/// call (or a delegate's): a plain roster member can neither propose nor
/// accept, a co-captain can withdraw a teammate's proposal, and a member the
/// captain delegates to can then accept.
#[tokio::test]
async fn scheduling_is_for_captains_and_their_delegates() {
    let app = TestApp::new().await;
    let fx = team_match(&app).await;
    let base = format!(
        "/v1/tournaments/{}/matches/{}/schedule",
        fx.tournament_id, fx.match_id
    );
    let times = |hours: i64| {
        json!({
            "proposed_times": [(chrono::Utc::now() + chrono::Duration::hours(hours)).to_rfc3339()]
        })
    };

    // A plain roster member cannot propose.
    let response = app
        .post_json_with_token(&format!("{base}/propose"), &times(24), &fx.a_member_token)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert!(
        response.text().contains("captain or a delegate"),
        "{}",
        response.text()
    );

    // The captain can.
    let response = app
        .post_json_with_token(&format!("{base}/propose"), &times(24), &fx.a_captain_token)
        .await;
    response.assert_status(StatusCode::CREATED);
    let first = str_of(&response.json::<Value>()["data"], "id");

    // The opposing plain member cannot accept or reject it.
    let accept_first = json!({
        "proposal_id": first,
        "selected_time": response.json::<Value>()["data"]["proposed_times"][0]
    });
    app.post_json_with_token(&format!("{base}/accept"), &accept_first, &fx.b_member_token)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    app.post_json_with_token(
        &format!("{base}/reject"),
        &json!({ "proposal_id": first, "reason": "no" }),
        &fx.b_member_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    // The opponent cannot withdraw it; the proposer's co-captain can.
    app.post_json_with_token(
        &format!("{base}/cancel"),
        &json!({ "proposal_id": first }),
        &fx.b_captain_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    app.post_json_with_token(
        &format!("{base}/cancel"),
        &json!({ "proposal_id": first }),
        &fx.a_cocaptain_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // A fresh proposal from the co-captain.
    let response = app
        .post_json_with_token(
            &format!("{base}/propose"),
            &times(48),
            &fx.a_cocaptain_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let second = response.json::<Value>()["data"].clone();

    // Team B's captain delegates to the plain member, who can now accept.
    app.post_json_with_token(
        &format!(
            "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
            fx.league_id, fx.team_b_id, fx.team_b_season_id
        ),
        &json!({ "player_id": fx.b_member_player_id.to_string() }),
        &fx.b_captain_token,
    )
    .await
    .assert_status(StatusCode::CREATED);
    let response = app
        .post_json_with_token(
            &format!("{base}/accept"),
            &json!({
                "proposal_id": second["id"],
                "selected_time": second["proposed_times"][0]
            }),
            &fx.b_member_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let m = response.json::<Value>()["data"].clone();
    assert_eq!(m["status"], "scheduled");
    assert!(m["scheduled_at"].is_string());
}

// =============================================================================
// Tournament settings on a running cup
// =============================================================================

/// The game-server opt-in and its tunables live in `settings`; an organiser
/// has to be able to change them once the cup is under way, without the
/// other settings keys being lost.
#[tokio::test]
async fn settings_can_be_patched_on_a_started_tournament() {
    let app = TestApp::new().await;
    let (tid, _mid, _, _) = create_tournament_with_matches(&app, "rescue-settings").await;

    let response = app
        .patch_json(
            &format!("/v1/tournaments/{tid}"),
            &json!({ "settings": { "side_selection_mode": "knife" } }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let response = app
        .patch_json(
            &format!("/v1/tournaments/{tid}"),
            &json!({ "settings": { "game_server": {
                "enabled": true, "auto_confirm_minutes": 30, "min_players_to_ready": 4
            } } }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let settings = response.json::<Value>()["data"]["settings"].clone();
    assert_eq!(settings["game_server"]["enabled"], true, "{settings}");
    assert_eq!(settings["game_server"]["auto_confirm_minutes"], 30);
    assert_eq!(
        settings["side_selection_mode"], "knife",
        "other keys survive: {settings}"
    );

    // Frozen fields stay frozen, alone or alongside settings.
    app.patch_json(
        &format!("/v1/tournaments/{tid}"),
        &json!({ "name": "Renamed mid-cup", "settings": { "side_selection_mode": "knife" } }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}
