//! Retry semantics for the discovered-match pipeline.
//!
//! Three defects lost match data permanently, and each of them is pinned here
//! by a test that fails against the old behaviour:
//!
//! 1. **No backoff.** A failed match was re-offered on the very next enricher
//!    cycle, so three attempts fitted inside 90 seconds and any transient GC
//!    outage wrote the match off for good.
//! 2. **No demo retry at all.** The demo was fetched inline during enrichment
//!    and, on any failure, the match was recorded as `enriched` with no ratings
//!    and no map name. Valve does not publish a demo the instant a match ends,
//!    so the most likely failure discarded the rank data permanently.
//! 3. **Stranded claims.** A worker that died after claiming left the row in
//!    `enriching` forever — out of the queue and out of the retry-exhausted
//!    count, so nothing surfaced it.
//!
//! Time is advanced by pushing the schedule columns into the past rather than
//! by sleeping: the production intervals are minutes to hours.

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

const ENRICHER_PERMISSIONS: &[&str] = &["discovered_matches.read", "discovered_matches.write"];

async fn create_enricher_key(pool: &DbPool) -> String {
    let raw_key = format!("cgp_test{}", Uuid::now_v7().to_string().replace('-', ""));
    let key_hash = hash_api_key(&raw_key);
    let key_prefix = &raw_key[..8];

    let (key_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO api_keys (service_name, key_hash, key_prefix) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind("cs2-enricher")
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
    .bind(ENRICHER_PERMISSIONS)
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

async fn key_post(app: &TestApp, uri: &str, body: &serde_json::Value, key: &str) -> TestResponse {
    raw_request(
        app,
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("X-API-Key", key)
            .body(Body::from(serde_json::to_string(body).unwrap()))
            .unwrap(),
    )
    .await
}

/// Short unique token for test identities.
///
/// `users.username` and `players.display_name` are both VARCHAR(32), and
/// `build_persisted` copies the username into the display name — so a full
/// 32-char simple UUID overflows the column the moment it carries a prefix.
/// Takes the tail, which is the random half of a v7 rather than the timestamp.
fn unique_suffix() -> String {
    let uuid = Uuid::now_v7().simple().to_string();
    uuid[12..32].to_string()
}

/// Seed a tracking row and return its id.
async fn seed_tracking(app: &TestApp, steam_id_64: i64) -> Uuid {
    let user = UserBuilder::new()
        .username(format!("retry_{}", unique_suffix()))
        .email(format!("retry-{}@example.com", unique_suffix()))
        .build_persisted(app.pool())
        .await;
    let game_id = portal_test::helpers::get_game_id(app.pool(), "cs2").await;

    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO steam_tracking (player_id, game_id, steam_id_64, game_auth_code) \
         VALUES ($1, $2, $3, 'AAAA-BBBBB-CCCC') RETURNING id",
    )
    .bind(user.id)
    .bind(game_id)
    .bind(steam_id_64)
    .fetch_one(app.pool())
    .await
    .expect("seed tracking");
    id
}

/// Seed a discovered match in `pending`, due immediately.
async fn seed_match(app: &TestApp, tracking_id: Uuid, share_code: &str) -> Uuid {
    let game_id = portal_test::helpers::get_game_id(app.pool(), "cs2").await;
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO discovered_matches \
             (tracking_id, game_id, share_code, match_id, outcome_id, token) \
         VALUES ($1, $2, $3, 1, 2, 3) RETURNING id",
    )
    .bind(tracking_id)
    .bind(game_id)
    .bind(share_code)
    .fetch_one(app.pool())
    .await
    .expect("seed discovered match");
    id
}

/// One row of retry state, as the queue sees it.
#[derive(Debug, sqlx::FromRow)]
struct RetryState {
    status: String,
    retry_count: i32,
    max_retries: i32,
    next_attempt_at: chrono::DateTime<chrono::Utc>,
    claimed_at: Option<chrono::DateTime<chrono::Utc>>,
    demo_status: String,
    demo_retry_count: i32,
    demo_max_retries: i32,
    demo_next_attempt_at: chrono::DateTime<chrono::Utc>,
    demo_error: Option<String>,
}

