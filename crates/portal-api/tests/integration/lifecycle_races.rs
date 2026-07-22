//! TDD Wave 4 — failing tests that demonstrate lifecycle race / TOCTOU /
//! non-atomicity bugs. These are RED by design: they assert the *correct*
//! invariant and observe the *wrong* behaviour produced by the current code.
//!
//! Scope note: none of these modify product code. Where true simultaneity is
//! not deterministically reproducible in-process, the race *window* is staged
//! with a direct SQL write (the same technique the codebase already uses for
//! crash/partial-write recovery tests) so the missing compare-and-set is
//! demonstrated deterministically rather than flakily.

use crate::common::TestApp;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use portal_test::prelude::*;
use serde_json::json;
use tower::util::ServiceExt;
use uuid::Uuid;

// ===========================================================================
// Small concurrency helper — fire N truly-concurrent requests through cloned
// routers (mirrors the capacity-race test, but at the HTTP layer).
// ===========================================================================

async fn fire_concurrent(
    app: &TestApp,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    n: usize,
) -> Vec<StatusCode> {
    let mut handles = Vec::new();
    for _ in 0..n {
        let router = app.app.clone();
        let method = method.to_string();
        let uri = uri.to_string();
        let payload = body.as_ref().map(|b| serde_json::to_vec(b).unwrap());
        handles.push(tokio::spawn(async move {
            let mut builder = Request::builder()
                .method(method.as_str())
                .uri(&uri)
                .header("Authorization", "Bearer dev-token");
            let req = if let Some(bytes) = payload {
                builder = builder.header("Content-Type", "application/json");
                builder.body(Body::from(bytes)).unwrap()
            } else {
                builder.body(Body::empty()).unwrap()
            };
            router.oneshot(req).await.unwrap().status()
        }));
    }
    let mut out = Vec::new();
    for h in handles {
        out.push(h.await.expect("task panicked"));
    }
    out
}

// ===========================================================================
// TEST 1 — start_tournament has no compare-and-set, so a crash between
// "brackets built" and "mark_started" lets a retry build a SECOND bracket set.
// ===========================================================================

/// Create an individual single-elimination tournament with 4 approved,
/// seeded participants — i.e. ready to `start`. Returns the tournament id.
async fn create_startable_tournament(app: &TestApp, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Race Start {slug}"),
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

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Player 1 is the dev user.
    let reg1 = crate::tournaments::register_player(app, &tournament_id, "Player1").await;
    crate::tournaments::approve_registration(app, &tournament_id, &reg1).await;

    // Players 2-4 are distinct users, inserted pre-approved.
    for i in 2..=4 {
        let (user_id, player_id) =
            crate::tournaments::create_test_player(app, &format!("racestart_{slug}_{i}")).await;
        crate::tournaments::insert_test_registration(
            app,
            &tournament_id,
            player_id,
            user_id,
            &format!("Player{i}"),
        )
        .await;
    }

    // Auto-seed so the participants are eligible for bracket generation.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    tournament_id
}

