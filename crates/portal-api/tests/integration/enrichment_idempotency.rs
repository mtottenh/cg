//! Enrichment-pipeline idempotency / partial-write tests (audit: demo
//! enrichment is a multi-write sequence with no transaction and, in the
//! aggregate case, an accumulative write with no match-scoped key).
//!
//! Each scenario below is a real bug in the CS2 demo-enrichment path:
//!
//!   handlers/internal.rs::submit_enriched
//!     -> discovered_match.mark_enriched   (status='enriched', NO status predicate)
//!     -> process_demo_ratings             (player_rating_history: bare INSERT)
//!     -> process_match_stats              (player_match_history: deduped;
//!                                          player_mm_stats: accumulate, keyed
//!                                          on the AGGREGATE row, NO match id)
//!
//! and, separately,
//!
//!   services/demo.rs::save_demo_stats
//!     -> demo_repo.update_stats           (status='ready' FIRST)
//!     -> player_repo.create_batch         (demo_players rows SECOND)
//!   demo_repo.find_pending_processing selects ONLY status='pending'.
//!
//! These tests are WRITTEN TO FAIL against today's code. Each fails by
//! asserting the correct value and observing the wrong one (not by 404 or
//! compile error). No fault-injection framework is used: partial failures are
//! either staged directly with sqlx or forced through a real column-overflow
//! constraint.

use crate::common::{TestApp, TestResponse};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use portal_api::extractors::api_key::hash_api_key;
use portal_api::state::AppState;
use portal_db::DbPool;
use portal_test::prelude::*;
use serde_json::json;
use tower::util::ServiceExt;
use uuid::Uuid;

const OWNER_STEAM_ID_64: i64 = 76561198012345678;

// ===========================================================================
// SHARED HELPERS (duplicated from steam_tracking.rs — that file must stay
// untouched, so the API-key plumbing is re-declared locally here).
// ===========================================================================

