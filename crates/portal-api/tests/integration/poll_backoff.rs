//! Retry semantics for the share-code poller.
//!
//! `cs2-poller` skipped any entry with `poll_errors >= 10`, which was a
//! one-way door rather than a backoff:
//!
//! * No delay between attempts, so ten failures accumulated in ten minutes of
//!   ordinary polling and any outage longer than that spent the allowance.
//! * A skipped entry was never polled again, and `poll_errors` only reset on a
//!   successful poll — so the counter could not come down by any route the
//!   system had.
//! * `update_auth_code` did not clear it, so the one self-service fix for the
//!   commonest cause changed nothing. There was no admin route either.
//! * Every failure counted the same, so rate limiting alone — a global
//!   condition — could permanently kill every entry in the table.
//!
//! Each of those is pinned below. Time is advanced by moving `next_poll_at`
//! into the past rather than by sleeping.

use crate::common::{TestApp, TestResponse};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use portal_api::extractors::api_key::hash_api_key;
use portal_db::DbPool;
use portal_test::builders::UserBuilder;
use serde_json::json;
use tower::util::ServiceExt;
use uuid::Uuid;

// =============================================================================
// Helpers
// =============================================================================

const POLLER_PERMISSIONS: &[&str] = &[
    "steam_tracking.read",
    "steam_tracking.write",
    "discovered_matches.read",
    "discovered_matches.write",
];