async fn retry_state(app: &TestApp, id: Uuid) -> RetryState {
    sqlx::query_as::<_, RetryState>(
        "SELECT status::TEXT as status, retry_count, max_retries, next_attempt_at, claimed_at, \
                demo_status::TEXT as demo_status, demo_retry_count, demo_max_retries, \
                demo_next_attempt_at, demo_error \
         FROM discovered_matches WHERE id = $1",
    )
    .bind(id)
    .fetch_one(app.pool())
    .await
    .expect("read retry state")
}

/// Make the enrichment backoff elapse without waiting for it.
async fn expire_enrich_backoff(app: &TestApp, id: Uuid) {
    sqlx::query(
        "UPDATE discovered_matches SET next_attempt_at = NOW() - INTERVAL '1 second' WHERE id = $1",
    )
    .bind(id)
    .execute(app.pool())
    .await
    .unwrap();
}

/// Make the demo backoff (or lease) elapse without waiting for it.
async fn expire_demo_backoff(app: &TestApp, id: Uuid) {
    sqlx::query(
        "UPDATE discovered_matches SET demo_next_attempt_at = NOW() - INTERVAL '1 second' \
         WHERE id = $1",
    )
    .bind(id)
    .execute(app.pool())
    .await
    .unwrap();
}

/// Age a claim past the 15-minute enrichment lease.
async fn expire_claim_lease(app: &TestApp, id: Uuid) {
    sqlx::query(
        "UPDATE discovered_matches SET claimed_at = NOW() - INTERVAL '1 hour' WHERE id = $1",
    )
    .bind(id)
    .execute(app.pool())
    .await
    .unwrap();
}

async fn pending_ids(app: &TestApp, key: &str) -> Vec<String> {
    let response = key_get(
        app,
        "/v1/internal/discovered-matches/pending?game=cs2&limit=50",
        key,
    )
    .await;
    response.assert_status(StatusCode::OK);
    response
        .json::<Vec<serde_json::Value>>()
        .into_iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

/// Run one full enrichment attempt that fails, advancing the backoff first so
/// the match is actually due.
async fn fail_one_attempt(app: &TestApp, id: Uuid, key: &str, error: &str) {
    expire_enrich_backoff(app, id).await;
    key_post(
        app,
        &format!("/v1/internal/discovered-matches/{id}/claim"),
        &json!({}),
        key,
    )
    .await
    .assert_status(StatusCode::OK);
    key_post(
        app,
        &format!("/v1/internal/discovered-matches/{id}/failed"),
        &json!({ "error": error }),
        key,
    )
    .await
    .assert_status(StatusCode::OK);
}

/// Enrich a match successfully, which is what opens its demo stage.
async fn enrich_with_demo(app: &TestApp, id: Uuid, key: &str, demo_url: Option<&str>) {
    key_post(
        app,
        &format!("/v1/internal/discovered-matches/{id}/claim"),
        &json!({}),
        key,
    )
    .await
    .assert_status(StatusCode::OK);

    key_post(
        app,
        &format!("/v1/internal/discovered-matches/{id}/enriched"),
        &json!({
            "gc_data": [{ "map": "", "players": [], "team_scores": [13, 7] }],
            "demo_url": demo_url,
        }),
        key,
    )
    .await
    .assert_status(StatusCode::OK);
}

async fn lease_demo_jobs(app: &TestApp, key: &str, limit: i64) -> Vec<serde_json::Value> {
    let response = key_post(
        app,
        &format!("/v1/internal/discovered-matches/demo-jobs?game=cs2&limit={limit}"),
        &json!({}),
        key,
    )
    .await;
    response.assert_status(StatusCode::OK);
    response.json()
}

async fn report_demo(app: &TestApp, id: Uuid, key: &str, body: &serde_json::Value) {
    key_post(
        app,
        &format!("/v1/internal/discovered-matches/{id}/demo-result"),
        body,
        key,
    )
    .await
    .assert_status(StatusCode::OK);
}

// =============================================================================
// Defect 1 — enrichment backoff
// =============================================================================

/// The delay between attempts grows. Without this a 30-second cycle burns the
/// whole budget inside a couple of minutes.
#[tokio::test]
async fn test_enrichment_backoff_grows_between_attempts() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_201).await;
    let id = seed_match(&app, tracking, "CSGO-backoff-grows").await;

    let mut gaps = Vec::new();
    for attempt in 1..=3 {
        let before = chrono::Utc::now();
        fail_one_attempt(&app, id, &key, &format!("GC timeout {attempt}")).await;
        let state = retry_state(&app, id).await;
        assert_eq!(state.retry_count, attempt);
        gaps.push((state.next_attempt_at - before).num_seconds());
    }

    // Base is 60s with equal jitter, so attempt n waits within
    // [30 * 2^(n-1), 60 * 2^(n-1)]. Bands, not exact values — and the upper
    // bounds carry headroom, because the gap is measured from before the round
    // trip, so a loaded CI runner adds its own latency to every reading.
    assert!(
        (29..=70).contains(&gaps[0]),
        "first retry should wait ~30-60s, waited {}s",
        gaps[0]
    );
    assert!(
        (59..=130).contains(&gaps[1]),
        "second retry should wait ~60-120s, waited {}s",
        gaps[1]
    );
    assert!(
        (119..=250).contains(&gaps[2]),
        "third retry should wait ~120-240s, waited {}s",
        gaps[2]
    );
    assert!(gaps[2] > gaps[0], "backoff must grow: {gaps:?}");
}