async fn create_test_api_key(pool: &DbPool, service_name: &str, permissions: &[&str]) -> String {
    let raw_key = format!("cgp_test{}", Uuid::now_v7().to_string().replace('-', ""));
    let key_hash = hash_api_key(&raw_key);
    let key_prefix = &raw_key[..8];

    let (key_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO api_keys (service_name, key_hash, key_prefix) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(service_name)
    .bind(&key_hash)
    .bind(key_prefix)
    .fetch_one(pool)
    .await
    .expect("Failed to create test API key");

    sqlx::query(
        "INSERT INTO api_key_permissions (api_key_id, permission_id) \
         SELECT $1, p.id FROM permissions p WHERE p.name = ANY($2)",
    )
    .bind(key_id)
    .bind(permissions)
    .execute(pool)
    .await
    .expect("Failed to link test API key permissions");

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

async fn api_key_post_json(
    app: &TestApp,
    uri: &str,
    body: &serde_json::Value,
    api_key: &str,
) -> TestResponse {
    raw_request(
        app,
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .body(Body::from(serde_json::to_string(body).unwrap()))
            .unwrap(),
    )
    .await
}

/// Create a user+player with `steam_id_64` set (so the enricher's
/// `find_by_steam_id_64` resolves them) and register steam tracking so the
/// poller has a `tracking_id` to submit a discovered match against.
///
/// Returns (player_id, tracking_id, account_id).
async fn setup_tracked_player(app: &TestApp, steam_id_64: i64) -> (Uuid, String, u32) {
    let user = UserBuilder::new().build_persisted(app.pool()).await;
    let player_id = user.id; // UserBuilder uses the same UUID for user and player

    sqlx::query("UPDATE players SET steam_id = $1, steam_id_64 = $2 WHERE id = $3")
        .bind(steam_id_64.to_string())
        .bind(steam_id_64)
        .bind(player_id)
        .execute(app.pool())
        .await
        .expect("Failed to set steam_id_64");

    let token = create_test_token(user.id, player_id, &user.username, TEST_JWT_SECRET);

    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({ "game_auth_code": "ABCD-EFGHI-JKLM", "game_slug": "cs2" }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let tracking_response = app
        .get_with_token("/v1/players/me/steam-tracking", &token)
        .await;
    let tracking_id = tracking_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let account_id = (steam_id_64 - 76_561_197_960_265_728) as u32;
    (player_id, tracking_id, account_id)
}

/// Submit a discovered match and claim it, returning the discovered-match id.
async fn submit_and_claim(
    app: &TestApp,
    tracking_id: &str,
    enricher_key: &str,
    share_code: &str,
) -> String {
    let submit = api_key_post_json(
        app,
        "/v1/internal/discovered-matches",
        &json!({
            "tracking_id": tracking_id,
            "game": "cs2",
            "matches": [{
                "share_code": share_code,
                "match_id": 999_888_777_i64,
                "outcome_id": 111_222_333_i64,
                "token": 42
            }]
        }),
        enricher_key,
    )
    .await;
    submit.assert_status(StatusCode::CREATED);
    let match_id = submit.json::<Vec<serde_json::Value>>()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    api_key_post_json(
        app,
        &format!("/v1/internal/discovered-matches/{match_id}/claim"),
        &json!({}),
        enricher_key,
    )
    .await
    .assert_status(StatusCode::OK);

    match_id
}

/// A `gc_data` payload as the enricher actually sends it: a JSON **array** of
/// MatchInfo (the handler reads `gc_data[0]`). One player on team 1, which wins
/// (team_scores `[16, 13]`).
fn gc_data_array(account_id: u32, kills: i64) -> serde_json::Value {
    json!([{
        "match_id": 999_888_777_i64,
        "map": "de_dust2",
        "team_scores": [16, 13],
        "match_time": "2026-01-01T00:00:00Z",
        "match_duration_secs": 2000,
        "players": [{
            "account_id": account_id,
            "team": 1,
            "kills": kills,
            "deaths": 10,
            "assists": 5,
            "score": 60,
            "headshots": 8,
            "mvps": 3,
            "entry_3k": 1,
            "entry_4k": 0,
            "entry_5k": 0
        }]
    }])
}

// ===========================================================================
// TEST 1 — Re-submitting an enrichment DOUBLE-COUNTS aggregate MM stats.
//
// player_mm_stats.accumulate_match_stats keys ON CONFLICT (player_id, game_id)
// — the aggregate row — and always does `+1` / `+EXCLUDED.*`, with NO
// discovered_match_id anywhere in the statement. Its sibling
// player_match_history dedupes on (player_id, discovered_match_id). And
// mark_enriched has no status predicate, so nothing rejects a second submit.
//
// Submit the SAME enrichment twice: history stays at 1 row (correct), but the
// aggregate is counted twice. history=1 vs aggregate=2 is the smoking gun.
// ===========================================================================

#[tokio::test]
async fn test_resubmit_enrichment_double_counts_mm_stats() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let (player_id, tracking_id, account_id) = setup_tracked_player(&app, OWNER_STEAM_ID_64).await;

    let key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    let match_id = submit_and_claim(
        &app,
        &tracking_id,
        &key,
        "CSGO-dbl01-dbl01-dbl01-dbl01-dbl01",
    )
    .await;

    let payload = json!({
        "gc_data": gc_data_array(account_id, 25),
        "demo_url": "http://replay.valve.net/730/dbl.dem.bz2"
    });

    // First submit.
    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/enriched"),
        &payload,
        &key,
    )
    .await
    .assert_status(StatusCode::OK);

    // Second submit of the identical enrichment (a retry / at-least-once
    // delivery). mark_enriched has no status predicate, so it is accepted.
    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/enriched"),
        &payload,
        &key,
    )
    .await
    .assert_status(StatusCode::OK);

    // player_match_history dedupes correctly: exactly one row for this match.
    let (history_rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM player_match_history WHERE player_id = $1 AND discovered_match_id = $2",
    )
    .bind(player_id)
    .bind(Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(
        history_rows, 1,
        "player_match_history must hold exactly one row per (player, match) — \
         it is deduped on (player_id, discovered_match_id)"
    );

    // The aggregate must reflect exactly ONE match. Today it reflects two,
    // because accumulate_match_stats has no match-scoped idempotency key.
    let (matches_played, kills): (i32, i32) = sqlx::query_as(
        "SELECT matches_played, kills FROM player_mm_stats WHERE player_id = $1 AND game_id = $2",
    )
    .bind(player_id)
    .bind(game_id)
    .fetch_one(app.pool())
    .await
    .unwrap();

    assert_eq!(
        matches_played, 1,
        "re-submitting the same enrichment must NOT double-count matches_played \
         (player_match_history says 1 match, the aggregate says {matches_played}); \
         accumulate_match_stats is keyed on (player_id, game_id) with no \
         discovered_match_id, so every re-delivery increments it again"
    );
    assert_eq!(
        kills, 25,
        "kills must reflect the single match's 25, not an accumulated {kills}"
    );
}

// ===========================================================================
// TEST 2 — Player rating history DUPLICATES on re-submit.
//
// process_demo_ratings appends to player_rating_history with a bare INSERT.
// Migration 0049 has no unique constraint and no discovered_match_id column,
// so re-delivering the same match's rank data writes a second, identical row —
// polluting data_points / AVG / median in get_rating_stats.
//
// Intended grain: one rating-history entry per player per enriched match.
// ===========================================================================

#[tokio::test]
async fn test_resubmit_enrichment_duplicates_rating_history() {
    let app = TestApp::new().await;
    let (player_id, tracking_id, account_id) = setup_tracked_player(&app, OWNER_STEAM_ID_64).await;

    let key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    let match_id = submit_and_claim(
        &app,
        &tracking_id,
        &key,
        "CSGO-rat01-rat01-rat01-rat01-rat01",
    )
    .await;

    // gc_data as an array so `recorded_at` derives deterministically from
    // match_time — both submits produce byte-identical rating rows.
    let payload = json!({
        "gc_data": gc_data_array(account_id, 25),
        "demo_url": "http://replay.valve.net/730/rat.dem.bz2",
        "player_ratings": [{
            "account_id": account_id,
            "rank_id": 15250,
            "rank_type_id": 11,   // Premier — the only type processed
            "wins": 42,
            "rank_change": 250.0
        }]
    });

    for _ in 0..2 {
        api_key_post_json(
            &app,
            &format!("/v1/internal/discovered-matches/{match_id}/enriched"),
            &payload,
            &key,
        )
        .await
        .assert_status(StatusCode::OK);
    }

    let (rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM player_rating_history \
         WHERE player_id = $1 AND source = 'demo_rank_update'",
    )
    .bind(player_id)
    .fetch_one(app.pool())
    .await
    .unwrap();

    assert_eq!(
        rows, 1,
        "re-delivering the same enriched match must leave exactly ONE \
         rating-history entry for the player; process_demo_ratings does a bare \
         INSERT with no unique key, so a retry duplicates it (observed {rows})"
    );
}

// ===========================================================================
// TEST 3 — Partial enrichment marks the match DONE and strands the rest.
//          (marker-before-effect — the LIVE data-loss bug.)
//
// submit_enriched writes status='enriched' FIRST (mark_enriched), THEN writes
// per-player stats, swallowing per-player failures as non-fatal while still
// returning 200. If a stats write fails, the match is already finalized as
// 'enriched' — so find_pending (status IN ('pending','failed')) will never
// retry it, and any players not yet written are lost forever.
//
// We force the stats write to fail through a REAL constraint: pre-seed the
// player's aggregate `kills` near i32::MAX so the `kills + EXCLUDED.kills`
// update in accumulate_match_stats overflows the INTEGER column. match_history
// for the player is written first (it succeeds); accumulate then aborts
// process_match_stats — exactly the mid-sequence failure the handler swallows.
// ===========================================================================

#[tokio::test]
async fn test_partial_enrichment_finalizes_match_and_strands_missing_stats() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let (player_id, tracking_id, account_id) = setup_tracked_player(&app, OWNER_STEAM_ID_64).await;

    let key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    let match_id = submit_and_claim(
        &app,
        &tracking_id,
        &key,
        "CSGO-par01-par01-par01-par01-par01",
    )
    .await;

    // Pre-seed the aggregate so the accumulate UPDATE overflows INTEGER kills.
    sqlx::query(
        "INSERT INTO player_mm_stats (player_id, game_id, matches_played, kills) \
         VALUES ($1, $2, 1, 2000000000)",
    )
    .bind(player_id)
    .bind(game_id)
    .execute(app.pool())
    .await
    .expect("staging the near-overflow aggregate must succeed");

    // Submit enrichment. process_match_stats writes the player's match_history
    // row (OK), then accumulate_match_stats overflows (2e9 + 2e9 > i32::MAX) →
    // Err → swallowed → still 200. mark_enriched already ran FIRST.
    let response = api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/enriched"),
        &json!({
            "gc_data": gc_data_array(account_id, 2_000_000_000_i64),
            "demo_url": "http://replay.valve.net/730/par.dem.bz2"
        }),
        &key,
    )
    .await;
    // The handler swallows the per-player failure and reports success.
    response.assert_status(StatusCode::OK);

    // Confirm the stats write really did fail (aggregate untouched at the seed
    // value; the +25/... would have applied on success).
    let (kills,): (i32,) =
        sqlx::query_as("SELECT kills FROM player_mm_stats WHERE player_id = $1 AND game_id = $2")
            .bind(player_id)
            .bind(game_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        kills, 2_000_000_000,
        "precondition: the accumulate write must have failed (aggregate unchanged)"
    );

    // The bug: despite the failed stats write, the match is finalized.
    let (status,): (String,) =
        sqlx::query_as("SELECT status::TEXT FROM discovered_matches WHERE id = $1")
            .bind(Uuid::parse_str(&match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();

    assert_ne!(
        status, "enriched",
        "a match whose per-player stats write failed must NOT be finalized as \
         'enriched' (marker-before-effect data loss): submit_enriched writes \
         status='enriched' BEFORE the stats and swallows the failure, so \
         find_pending (status IN ('pending','failed')) never retries it and the \
         missing stats are lost forever. Observed status={status}"
    );
}

// ===========================================================================
// TEST 5 — Retry after a PARTIAL accumulate applies stats EXACTLY once.
//          (history-gate / accumulate split-transaction residual.)
//
// process_match_stats writes the per-player `player_match_history` row (the
// ON CONFLICT DO NOTHING idempotency claim) and the accumulative
// `player_mm_stats` bump as SEPARATE autocommit statements. So when the
// history row commits but the accumulate then FAILS (integer overflow), a
// wave-4 `failed` mark drives a retry — but the retry sees the already-
// committed history row (is_new = FALSE) and SKIPS the accumulate forever.
// The player's aggregate is permanently under-counted: an under-count no
// further retry can repair.
//
// Two players in ONE gc payload, ordered [B, A]:
//   B  — succeeds on the first pass (history + accumulate).
//   A  — history row commits, then accumulate overflows i32 kills → the match
//        is marked `failed` (wave-4). A's aggregate is untouched.
// Then we clear A's overflow condition and RE-SUBMIT. The fix (history claim
// + accumulate in ONE tx) makes A's first-pass history insert roll back with
// the failed accumulate, so the retry re-claims (is_new = true) and applies
// A's stats exactly once — while B, whose history row survived, is skipped
// and NOT double-counted.
// ===========================================================================

const SECOND_STEAM_ID_64: i64 = 76561198087654321;

/// Two players on team 1 (which wins, `[16, 13]`), emitted in the given order.
fn gc_data_two_players(
    first_account: u32,
    first_kills: i64,
    second_account: u32,
    second_kills: i64,
) -> serde_json::Value {
    let mk = |account_id: u32, kills: i64| {
        json!({
            "account_id": account_id,
            "team": 1,
            "kills": kills,
            "deaths": 10,
            "assists": 5,
            "score": 60,
            "headshots": 8,
            "mvps": 3,
            "entry_3k": 1,
            "entry_4k": 0,
            "entry_5k": 0
        })
    };
    json!([{
        "match_id": 999_888_777_i64,
        "map": "de_dust2",
        "team_scores": [16, 13],
        "match_time": "2026-01-01T00:00:00Z",
        "match_duration_secs": 2000,
        "players": [mk(first_account, first_kills), mk(second_account, second_kills)]
    }])
}

#[tokio::test]
async fn test_retry_after_partial_accumulate_applies_stats_exactly_once() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await;

    // A: the player whose accumulate overflows on the first pass.
    let (player_a, tracking_a, account_a) = setup_tracked_player(&app, OWNER_STEAM_ID_64).await;
    // B: the player who succeeds on the first pass (processed FIRST).
    let (player_b, _tracking_b, account_b) = setup_tracked_player(&app, SECOND_STEAM_ID_64).await;

    let key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    let match_id = submit_and_claim(
        &app,
        &tracking_a,
        &key,
        "CSGO-rty01-rty01-rty01-rty01-rty01",
    )
    .await;

    // Pre-seed A's aggregate so the accumulate UPDATE overflows INTEGER kills
    // (2e9 seed + 2e9 match > i32::MAX).
    sqlx::query(
        "INSERT INTO player_mm_stats (player_id, game_id, matches_played, kills) \
         VALUES ($1, $2, 1, 2000000000)",
    )
    .bind(player_a)
    .bind(game_id)
    .execute(app.pool())
    .await
    .expect("staging the near-overflow aggregate must succeed");

    // Order the payload [B, A]: B commits fully, then A's accumulate overflows.
    let payload = json!({
        "gc_data": gc_data_two_players(account_b, 30, account_a, 2_000_000_000_i64),
        "demo_url": "http://replay.valve.net/730/rty.dem.bz2"
    });

    // First submit — A's accumulate fails; wave-4 marks the match `failed`.
    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/enriched"),
        &payload,
        &key,
    )
    .await
    .assert_status(StatusCode::OK);

    let (status,): (String,) =
        sqlx::query_as("SELECT status::TEXT FROM discovered_matches WHERE id = $1")
            .bind(Uuid::parse_str(&match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        status, "failed",
        "wave-4: a match whose stats write failed must be left 'failed' so \
         find_pending retries it (observed {status})"
    );

    // Clear A's overflow condition so the retry CAN succeed: drop the seed row
    // entirely, leaving a clean slate (no match counted yet).
    sqlx::query("DELETE FROM player_mm_stats WHERE player_id = $1 AND game_id = $2")
        .bind(player_a)
        .bind(game_id)
        .execute(app.pool())
        .await
        .expect("clearing the overflow seed must succeed");

    // RE-SUBMIT the identical enrichment (the enricher's retry of a `failed`).
    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/enriched"),
        &payload,
        &key,
    )
    .await
    .assert_status(StatusCode::OK);

    // A must now reflect the match EXACTLY once. Today the first pass committed
    // A's history row (autocommit) while the accumulate failed, so the retry
    // sees is_new=FALSE and skips the accumulate forever — A's aggregate is
    // missing (row was deleted, never re-created).
    let a_stats: Option<(i32, i32)> = sqlx::query_as(
        "SELECT matches_played, kills FROM player_mm_stats WHERE player_id = $1 AND game_id = $2",
    )
    .bind(player_a)
    .bind(game_id)
    .fetch_optional(app.pool())
    .await
    .unwrap();
    let (a_matches, a_kills) = a_stats.unwrap_or((0, 0));
    assert_eq!(
        a_matches, 1,
        "after clearing the overflow and retrying, player A's aggregate must \
         reflect the match exactly once; the committed-but-orphaned history row \
         from the failed first pass suppresses the retry's accumulate \
         (observed matches_played={a_matches})"
    );
    assert_eq!(
        a_kills, 2_000_000_000,
        "player A's kills must reflect the single match's value on retry \
         (observed {a_kills})"
    );

    // B succeeded on the FIRST pass and must NOT be double-counted by the retry
    // (its history row is present, so the retry's accumulate is correctly
    // skipped).
    let (b_matches, b_kills): (i32, i32) = sqlx::query_as(
        "SELECT matches_played, kills FROM player_mm_stats WHERE player_id = $1 AND game_id = $2",
    )
    .bind(player_b)
    .bind(game_id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(
        b_matches, 1,
        "player B (succeeded first pass) must be counted exactly once, not \
         double-counted by the retry (observed matches_played={b_matches})"
    );
    assert_eq!(
        b_kills, 30,
        "player B's kills must reflect the single match (observed {b_kills})"
    );
}