async fn count_brackets(app: &TestApp, tournament_id: &str) -> i64 {
    let tid: Uuid = tournament_id.parse().unwrap();
    sqlx::query_scalar("SELECT COUNT(*) FROM tournament_brackets WHERE tournament_id = $1")
        .bind(tid)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

async fn count_matches(app: &TestApp, tournament_id: &str) -> i64 {
    let tid: Uuid = tournament_id.parse().unwrap();
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM tournament_matches tm
         JOIN tournament_brackets tb ON tb.id = tm.bracket_id
         WHERE tb.tournament_id = $1",
    )
    .bind(tid)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

/// A crash after building brackets but before `mark_started` leaves the
/// tournament `scheduled` WITH a full bracket set. `start_tournament` has no
/// idempotency / compare-and-set guard, so a retry rebuilds everything: a
/// SECOND bracket, SECOND match set, SECOND standings. A correct start would
/// detect the existing brackets (or gate the status UPDATE with a predicate)
/// and be a no-op on retry.
#[tokio::test]
async fn test_start_tournament_retry_after_crash_double_builds_brackets() {
    let app = TestApp::new().await;
    let tournament_id = create_startable_tournament(&app, "crash-retry").await;

    // First start succeeds and builds exactly one bracket set.
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    let brackets_after_first = count_brackets(&app, &tournament_id).await;
    let matches_after_first = count_matches(&app, &tournament_id).await;
    assert_eq!(
        brackets_after_first, 1,
        "sanity: first start builds exactly one bracket"
    );
    assert!(
        matches_after_first > 0,
        "sanity: first start builds matches"
    );

    // Simulate the crash window: brackets exist, but mark_started never
    // committed, so status is still pre-start.
    let tid: Uuid = tournament_id.parse().unwrap();
    sqlx::query("UPDATE tournaments SET status = 'scheduled', started_at = NULL WHERE id = $1")
        .bind(tid)
        .execute(app.pool())
        .await
        .unwrap();

    // Retry the start. A correct implementation is idempotent here.
    let retry = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await;

    let brackets_after_retry = count_brackets(&app, &tournament_id).await;
    let matches_after_retry = count_matches(&app, &tournament_id).await;

    assert_eq!(
        brackets_after_retry, 1,
        "start retry after a crash must NOT build a second bracket set \
         (got {brackets_after_retry} brackets; retry HTTP status was {})",
        retry.status
    );
    assert_eq!(
        matches_after_retry, matches_after_first,
        "start retry after a crash must NOT duplicate matches \
         (had {matches_after_first}, now {matches_after_retry})"
    );
}

/// Two simultaneous `start` requests against one scheduled tournament. With
/// no compare-and-set on the status UPDATE, both can pass the status gate and
/// both build brackets. Correct behaviour: exactly one bracket set exists.
#[tokio::test]
async fn test_concurrent_start_requests_build_one_bracket_set() {
    let app = TestApp::new().await;
    let tournament_id = create_startable_tournament(&app, "concurrent-start").await;

    let statuses = fire_concurrent(
        &app,
        "POST",
        &format!("/v1/tournaments/{tournament_id}/start"),
        None,
        2,
    )
    .await;

    let brackets = count_brackets(&app, &tournament_id).await;
    assert_eq!(
        brackets, 1,
        "concurrent starts must not double-build brackets (got {brackets}; \
         response statuses were {statuses:?})"
    );
}

// ===========================================================================
// TEST 2 — coin-flip has no compare-and-set: `record_coin_flip` gates only on
// status == coin_flip, then does an UNCONDITIONAL update. Two flips that both
// pass the gate (the TOCTOU window on concurrent connects) produce two
// independent random outcomes, the second silently overwriting the first.
// ===========================================================================

/// Drive a match to a veto session in the `coin_flip` phase, record a flip,
/// then reproduce the race window (a second connect whose flip is still in
/// flight while status is momentarily back at `coin_flip`) and record a
/// SECOND flip with a different winner. The recorded winner / first-picker
/// must be STABLE — the first flip must win. It does not: the second
/// unconditionally overwrites it.
#[tokio::test]
async fn test_record_coin_flip_is_idempotent_not_rerandomized() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "coinflip-race")
            .await;

    // Create + start the veto session (moves it to coin_flip).
    app.post_json(
        &format!("/v1/matches/{match_id}/veto"),
        &json!({ "veto_format_id": "bo1_veto" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    app.post_auth(&format!("/v1/matches/{match_id}/veto/start"))
        .await
        .assert_status(StatusCode::OK);

    // First flip: reg1 wins and goes first.
    app.post_json(
        &format!("/v1/matches/{match_id}/veto/coin-flip"),
        &json!({ "winner_registration_id": reg1, "winner_goes_first": true }),
    )
    .await
    .assert_status(StatusCode::OK);

    let match_uuid: Uuid = match_id.parse().unwrap();
    let winner_after_first: Option<Uuid> = sqlx::query_scalar(
        "SELECT coin_flip_winner_registration_id FROM veto_sessions WHERE match_id = $1",
    )
    .bind(match_uuid)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(
        winner_after_first.map(|u| u.to_string()),
        Some(reg1.clone()),
        "sanity: first flip recorded reg1 as winner"
    );

    // Stage the TOCTOU window: a concurrent connect's flip passed the
    // `status == coin_flip` gate before the first flip's update committed.
    // record_coin_flip re-checks `can_coin_flip()`, so reproduce the state
    // it would have seen.
    sqlx::query("UPDATE veto_sessions SET status = 'coin_flip' WHERE match_id = $1")
        .bind(match_uuid)
        .execute(app.pool())
        .await
        .unwrap();

    // Second flip: a DIFFERENT independent outcome (reg2 wins).
    app.post_json(
        &format!("/v1/matches/{match_id}/veto/coin-flip"),
        &json!({ "winner_registration_id": reg2, "winner_goes_first": true }),
    )
    .await
    .assert_status(StatusCode::OK);

    let winner_after_second: Option<Uuid> = sqlx::query_scalar(
        "SELECT coin_flip_winner_registration_id FROM veto_sessions WHERE match_id = $1",
    )
    .bind(match_uuid)
    .fetch_one(app.pool())
    .await
    .unwrap();

    assert_eq!(
        winner_after_second.map(|u| u.to_string()),
        Some(reg1),
        "an already-recorded coin flip must not be re-randomized by a \
         second racing flip (winner flipped to reg2)"
    );
}

// ===========================================================================
// TEST 3 — create_ban read-guard + no-unique-constraint TOCTOU: two
// concurrent platform-ban creations for the same user both pass the
// get_active_for_user guard and both INSERT, leaving duplicate active bans.
// ===========================================================================

/// Grant `platform_admin` to the dev user so it holds `admin.bans.manage`.
async fn grant_platform_admin_to_dev(app: &TestApp) {
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let role_id: Uuid = sqlx::query_scalar("SELECT id FROM roles WHERE name = 'platform_admin'")
        .fetch_one(app.pool())
        .await
        .expect("platform_admin role should exist");
    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(dev_user_id)
        .bind(role_id)
        .execute(app.pool())
        .await
        .unwrap();
}

async fn count_active_platform_bans(app: &TestApp, user_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM bans
         WHERE user_id = $1 AND ban_type = 'platform' AND lifted_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn test_concurrent_create_ban_does_not_duplicate_active_bans() {
    let app = TestApp::new().await;
    grant_platform_admin_to_dev(&app).await;

    let target = UserBuilder::new()
        .username("ban_race_target")
        .build_persisted(app.pool())
        .await;

    // Fire several concurrent identical platform-ban creations.
    let statuses = fire_concurrent(
        &app,
        "POST",
        "/v1/admin/bans",
        Some(json!({
            "user_id": target.id.to_string(),
            "ban_type": "platform",
            "reason": "Race test"
        })),
        4,
    )
    .await;

    let active = count_active_platform_bans(&app, target.id).await;
    assert_eq!(
        active, 1,
        "a user must not accumulate duplicate active platform bans under \
         concurrent creation (got {active}; response statuses {statuses:?})"
    );
}

// ===========================================================================
// TEST 4 — availability suggestion generation has no dedupe/replace and no
// unique constraint on suggested_times: calling generate twice duplicates
// every suggestion.
// ===========================================================================

async fn count_suggestions(app: &TestApp, match_id: &str) -> i64 {
    let mid: Uuid = match_id.parse().unwrap();
    sqlx::query_scalar("SELECT COUNT(*) FROM suggested_times WHERE match_id = $1")
        .bind(mid)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

/// Insert a registration-scoped availability window (bypasses the player-only
/// HTTP endpoint so both match participants have overlapping availability).
async fn insert_registration_window(
    app: &TestApp,
    registration_id: &str,
    day_of_week: i16,
    start: &str,
    end: &str,
) {
    let rid: Uuid = registration_id.parse().unwrap();
    sqlx::query(
        "INSERT INTO availability_windows
            (registration_id, day_of_week, start_time, end_time, is_preferred)
         VALUES ($1, $2, $3::time, $4::time, true)",
    )
    .bind(rid)
    .bind(day_of_week)
    .bind(start)
    .bind(end)
    .execute(app.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn test_generate_suggestions_twice_does_not_duplicate() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2) =
        crate::tournaments::create_tournament_with_matches(&app, "suggest-dup").await;

    // 2025-01-13 is a Monday (day_of_week = 1). Give both participants an
    // overlapping window so at least one suggestion is generated.
    insert_registration_window(&app, &reg1, 1, "10:00:00", "14:00:00").await;
    insert_registration_window(&app, &reg2, 1, "11:00:00", "13:00:00").await;

    let gen_body = json!({
        "start_date": "2025-01-13",
        "end_date": "2025-01-19",
        "min_duration_minutes": 60
    });
    let url = format!("/v1/tournaments/{tournament_id}/matches/{match_id}/suggestions/generate");

    app.post_json(&url, &gen_body)
        .await
        .assert_status(StatusCode::CREATED);
    let after_first = count_suggestions(&app, &match_id).await;
    assert!(
        after_first > 0,
        "sanity: overlapping availability should yield at least one suggestion"
    );

    app.post_json(&url, &gen_body)
        .await
        .assert_status(StatusCode::CREATED);
    let after_second = count_suggestions(&app, &match_id).await;

    assert_eq!(
        after_second, after_first,
        "regenerating suggestions must not duplicate them (dedupe or replace); \
         had {after_first}, now {after_second}"
    );
}