/// The budget is spent over the full schedule, and only then does the match
/// leave the queue. Six attempts, not three.
#[tokio::test]
async fn test_enrichment_budget_survives_a_full_schedule() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_202).await;
    let id = seed_match(&app, tracking, "CSGO-budget").await;

    let default_max = retry_state(&app, id).await.max_retries;
    assert_eq!(
        default_max, 6,
        "the default budget should be 6 attempts spread over backoff"
    );

    for attempt in 1..default_max {
        fail_one_attempt(&app, id, &key, "GC timeout").await;
        expire_enrich_backoff(&app, id).await;
        assert!(
            pending_ids(&app, &key).await.contains(&id.to_string()),
            "still had budget after attempt {attempt}, must remain queued"
        );
    }

    fail_one_attempt(&app, id, &key, "GC timeout").await;
    expire_enrich_backoff(&app, id).await;

    let state = retry_state(&app, id).await;
    assert_eq!(state.retry_count, default_max);
    assert!(
        !pending_ids(&app, &key).await.contains(&id.to_string()),
        "budget spent, the match must leave the queue"
    );
}

// =============================================================================
// Defect 3 — stranded claims
// =============================================================================

/// A worker that claims and never reports must not strand the row. The sweep
/// runs off the pending fetch, so simply asking for work recovers it.
#[tokio::test]
async fn test_expired_claim_is_reclaimed_and_costs_one_attempt() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_203).await;
    let id = seed_match(&app, tracking, "CSGO-stranded").await;

    key_post(
        &app,
        &format!("/v1/internal/discovered-matches/{id}/claim"),
        &json!({}),
        &key,
    )
    .await
    .assert_status(StatusCode::OK);

    // The worker dies here — no /enriched, no /failed.
    let claimed = retry_state(&app, id).await;
    assert_eq!(claimed.status, "enriching");
    assert!(claimed.claimed_at.is_some());
    assert_eq!(claimed.retry_count, 0);

    // Inside the lease it stays put: a slow worker must not have its match
    // stolen out from under it.
    assert!(
        !pending_ids(&app, &key).await.contains(&id.to_string()),
        "a live claim must be respected"
    );
    assert_eq!(retry_state(&app, id).await.status, "enriching");

    expire_claim_lease(&app, id).await;

    // Asking for work triggers the sweep.
    let _ = pending_ids(&app, &key).await;

    let reclaimed = retry_state(&app, id).await;
    assert_eq!(
        reclaimed.status, "failed",
        "stranded row must return to the queue"
    );
    assert_eq!(
        reclaimed.retry_count, 1,
        "a dead worker's attempt must still be charged, or a poison match loops forever"
    );
    assert!(reclaimed.claimed_at.is_none());
    assert!(
        reclaimed.next_attempt_at > chrono::Utc::now(),
        "the reclaimed row should back off like any other failure"
    );

    expire_enrich_backoff(&app, id).await;
    assert!(
        pending_ids(&app, &key).await.contains(&id.to_string()),
        "and then be offered again"
    );
}