// ===========================================================================
// TEST 4 — A demo marked 'ready' before its stats rows exist is invisible
//          forever (marker-before-effect on the demo-processing side).
//
// save_demo_stats calls update_stats (status='ready') FIRST, then
// create_batch writes demo_players. find_pending_processing selects ONLY
// status='pending'. A demo left 'ready' with zero demo_players rows — the
// post-crash state — is therefore never re-selected. The same query also skips
// status='failed', so a transient stats-service 500 parks a demo permanently.
// ===========================================================================

#[tokio::test]
async fn test_ready_demo_without_stats_rows_is_stranded() {
    let app = TestApp::new().await;
    let state = AppState::new(app.pool().clone(), TEST_JWT_SECRET).await;

    // Post-crash state: update_stats landed (status='ready', stats present) but
    // create_batch never ran, so there are zero demo_players.
    let ready_demo = DemoBuilder::new()
        .ready()
        .stats_json(json!({ "player_summaries": {} }))
        .build_persisted(app.pool())
        .await;

    // A demo parked by a transient stats-service failure.
    let failed_demo = DemoBuilder::new()
        .failed()
        .build_persisted(app.pool())
        .await;

    // Precondition: the 'ready' demo genuinely has no player rows.
    let (player_rows,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM demo_players WHERE demo_id = $1")
            .bind(ready_demo.id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        player_rows, 0,
        "precondition: ready demo must have no players"
    );

    // The processing pipeline's selection query.
    let pending = state
        .demo_service
        .get_pending_demos(100)
        .await
        .expect("listing pending demos must succeed");

    assert!(
        pending.iter().any(|d| d.id.as_uuid() == ready_demo.id),
        "a demo left in status='ready' with zero demo_players rows (crash \
         between update_stats and create_batch) must be re-selected for \
         processing; find_pending_processing filters status='pending' only, so \
         it is invisible forever. status='ready' is set BEFORE the player rows \
         exist — the marker must be written AFTER the effect (or a recovery \
         pass must reclaim ready-but-empty demos)"
    );

    assert!(
        pending.iter().any(|d| d.id.as_uuid() == failed_demo.id),
        "a demo parked in status='failed' by a transient stats-service error \
         must also be retryable; find_pending_processing skips 'failed', so it \
         is parked permanently"
    );
}
