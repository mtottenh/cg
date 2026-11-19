//! MatchZy demo upload attempts (docs/demo-ingestion-admin-design.md §4):
//! every exit of `POST /v1/gameserver/demos` records exactly one row, and
//! the Pipeline / Game Servers reads built on them.

use crate::common::TestApp;
use crate::game_server_flow::{
    get_game_id_str, seed_allocated_reservation, seed_available_server, state_for,
};
use crate::tournaments::create_tournament_with_matches;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use portal_api::background::{LifecycleConfig, run_lifecycle_pass};
use portal_api::state::AppState;
use portal_core::ids::GameServerId;
use portal_domain::entities::ServerReservation;
use portal_domain::repositories::ServerReservationRepository;
use portal_domain::services::game_server::hash_token;
use portal_test::prelude::*;
use serde_json::Value;

const DEMO_TOKEN: &str = "cgm_demo_token_attempts";

/// A tournament match on a server with a demo token, reservation allocated.
struct Fixture {
    app: TestApp,
    state: AppState,
    server_id: GameServerId,
    reservation: ServerReservation,
    tournament_id: String,
    match_id: String,
}

async fn fixture(slug: &str) -> Fixture {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _r1, _r2) = create_tournament_with_matches(&app, slug).await;
    let game_id = get_game_id_str(&app).await;
    let server_id = seed_available_server(&state, &game_id, "Upload box").await;
    let (reservation, _token) =
        seed_allocated_reservation(&state, &match_id, &tournament_id, &game_id).await;
    state
        .game_server_registry
        .set_demo_token(server_id, &hash_token(DEMO_TOKEN))
        .await
        .unwrap();
    Fixture {
        app,
        state,
        server_id,
        reservation,
        tournament_id,
        match_id,
    }
}

async fn upload(app: &TestApp, body: &[u8], headers: &[(&str, &str)]) -> StatusCode {
    app.post_bytes_with_headers("/v1/gameserver/demos", body.to_vec(), headers)
        .await
}

fn bearer() -> String {
    format!("Bearer {DEMO_TOKEN}")
}

async fn attempt_count(app: &TestApp) -> i64 {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM demo_upload_attempts")
        .fetch_one(app.pool())
        .await
        .unwrap();
    n
}

/// The newest attempt, as the admin list returns it.
async fn latest_attempt(app: &TestApp) -> Value {
    let response = app.get_auth("/v1/admin/pipeline/uploads?limit=1").await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"]["attempts"][0].clone()
}

async fn overview_uploads(app: &TestApp) -> Value {
    let response = app.get_auth("/v1/admin/pipeline/overview").await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"]["server_uploads"].clone()
}