async fn create_poller_key(pool: &DbPool) -> String {
    let raw_key = format!("cgp_test{}", Uuid::now_v7().to_string().replace('-', ""));
    let key_hash = hash_api_key(&raw_key);
    let key_prefix = &raw_key[..8];

    let (key_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO api_keys (service_name, key_hash, key_prefix) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind("cs2-poller")
    .bind(&key_hash)
    .bind(key_prefix)
    .fetch_one(pool)
    .await
    .expect("create api key");

    sqlx::query(
        "INSERT INTO api_key_permissions (api_key_id, permission_id) \
         SELECT $1, p.id FROM permissions p WHERE p.name = ANY($2)",
    )
    .bind(key_id)
    .bind(POLLER_PERMISSIONS)
    .execute(pool)
    .await
    .expect("link permissions");

    raw_key
}

async fn raw_request(app: &TestApp, req: Request<Body>) -> TestResponse {
    let response = app.app.clone().oneshot(req).await.expect("request failed");
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    TestResponse {
        status,
        headers,
        body,
    }
}

async fn key_get(app: &TestApp, uri: &str, key: &str) -> TestResponse {
    raw_request(
        app,
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

async fn key_patch(app: &TestApp, uri: &str, body: &serde_json::Value, key: &str) -> TestResponse {
    raw_request(
        app,
        Request::builder()
            .method("PATCH")
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("X-API-Key", key)
            .body(Body::from(serde_json::to_string(body).unwrap()))
            .unwrap(),
    )
    .await
}

/// Seed one active tracking entry with a cursor already set.
async fn seed_tracking(app: &TestApp, steam_id_64: i64) -> Uuid {
    let user = UserBuilder::new()
        .username(format!("poll_{}", Uuid::now_v7().simple()))
        .email(format!("poll-{}@example.com", Uuid::now_v7().simple()))
        .build_persisted(app.pool())
        .await;
    let game_id = portal_test::helpers::get_game_id(app.pool(), "cs2").await;

    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO steam_tracking \
             (player_id, game_id, steam_id_64, game_auth_code, last_known_code) \
         VALUES ($1, $2, $3, 'AAAA-BBBBB-CCCC', 'CSGO-aaaaa-bbbbb-ccccc-ddddd-eeeee') \
         RETURNING id",
    )
    .bind(user.id)
    .bind(game_id)
    .bind(steam_id_64)
    .fetch_one(app.pool())
    .await
    .expect("seed tracking");
    id
}

#[derive(Debug, sqlx::FromRow)]
struct PollState {
    poll_errors: i32,
    poll_state: String,
    next_poll_at: chrono::DateTime<chrono::Utc>,
    paused_at: Option<chrono::DateTime<chrono::Utc>>,
    last_error: Option<String>,
    last_known_code: Option<String>,
}

async fn poll_state(app: &TestApp, id: Uuid) -> PollState {
    sqlx::query_as::<_, PollState>(
        "SELECT poll_errors, poll_state::TEXT as poll_state, next_poll_at, paused_at, \
                last_error, last_known_code \
         FROM steam_tracking WHERE id = $1",
    )
    .bind(id)
    .fetch_one(app.pool())
    .await
    .expect("read poll state")
}

async fn expire_backoff(app: &TestApp, id: Uuid) {
    sqlx::query(
        "UPDATE steam_tracking SET next_poll_at = NOW() - INTERVAL '1 second' WHERE id = $1",
    )
    .bind(id)
    .execute(app.pool())
    .await
    .unwrap();
}

/// The poller's work list.
async fn due_ids(app: &TestApp, key: &str) -> Vec<String> {
    let response = key_get(app, "/v1/internal/steam-tracking/active?game=cs2", key).await;
    response.assert_status(StatusCode::OK);
    response
        .json::<Vec<serde_json::Value>>()
        .into_iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

async fn report(app: &TestApp, id: Uuid, key: &str, body: &serde_json::Value) {
    key_patch(
        app,
        &format!("/v1/internal/steam-tracking/{id}/poll-result"),
        body,
        key,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);
}

// =============================================================================
// Transient failures: backoff, and no ceiling
// =============================================================================

/// The delay grows, and the entry is held back until it elapses.
#[tokio::test]
async fn test_transient_failures_back_off_progressively() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let id = seed_tracking(&app, 76_561_198_000_000_301).await;

    let mut gaps = Vec::new();
    for attempt in 1..=3 {
        expire_backoff(&app, id).await;
        let before = chrono::Utc::now();
        report(
            &app,
            id,
            &key,
            &json!({ "outcome": "transient", "error": "connection reset" }),
        )
        .await;
        let state = poll_state(&app, id).await;
        assert_eq!(state.poll_errors, attempt);
        assert_eq!(state.poll_state, "backoff");
        gaps.push((state.next_poll_at - before).num_seconds());
    }

    // Base 60s with equal jitter: attempt n waits in [30 * 2^(n-1), 60 * 2^(n-1)].
    assert!((30..=61).contains(&gaps[0]), "first gap {}s", gaps[0]);
    assert!((60..=121).contains(&gaps[1]), "second gap {}s", gaps[1]);
    assert!((120..=241).contains(&gaps[2]), "third gap {}s", gaps[2]);

    // And while it is backing off, the poller is not offered it.
    assert!(
        !due_ids(&app, &key).await.contains(&id.to_string()),
        "an entry mid-backoff must not be handed out"
    );
}

/// The core regression. Far past the old cliff of ten, the entry is STILL
/// polled — transient failures have no attempt ceiling, only a growing delay.
#[tokio::test]
async fn test_transient_failures_never_permanently_park_the_entry() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let id = seed_tracking(&app, 76_561_198_000_000_302).await;

    for _ in 0..25 {
        expire_backoff(&app, id).await;
        assert!(
            due_ids(&app, &key).await.contains(&id.to_string()),
            "entry must remain pollable regardless of how many transient \
             failures preceded it"
        );
        report(
            &app,
            id,
            &key,
            &json!({ "outcome": "transient", "error": "timeout" }),
        )
        .await;
    }

    let state = poll_state(&app, id).await;
    assert_eq!(state.poll_errors, 25);
    assert_eq!(
        state.poll_state, "backoff",
        "still merely backing off, never abandoned"
    );

    // The delay is capped rather than unbounded — six hours, not 2^25 minutes.
    let gap = (state.next_poll_at - chrono::Utc::now()).num_seconds();
    assert!(
        (10_800..=21_601).contains(&gap),
        "backoff should saturate at the 6h cap, got {gap}s"
    );

    // And a single success wipes the slate.
    expire_backoff(&app, id).await;
    report(&app, id, &key, &json!({ "outcome": "ok" })).await;
    let recovered = poll_state(&app, id).await;
    assert_eq!(recovered.poll_errors, 0);
    assert_eq!(recovered.poll_state, "ok");
    assert!(due_ids(&app, &key).await.contains(&id.to_string()));
}

// =============================================================================
// Rate limiting is not the entry's fault
// =============================================================================

/// A 429 delays the entry without counting against it. Rate limiting is a
/// property of our own request volume; charging it per entry is how it could
/// once kill the whole table.
#[tokio::test]
async fn test_rate_limiting_does_not_count_against_the_entry() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let id = seed_tracking(&app, 76_561_198_000_000_303).await;

    for _ in 0..15 {
        expire_backoff(&app, id).await;
        report(
            &app,
            id,
            &key,
            &json!({ "outcome": "rate-limited", "error": "HTTP 429" }),
        )
        .await;
    }

    let state = poll_state(&app, id).await;
    assert_eq!(
        state.poll_errors, 0,
        "rate limiting must not accumulate against the token: {state:?}"
    );
    assert_eq!(
        state.poll_state, "ok",
        "a rate-limited entry is not an unhealthy entry"
    );

    // Cooldown is flat, not escalating: still ~5 minutes after 15 hits.
    let gap = (state.next_poll_at - chrono::Utc::now()).num_seconds();
    assert!(
        (240..=301).contains(&gap),
        "expected a flat ~300s cooldown, got {gap}s"
    );
}