/// Repeated strandings exhaust the budget rather than looping forever. This is
/// why the reclaim charges a retry.
#[tokio::test]
async fn test_repeated_strandings_eventually_exhaust_the_budget() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_204).await;
    let id = seed_match(&app, tracking, "CSGO-poison").await;

    for _ in 0..6 {
        expire_enrich_backoff(&app, id).await;
        key_post(
            &app,
            &format!("/v1/internal/discovered-matches/{id}/claim"),
            &json!({}),
            &key,
        )
        .await;
        expire_claim_lease(&app, id).await;
        let _ = pending_ids(&app, &key).await;
    }

    let state = retry_state(&app, id).await;
    assert!(
        state.retry_count >= state.max_retries,
        "a match that reliably kills its worker must stop being handed out: {state:?}"
    );
    expire_enrich_backoff(&app, id).await;
    assert!(!pending_ids(&app, &key).await.contains(&id.to_string()));
}

// =============================================================================
// Defect 2 — the demo stage
// =============================================================================

/// The production failure, pinned. A demo Valve has not published yet must not
/// cost the match its rank data: the job stays queued for another attempt.
#[tokio::test]
async fn test_demo_not_yet_published_is_retried_not_discarded() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_205).await;
    let id = seed_match(&app, tracking, "CSGO-demo-late").await;

    enrich_with_demo(
        &app,
        id,
        &key,
        Some("http://replay1.valve.net/730/003.dem.bz2"),
    )
    .await;

    let opened = retry_state(&app, id).await;
    assert_eq!(opened.status, "enriched");
    assert_eq!(
        opened.demo_status, "pending",
        "a successful enrichment with a demo URL must open the demo stage"
    );

    // First attempt: the CDN 404s because the demo is not up yet.
    let jobs = lease_demo_jobs(&app, &key, 5).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["id"].as_str().unwrap(), id.to_string());
    assert_eq!(
        jobs[0]["attempt"], 1,
        "the lease banks the attempt up front"
    );

    report_demo(
        &app,
        id,
        &key,
        &json!({ "outcome": "unavailable", "error": "demo not present on the CDN (404)" }),
    )
    .await;

    let after = retry_state(&app, id).await;
    assert_eq!(
        after.demo_status, "pending",
        "the match must remain eligible — this is the bug that lost data"
    );
    assert_eq!(after.demo_retry_count, 1);
    assert!(
        after.demo_next_attempt_at > chrono::Utc::now(),
        "and be scheduled for a later attempt"
    );
    assert!(after.demo_error.is_some());

    // Second attempt, once the backoff elapses, succeeds.
    expire_demo_backoff(&app, id).await;
    let jobs = lease_demo_jobs(&app, &key, 5).await;
    assert_eq!(jobs.len(), 1, "the demo must come back around");
    assert_eq!(jobs[0]["attempt"], 2);
}