#[tokio::test]
async fn test_every_exit_records_exactly_one_attempt() {
    let f = fixture("upload-attempts").await;
    let app = &f.app;
    let mid = f.reservation.matchzy_id.to_string();
    let auth = bearer();
    let server_id = f.server_id.to_string();

    // Unknown token → 401, recorded without a server.
    let status = upload(
        app,
        b"DEMOBYTES",
        &[
            ("authorization", "Bearer cgm_wrong"),
            ("matchzy-filename", "a.dem"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(attempt_count(app).await, 1);
    let row = latest_attempt(app).await;
    assert_eq!(row["outcome"], "rejected_auth");
    assert_eq!(row["http_status"], 401);
    assert!(row["server_id"].is_null());
    assert_eq!(row["file_name"], "a.dem");
    assert_eq!(row["bytes"], 9);

    // Missing bearer entirely → also rejected_auth.
    let status = upload(app, b"DEMOBYTES", &[("matchzy-filename", "a.dem")]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(attempt_count(app).await, 2);
    assert_eq!(latest_attempt(app).await["outcome"], "rejected_auth");

    // Missing filename → 400 rejected_request, server known.
    let status = upload(app, b"DEMOBYTES", &[("authorization", &auth)]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(attempt_count(app).await, 3);
    let row = latest_attempt(app).await;
    assert_eq!(row["outcome"], "rejected_request");
    assert_eq!(row["http_status"], 400);
    assert_eq!(row["server_id"], server_id);
    assert_eq!(row["server_name"], "Upload box");
    assert!(row["file_name"].is_null());
    assert!(row["error"].as_str().unwrap().contains("FileName"));

    // Empty body → 400 rejected_request.
    let status = upload(
        app,
        b"",
        &[("authorization", &auth), ("matchzy-filename", "empty.dem")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(attempt_count(app).await, 4);
    let row = latest_attempt(app).await;
    assert_eq!(row["outcome"], "rejected_request");
    assert_eq!(row["bytes"], 0);

    // Success with a match id → stored + linked.
    let headers = [
        ("authorization", auth.as_str()),
        ("matchzy-filename", "series_map1.dem"),
        ("matchzy-matchid", mid.as_str()),
        ("matchzy-mapnumber", "0"),
    ];
    let status = upload(app, b"DEMOBYTES", &headers).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(attempt_count(app).await, 5);
    let row = latest_attempt(app).await;
    assert_eq!(row["outcome"], "stored");
    assert_eq!(row["http_status"], 200);
    assert_eq!(row["link_note"], "linked");
    assert_eq!(row["linked_match_id"], f.match_id);
    assert_eq!(row["linked_tournament_id"], f.tournament_id);
    assert!(row["linked_tournament_slug"].is_string());
    assert_eq!(row["matchzy_id"], f.reservation.matchzy_id);
    assert_eq!(row["map_number"], 0);
    assert_eq!(row["demo_status"], "pending");
    assert!(row["duration_ms"].as_i64().unwrap() >= 0);
    assert!(row["error"].is_null());
    let demo_id = row["demo_id"].as_str().unwrap().to_string();

    // Replay of the same upload → still linked, a second stored row.
    let status = upload(app, b"DEMOBYTES", &headers).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(attempt_count(app).await, 6);
    let row = latest_attempt(app).await;
    assert_eq!(row["outcome"], "stored");
    assert_eq!(row["link_note"], "linked");
    assert_eq!(row["demo_id"], demo_id);
    assert_eq!(row["linked_match_id"], f.match_id);

    // No match id header → stored, no_match_id.
    let status = upload(
        app,
        b"DEMOBYTES",
        &[("authorization", &auth), ("matchzy-filename", "loose.dem")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(attempt_count(app).await, 7);
    let row = latest_attempt(app).await;
    assert_eq!(row["outcome"], "stored");
    assert_eq!(row["link_note"], "no_match_id");
    assert!(row["linked_match_id"].is_null());

    // A match id no reservation carries → no_reservation.
    let status = upload(
        app,
        b"DEMOBYTES",
        &[
            ("authorization", &auth),
            ("matchzy-filename", "orphan.dem"),
            ("matchzy-matchid", "987654321"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(attempt_count(app).await, 8);
    assert_eq!(latest_attempt(app).await["link_note"], "no_reservation");

    // Metrics/overview agree with the rows.
    let uploads = overview_uploads(app).await;
    assert_eq!(uploads["uploaded_24h"], 4);
    assert_eq!(uploads["refused_24h"], 4);
    assert_eq!(uploads["failed_24h"], 0);
    assert_eq!(uploads["missing_after_minutes"], 30);
    assert_eq!(uploads["stuck_after_minutes"], 60);
    assert!(uploads["last_upload_at"].is_string());

    // The uploaded demo reports its source; a scanned one does not.
    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["source"], "matchzy_upload");
    let scanned = DemoBuilder::new()
        .s3_key("demos/2026/scanned.dem")
        .build_persisted(app.pool())
        .await;
    let response = app.get_auth(&format!("/v1/demos/{}", scanned.id)).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["source"], "bucket_scan");
}

#[tokio::test]
async fn test_upload_from_another_server_is_not_linked() {
    let f = fixture("upload-other-server").await;
    let game_id = get_game_id_str(&f.app).await;
    let other = seed_available_server(&f.state, &game_id, "Other box").await;
    f.state
        .game_server_registry
        .set_demo_token(other, &hash_token("cgm_other_token"))
        .await
        .unwrap();

    let mid = f.reservation.matchzy_id.to_string();
    let status = upload(
        &f.app,
        b"DEMOBYTES",
        &[
            ("authorization", "Bearer cgm_other_token"),
            ("matchzy-filename", "stolen.dem"),
            ("matchzy-matchid", &mid),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let row = latest_attempt(&f.app).await;
    assert_eq!(row["outcome"], "stored");
    assert_eq!(row["link_note"], "other_server");
    assert_eq!(row["server_id"], other.to_string());
    assert!(row["linked_match_id"].is_null());
}

#[tokio::test]
async fn test_uploads_list_filters_and_keyset() {
    let f = fixture("upload-list").await;
    let app = &f.app;
    let auth = bearer();

    for i in 0..3 {
        let name = format!("d{i}.dem");
        let status = upload(
            app,
            b"DEMOBYTES",
            &[("authorization", &auth), ("matchzy-filename", &name)],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    for _ in 0..2 {
        upload(app, b"X", &[("authorization", "Bearer nope")]).await;
    }

    // Outcome filter.
    let response = app
        .get_auth("/v1/admin/pipeline/uploads?outcome=rejected_auth")
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let rows = body["data"]["attempts"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["outcome"] == "rejected_auth"));
    assert!(body["data"]["next_before"].is_null(), "short page ends");

    // Server filter.
    let response = app
        .get_auth(&format!(
            "/v1/admin/pipeline/uploads?server_id={}",
            f.server_id
        ))
        .await;
    let body: Value = response.json();
    let rows = body["data"]["attempts"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    let demo_id = rows[0]["demo_id"].as_str().unwrap().to_string();

    // Demo filter.
    let response = app
        .get_auth(&format!("/v1/admin/pipeline/uploads?demo_id={demo_id}"))
        .await;
    let body: Value = response.json();
    assert_eq!(body["data"]["attempts"].as_array().unwrap().len(), 1);

    // Unknown outcome → 400, not an empty "all clear" list.
    app.get_auth("/v1/admin/pipeline/uploads?outcome=bogus")
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // Keyset: pages of two walk all five rows once, newest first.
    let mut seen: Vec<String> = Vec::new();
    let mut uri = "/v1/admin/pipeline/uploads?limit=2".to_string();
    loop {
        let response = app.get_auth(&uri).await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();
        for r in body["data"]["attempts"].as_array().unwrap() {
            seen.push(r["id"].as_str().unwrap().to_string());
        }
        match body["data"]["next_before"].as_str() {
            Some(before) => {
                uri = format!(
                    "/v1/admin/pipeline/uploads?limit=2&before={}",
                    before.replace('+', "%2B")
                );
            }
            None => break,
        }
    }
    assert_eq!(seen.len(), 5);
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 5, "no row repeats across pages");
}

#[tokio::test]
async fn test_missing_demo_appears_after_threshold_and_clears_on_upload() {
    let f = fixture("upload-missing").await;
    let app = &f.app;

    // Completed 10 minutes ago: not missing yet.
    f.state
        .server_reservation_repo
        .mark_completed(f.reservation.id, Utc::now() - Duration::minutes(10))
        .await
        .unwrap();
    assert_eq!(overview_uploads(app).await["missing"], 0);
    let response = app.get_auth("/v1/admin/pipeline/uploads/missing").await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());

    // Completed 31 minutes ago: missing.
    let completed_at = Utc::now() - Duration::minutes(31);
    f.state
        .server_reservation_repo
        .mark_completed(f.reservation.id, completed_at)
        .await
        .unwrap();
    assert_eq!(overview_uploads(app).await["missing"], 1);
    let response = app.get_auth("/v1/admin/pipeline/uploads/missing").await;
    let body: Value = response.json();
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["reservation_id"], f.reservation.id.to_string());
    assert_eq!(row["match_id"], f.match_id);
    assert_eq!(row["tournament_id"], f.tournament_id);
    assert!(row["tournament_slug"].is_string());
    assert!(row["tournament_name"].is_string());
    assert_eq!(row["matchzy_id"], f.reservation.matchzy_id);
    assert_eq!(row["server_id"], f.server_id.to_string());
    assert_eq!(row["server_name"], "Upload box");
    let completed: chrono::DateTime<Utc> = row["completed_at"].as_str().unwrap().parse().unwrap();
    let expected: chrono::DateTime<Utc> = row["expected_by"].as_str().unwrap().parse().unwrap();
    assert_eq!(expected - completed, Duration::minutes(30));

    // Older than the 14-day lookback: history, not missing.
    f.state
        .server_reservation_repo
        .mark_completed(f.reservation.id, Utc::now() - Duration::days(15))
        .await
        .unwrap();
    assert_eq!(overview_uploads(app).await["missing"], 0);
    f.state
        .server_reservation_repo
        .mark_completed(f.reservation.id, completed_at)
        .await
        .unwrap();

    // The demo arrives (late) and links → no longer missing.
    let mid = f.reservation.matchzy_id.to_string();
    let auth = bearer();
    let status = upload(
        app,
        b"DEMOBYTES",
        &[
            ("authorization", &auth),
            ("matchzy-filename", "late.dem"),
            ("matchzy-matchid", &mid),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest_attempt(app).await["link_note"], "linked");
    assert_eq!(overview_uploads(app).await["missing"], 0);
    let response = app.get_auth("/v1/admin/pipeline/uploads/missing").await;
    let body: Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_stuck_pending_counts_old_uploaded_demos_only() {
    let f = fixture("upload-stuck").await;
    let app = &f.app;
    let auth = bearer();
    let status = upload(
        app,
        b"DEMOBYTES",
        &[("authorization", &auth), ("matchzy-filename", "slow.dem")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview_uploads(app).await["stuck_pending"], 0);

    // A scanned demo pending for hours is not the server route's problem.
    DemoBuilder::new()
        .s3_key("demos/old_scan.dem")
        .pending()
        .build_persisted(app.pool())
        .await;
    sqlx::query("UPDATE demos SET discovered_at = NOW() - INTERVAL '61 minutes'")
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(overview_uploads(app).await["stuck_pending"], 1);

    sqlx::query("UPDATE demos SET status = 'ready' WHERE file_name = 'slow.dem'")
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(overview_uploads(app).await["stuck_pending"], 0);
}

#[tokio::test]
async fn test_game_server_response_demo_fields() {
    let f = fixture("upload-server-fields").await;
    let app = &f.app;
    let game_id = get_game_id_str(app).await;
    let tokenless = seed_available_server(&f.state, &game_id, "No token box").await;

    let server_json = |list: &Value, id: GameServerId| -> Value {
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id.to_string())
            .cloned()
            .unwrap()
    };

    let response = app.get_auth("/v1/admin/game-servers").await;
    response.assert_status(StatusCode::OK);
    let list: Value = response.json();
    let s = server_json(&list, f.server_id);
    assert_eq!(s["has_demo_token"], true);
    assert!(s["last_demo_upload_at"].is_null());
    assert!(s["last_demo_upload_outcome"].is_null());
    assert!(s.get("demo_token_hash").is_none(), "the hash never leaves");
    assert_eq!(server_json(&list, tokenless)["has_demo_token"], false);

    let auth = bearer();
    upload(
        app,
        b"DEMOBYTES",
        &[("authorization", &auth), ("matchzy-filename", "x.dem")],
    )
    .await;
    // Latest attempt of ANY outcome wins.
    upload(
        app,
        b"",
        &[("authorization", &auth), ("matchzy-filename", "y.dem")],
    )
    .await;

    let response = app.get_auth("/v1/admin/game-servers").await;
    let list: Value = response.json();
    let s = server_json(&list, f.server_id);
    assert_eq!(s["last_demo_upload_outcome"], "rejected_request");
    assert!(s["last_demo_upload_at"].is_string());
    assert!(server_json(&list, tokenless)["last_demo_upload_at"].is_null());

    let response = app
        .get_auth(&format!("/v1/admin/game-servers/{}", f.server_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["has_demo_token"], true);
    assert_eq!(body["data"]["last_demo_upload_outcome"], "rejected_request");

    let response = app
        .patch_json(
            &format!("/v1/admin/game-servers/{}", f.server_id),
            &serde_json::json!({ "region": "eu-test" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["last_demo_upload_outcome"], "rejected_request");
}

#[tokio::test]
async fn test_lifecycle_prunes_attempts_past_retention() {
    let f = fixture("upload-prune").await;
    let app = &f.app;
    upload(app, b"X", &[("authorization", "Bearer nope")]).await;
    upload(app, b"X", &[("authorization", "Bearer nope")]).await;
    sqlx::query(
        "UPDATE demo_upload_attempts SET received_at = NOW() - INTERVAL '91 days' \
         WHERE id = (SELECT id FROM demo_upload_attempts ORDER BY received_at LIMIT 1)",
    )
    .execute(app.pool())
    .await
    .unwrap();

    let summary = run_lifecycle_pass(&f.state, &LifecycleConfig::from_env(), false).await;
    assert_eq!(summary.demo_upload_attempts_pruned, 1);
    assert_eq!(attempt_count(app).await, 1);
}