// =============================================================================
// Permanent failures pause immediately, and name the fix
// =============================================================================

/// A revoked auth code pauses on the FIRST occurrence — retrying a 403 nine
/// more times tells nobody anything.
#[tokio::test]
async fn test_auth_expired_pauses_immediately() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let id = seed_tracking(&app, 76_561_198_000_000_304).await;

    report(
        &app,
        id,
        &key,
        &json!({ "outcome": "auth-expired", "error": "Steam returned 403" }),
    )
    .await;

    let state = poll_state(&app, id).await;
    assert_eq!(state.poll_state, "auth_expired");
    assert!(state.paused_at.is_some());
    assert!(state.last_error.is_some());

    // Paused means paused: no amount of elapsed time re-offers it.
    expire_backoff(&app, id).await;
    assert!(
        !due_ids(&app, &key).await.contains(&id.to_string()),
        "a paused entry must not be polled — retrying cannot help"
    );
}

/// Supplying a new auth code is the fix, so it resumes the entry. Previously
/// this changed the credential and left the entry just as dead.
#[tokio::test]
async fn test_new_auth_code_resumes_a_paused_entry() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;

    let user = UserBuilder::new()
        .username(format!("resume_{}", Uuid::now_v7().simple()))
        .email(format!("resume-{}@example.com", Uuid::now_v7().simple()))
        .build_persisted(app.pool())
        .await;
    let steam_id_64: i64 = 76_561_198_000_000_305;
    sqlx::query("UPDATE players SET steam_id = $1, steam_id_64 = $2 WHERE id = $3")
        .bind(steam_id_64.to_string())
        .bind(steam_id_64)
        .bind(user.id)
        .execute(app.pool())
        .await
        .unwrap();

    let token = portal_test::helpers::create_test_token(
        user.id,
        user.id,
        &user.username,
        portal_test::helpers::TEST_JWT_SECRET,
    );

    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": "AAAA-BBBBB-CCCC",
            "game_slug": "cs2",
            "initial_share_code": "CSGO-aaaaa-bbbbb-ccccc-ddddd-eeeee"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let (id,): (Uuid,) = sqlx::query_as("SELECT id FROM steam_tracking WHERE player_id = $1")
        .bind(user.id)
        .fetch_one(app.pool())
        .await
        .unwrap();

    // Steam revokes the code; the poller reports it and the entry pauses.
    report(
        &app,
        id,
        &key,
        &json!({ "outcome": "auth-expired", "error": "Steam returned 403" }),
    )
    .await;
    assert_eq!(poll_state(&app, id).await.poll_state, "auth_expired");

    // The player supplies a new one.
    app.patch_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({ "game_auth_code": "DDDD-EEEEE-FFFF" }),
        &token,
    )
    .await
    .assert_status(StatusCode::OK);

    let resumed = poll_state(&app, id).await;
    assert_eq!(resumed.poll_state, "ok", "a new code must clear the pause");
    assert_eq!(resumed.poll_errors, 0);
    assert!(resumed.last_error.is_none());
    assert!(resumed.paused_at.is_none());
    assert!(
        due_ids(&app, &key).await.contains(&id.to_string()),
        "and put the entry straight back in the poller's work list"
    );
}