/// The lease is exclusive and banks the attempt before any work happens, so a
/// worker that dies mid-parse cannot be handed the same job again immediately
/// — and a restart does not reset the count.
#[tokio::test]
async fn test_demo_lease_is_exclusive_and_durable() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_206).await;
    let id = seed_match(&app, tracking, "CSGO-demo-lease").await;

    enrich_with_demo(
        &app,
        id,
        &key,
        Some("http://replay1.valve.net/730/1.dem.bz2"),
    )
    .await;

    let first = lease_demo_jobs(&app, &key, 5).await;
    assert_eq!(first.len(), 1);

    // A second worker asking straight away gets nothing: the row is leased.
    let second = lease_demo_jobs(&app, &key, 5).await;
    assert!(
        second.is_empty(),
        "a leased job must not be handed to a second worker: {second:?}"
    );

    // The first worker dies without reporting. The attempt is already banked in
    // the database, which is precisely what an in-process counter could not do.
    assert_eq!(retry_state(&app, id).await.demo_retry_count, 1);

    // Lease expires; the job returns, having spent one attempt.
    expire_demo_backoff(&app, id).await;
    let third = lease_demo_jobs(&app, &key, 5).await;
    assert_eq!(third.len(), 1);
    assert_eq!(
        third[0]["attempt"], 2,
        "the dead worker's attempt must still count after a restart"
    );
}

/// A retryable failure settles terminally once the budget is spent — as
/// `unavailable`, matching the last classification.
#[tokio::test]
async fn test_demo_retries_settle_as_unavailable_when_budget_spent() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_207).await;
    let id = seed_match(&app, tracking, "CSGO-demo-gone-eventually").await;

    enrich_with_demo(
        &app,
        id,
        &key,
        Some("http://replay1.valve.net/730/2.dem.bz2"),
    )
    .await;

    let max = retry_state(&app, id).await.demo_max_retries;
    assert_eq!(max, 8, "demo budget should span roughly a day of backoff");

    for attempt in 1..=max {
        expire_demo_backoff(&app, id).await;
        let jobs = lease_demo_jobs(&app, &key, 5).await;
        assert_eq!(jobs.len(), 1, "attempt {attempt} should be offered");
        report_demo(
            &app,
            id,
            &key,
            &json!({ "outcome": "unavailable", "error": "404" }),
        )
        .await;
    }

    let settled = retry_state(&app, id).await;
    assert_eq!(settled.demo_status, "unavailable");
    assert_eq!(settled.demo_retry_count, max);

    expire_demo_backoff(&app, id).await;
    assert!(
        lease_demo_jobs(&app, &key, 5).await.is_empty(),
        "a settled demo must not be leased again"
    );
}

/// A 410 is definitive — no point spending eight attempts re-confirming it.
#[tokio::test]
async fn test_demo_gone_is_terminal_on_the_first_attempt() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_208).await;
    let id = seed_match(&app, tracking, "CSGO-demo-410").await;

    enrich_with_demo(
        &app,
        id,
        &key,
        Some("http://replay1.valve.net/730/3.dem.bz2"),
    )
    .await;
    let jobs = lease_demo_jobs(&app, &key, 5).await;
    assert_eq!(jobs.len(), 1);

    report_demo(
        &app,
        id,
        &key,
        &json!({ "outcome": "gone", "error": "demo retired by Valve (410)" }),
    )
    .await;

    let settled = retry_state(&app, id).await;
    assert_eq!(settled.demo_status, "unavailable");
    assert_eq!(
        settled.demo_retry_count, 1,
        "a definitive answer should not consume the whole budget"
    );

    expire_demo_backoff(&app, id).await;
    assert!(lease_demo_jobs(&app, &key, 5).await.is_empty());
}

/// A match with no demo URL is closed out rather than sitting in the queue.
#[tokio::test]
async fn test_enrichment_without_a_demo_url_closes_the_demo_stage() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_209).await;
    let id = seed_match(&app, tracking, "CSGO-no-demo").await;

    enrich_with_demo(&app, id, &key, None).await;

    assert_eq!(retry_state(&app, id).await.demo_status, "not_applicable");
    assert!(
        lease_demo_jobs(&app, &key, 5).await.is_empty(),
        "there is nothing to fetch"
    );
}

/// A casual demo parses fine but carries no rank updates. That is a success —
/// retrying it forever would be pointless — and its map name is still recorded.
#[tokio::test]
async fn test_empty_demo_is_a_terminal_success_that_still_yields_the_map() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_210).await;
    let id = seed_match(&app, tracking, "CSGO-demo-empty").await;

    enrich_with_demo(
        &app,
        id,
        &key,
        Some("http://replay1.valve.net/730/4.dem.bz2"),
    )
    .await;
    assert_eq!(lease_demo_jobs(&app, &key, 5).await.len(), 1);

    report_demo(
        &app,
        id,
        &key,
        &json!({ "outcome": "empty", "map_name": "de_dust2" }),
    )
    .await;

    assert_eq!(retry_state(&app, id).await.demo_status, "empty");
    expire_demo_backoff(&app, id).await;
    assert!(lease_demo_jobs(&app, &key, 5).await.is_empty());
}

/// An unrecognised outcome is rejected rather than silently settling the row.
#[tokio::test]
async fn test_unknown_demo_outcome_is_rejected() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_211).await;
    let id = seed_match(&app, tracking, "CSGO-demo-bad-outcome").await;

    enrich_with_demo(
        &app,
        id,
        &key,
        Some("http://replay1.valve.net/730/5.dem.bz2"),
    )
    .await;
    lease_demo_jobs(&app, &key, 5).await;

    key_post(
        &app,
        &format!("/v1/internal/discovered-matches/{id}/demo-result"),
        &json!({ "outcome": "probably-fine" }),
        &key,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    assert_eq!(retry_state(&app, id).await.demo_status, "pending");
}

/// The operator view separates the two stages, so a stall in demo fetching is
/// distinguishable from a stall in enrichment.
#[tokio::test]
async fn test_pipeline_overview_reports_the_demo_stage() {
    let app = TestApp::new().await;
    let key = create_enricher_key(app.pool()).await;
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    portal_test::helpers::assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;

    let tracking = seed_tracking(&app, 76_561_198_000_000_212).await;

    let waiting = seed_match(&app, tracking, "CSGO-stage-waiting").await;
    enrich_with_demo(
        &app,
        waiting,
        &key,
        Some("http://replay1.valve.net/730/6.dem.bz2"),
    )
    .await;

    let none = seed_match(&app, tracking, "CSGO-stage-none").await;
    enrich_with_demo(&app, none, &key, None).await;

    let dead = seed_match(&app, tracking, "CSGO-stage-dead").await;
    enrich_with_demo(
        &app,
        dead,
        &key,
        Some("http://replay1.valve.net/730/7.dem.bz2"),
    )
    .await;
    // Reported without leasing first. `record_demo_result` is keyed on the id,
    // and leasing here would actually pick up `waiting` — it sorts earlier on
    // demo_next_attempt_at — which reads as though it were leasing `dead`.
    report_demo(&app, dead, &key, &json!({ "outcome": "gone" })).await;

    let response = app.get_auth("/v1/admin/pipeline/overview?game=cs2").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let stage = &body["data"]["demo_extraction"];

    assert_eq!(stage["pending"], 1, "{body}");
    assert_eq!(stage["not_applicable"], 1, "{body}");
    assert_eq!(stage["unavailable"], 1, "{body}");
}

// =============================================================================
// Operator recovery — requeue
// =============================================================================

/// Grant the dev user `admin.demos.manage`.
///
/// The pipeline handlers check the permission through `permission_service`
/// rather than the `PermissionChecker` extractor, so the dev-token bypass does
/// not apply to them — the role has to be real. Same helper shape as
/// `demos.rs::make_dev_user_admin`.
async fn make_dev_user_admin(app: &TestApp) {
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    portal_test::helpers::assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
}