/// An invalid cursor is a different fault with a different fix, so a new auth
/// code must NOT silently resume it — that would just reproduce the 412.
#[tokio::test]
async fn test_cursor_invalid_needs_a_cursor_reset_not_a_new_auth_code() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let id = seed_tracking(&app, 76_561_198_000_000_306).await;

    report(
        &app,
        id,
        &key,
        &json!({ "outcome": "cursor-invalid", "error": "Steam returned 412" }),
    )
    .await;
    assert_eq!(poll_state(&app, id).await.poll_state, "cursor_invalid");

    // Admin resume WITHOUT a cursor reset leaves the bad cursor in place, so
    // the next poll would hit the same 412. With one, the cursor is cleared.
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    portal_test::helpers::assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;

    let response = app
        .post_json(
            &format!("/v1/admin/pipeline/tracking/{id}/resume?reset_cursor=true"),
            &json!({}),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let resumed = poll_state(&app, id).await;
    assert_eq!(resumed.poll_state, "ok");
    assert_eq!(resumed.poll_errors, 0);
    assert!(
        resumed.last_known_code.is_none(),
        "the rejected cursor must be dropped, not carried forward"
    );
}

// =============================================================================
// Cursor advances on a partial walk
// =============================================================================

/// A walk that found codes and then failed still banks them. Otherwise a
/// player whose walk reliably breaks partway re-walks the same prefix forever.
#[tokio::test]
async fn test_partial_walk_advances_the_cursor_and_still_backs_off() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let id = seed_tracking(&app, 76_561_198_000_000_307).await;

    report(
        &app,
        id,
        &key,
        &json!({
            "last_known_code": "CSGO-newer-newer-newer-newer-newer",
            "outcome": "transient",
            "error": "connection reset after 3 codes"
        }),
    )
    .await;

    let state = poll_state(&app, id).await;
    assert_eq!(
        state.last_known_code.as_deref(),
        Some("CSGO-newer-newer-newer-newer-newer"),
        "codes found before the failure must not be re-walked"
    );
    assert_eq!(state.poll_errors, 1, "and the failure still counts");
    assert_eq!(state.poll_state, "backoff");
}

// =============================================================================
// Operator surface
// =============================================================================

/// Paused entries are counted apart from erroring ones and sort first, and
/// each carries the action that unsticks it.
#[tokio::test]
async fn test_pipeline_view_separates_paused_from_backing_off() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    portal_test::helpers::assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;

    let healthy = seed_tracking(&app, 76_561_198_000_000_308).await;
    report(&app, healthy, &key, &json!({ "outcome": "ok" })).await;

    let backing_off = seed_tracking(&app, 76_561_198_000_000_309).await;
    report(
        &app,
        backing_off,
        &key,
        &json!({ "outcome": "transient", "error": "timeout" }),
    )
    .await;

    let revoked = seed_tracking(&app, 76_561_198_000_000_310).await;
    report(
        &app,
        revoked,
        &key,
        &json!({ "outcome": "auth-expired", "error": "403" }),
    )
    .await;

    let response = app.get_auth("/v1/admin/pipeline/overview?game=cs2").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let tracking = &body["data"]["tracking"];

    assert_eq!(tracking["paused"], 1, "{body}");
    assert_eq!(tracking["paused_auth_expired"], 1, "{body}");
    assert_eq!(tracking["paused_cursor_invalid"], 0, "{body}");
    assert_eq!(
        tracking["with_errors"], 1,
        "the backing-off entry is erroring; the paused one is counted separately: {body}"
    );

    let response = app.get_auth("/v1/admin/pipeline/tracking?game=cs2").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let entries = body["data"].as_array().unwrap();

    assert_eq!(
        entries[0]["id"].as_str().unwrap(),
        revoked.to_string(),
        "a paused entry outranks a merely-erroring one — it will not recover \
         on its own: {body}"
    );
    assert_eq!(entries[0]["is_paused"], true);
    assert!(
        entries[0]["required_action"]
            .as_str()
            .unwrap()
            .contains("auth code"),
        "the row must say what a person has to do: {body}"
    );

    let healthy_row = entries
        .iter()
        .find(|e| e["id"].as_str().unwrap() == healthy.to_string())
        .unwrap();
    assert_eq!(healthy_row["is_paused"], false);
    assert!(healthy_row["required_action"].is_null());
}

/// An unrecognised outcome is rejected rather than silently misfiled.
#[tokio::test]
async fn test_unknown_poll_outcome_is_rejected() {
    let app = TestApp::new().await;
    let key = create_poller_key(app.pool()).await;
    let id = seed_tracking(&app, 76_561_198_000_000_311).await;

    key_patch(
        &app,
        &format!("/v1/internal/steam-tracking/{id}/poll-result"),
        &json!({ "outcome": "sort-of-broken" }),
        &key,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    assert_eq!(poll_state(&app, id).await.poll_state, "ok");
}