/// The repair path for budgets that were spent on something other than the
/// match.
///
/// Live, the enricher wrote "Trying to work with closed connection" onto match
/// after match: a dead Steam websocket charged to each match in turn. Those
/// matches were never actually attempted, but `find_pending` excludes an
/// exhausted row by design, so the queue could not recover on its own once the
/// worker was fixed.
#[tokio::test]
async fn test_requeue_returns_exhausted_matches_to_the_queue() {
    // Requeue is gated on admin.demos.manage; grant it before acting.
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_301).await;
    let id = seed_match(&app, tracking, &format!("CSGO-requeue-{}", unique_suffix())).await;

    // Burn the whole budget the way the bug did.
    let max = retry_state(&app, id).await.max_retries;
    for _ in 0..max {
        fail_one_attempt(&app, id, &key, "Trying to work with closed connection").await;
        expire_enrich_backoff(&app, id).await;
    }
    assert!(
        !pending_ids(&app, &key).await.contains(&id.to_string()),
        "precondition: the match is out of the queue for good"
    );

    let response = app
        .post_json(
            "/v1/admin/pipeline/discovered-matches/requeue",
            &json!({ "game": "cs2", "only_exhausted": true }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["requeued"], 1);

    let state = retry_state(&app, id).await;
    assert_eq!(state.status, "pending");
    assert_eq!(
        state.retry_count, 0,
        "the budget is restored, not topped up"
    );
    assert!(state.claimed_at.is_none());

    expire_enrich_backoff(&app, id).await;
    assert!(
        pending_ids(&app, &key).await.contains(&id.to_string()),
        "the enricher must be able to pick it up again"
    );
}

/// A match still inside its backoff is already going to be retried; a bulk
/// requeue of "stuck" matches must not reset its counter and hide a genuine
/// repeated failure.
#[tokio::test]
async fn test_requeue_leaves_matches_that_still_have_budget_alone() {
    // Requeue is gated on admin.demos.manage; grant it before acting.
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_302).await;
    let id = seed_match(
        &app,
        tracking,
        &format!("CSGO-budget-left-{}", unique_suffix()),
    )
    .await;

    fail_one_attempt(&app, id, &key, "GC timeout").await;

    let response = app
        .post_json(
            "/v1/admin/pipeline/discovered-matches/requeue",
            &json!({ "only_exhausted": true }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["requeued"], 0);

    assert_eq!(
        retry_state(&app, id).await.retry_count,
        1,
        "a match with budget left keeps its attempt count"
    );
}

/// The single-row control the failure list offers per match.
#[tokio::test]
async fn test_requeue_one_clears_the_error_and_restores_the_budget() {
    // Requeue is gated on admin.demos.manage; grant it before acting.
    let app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let key = create_enricher_key(app.pool()).await;
    let tracking = seed_tracking(&app, 76_561_198_000_000_303).await;
    let id = seed_match(
        &app,
        tracking,
        &format!("CSGO-requeue-one-{}", unique_suffix()),
    )
    .await;

    fail_one_attempt(&app, id, &key, "Trying to work with closed connection").await;

    let response = app
        .post_auth(&format!(
            "/v1/admin/pipeline/discovered-matches/{id}/requeue"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "pending");
    assert!(body["data"]["error"].is_null());

    let state = retry_state(&app, id).await;
    assert_eq!(state.retry_count, 0);
    assert!(
        pending_ids(&app, &key).await.contains(&id.to_string()),
        "requeued immediately, with no backoff left to serve"
    );
}

/// Requeue is an admin control, not something the enricher's own API key can
/// reach.
#[tokio::test]
async fn test_requeue_requires_admin() {
    let app = TestApp::new().await;

    let user = UserBuilder::new()
        .username(format!("nonadmin_{}", unique_suffix()))
        .email(format!("nonadmin-{}@example.com", unique_suffix()))
        .build_persisted(app.pool())
        .await;
    let token =
        portal_domain::generate_access_token(user.id, user.id, "nonadmin", "test-jwt-secret")
            .expect("token");

    app.post_json_with_token(
        "/v1/admin/pipeline/discovered-matches/requeue",
        &json!({ "only_exhausted": true }),
        &token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
}
