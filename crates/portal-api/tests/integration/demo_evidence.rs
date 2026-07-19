//! CS2 demo-evidence and internal scanner-ingestion integration tests.
//!
//! Covers the two launch-critical surfaces that previously had no HTTP
//! coverage:
//!
//! 1. `/v1/matches/{id}/evidence/validate-demo` + `demo-stats/{name}` — the
//!    handler → plugin → external stats fetch → validator path, against a
//!    local mock stats server serving the committed fixture.
//! 2. `/v1/internal/demos/*` — the real service boundary the portal-scanner
//!    daemon calls (the scanner_e2e tests exercise the admin routes instead).

use crate::common::{TestApp, TestResponse};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use portal_test::prelude::*;
use serde_json::json;
use std::net::SocketAddr;
use tower::util::ServiceExt;

// ============================================================================
// MOCK STATS SERVER
// ============================================================================

/// Fixture facts (see tests/fixtures/demo_stats.json): de_inferno,
/// team_Zan 10 : 13 team_Maxymimi.
const DEMO_NAME: &str = "2024-09-14_20-17-30_9_de_inferno_team_Zan_vs_team_Maxymimi.dem";
const TEAM_ZAN_STEAM_IDS: &str = "76561197969684583,76561197985524918,76561198019332496";
const TEAM_MAXYMIMI_STEAM_IDS: &str = "76561197962015608,76561197968706174,76561198074283173";

/// Serve `GET /stats/*` with the committed fixture JSON.
async fn start_mock_stats_server() -> SocketAddr {
    let app = axum::Router::new().route(
        "/stats/{*path}",
        axum::routing::get(|| async {
            (
                StatusCode::OK,
                [("content-type", "application/json")],
                include_str!("../fixtures/demo_stats.json"),
            )
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind mock stats server");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

// ============================================================================
// EVIDENCE VALIDATE-DEMO / DEMO-STATS
// ============================================================================

#[tokio::test]
async fn test_validate_demo_happy_path_confirms_matching_claim() {
    let stats_addr = start_mock_stats_server().await;
    let app = TestApp::new_with_demo_service(&format!("http://{stats_addr}")).await;

    // Claim matches the fixture exactly: Zan (participant 1) 10, Maxymimi 13.
    let response = app
        .post_json(
            &format!(
                "/v1/matches/{}/evidence/validate-demo?participant1_steam_ids={}&participant2_steam_ids={}",
                uuid::Uuid::now_v7(),
                TEAM_ZAN_STEAM_IDS,
                TEAM_MAXYMIMI_STEAM_IDS
            ),
            &json!({
                "demo_name": DEMO_NAME,
                "map_id": "de_inferno",
                "participant1_score": 10,
                "participant2_score": 13
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["is_valid"], true,
        "matching claim should validate: {body}"
    );
    assert!(body["data"]["confidence"].as_f64().unwrap() > 0.5);
    let extracted = &body["data"]["extracted_result"];
    assert_eq!(extracted["map_id"], "de_inferno");
    assert!(body["data"]["stats_url"].as_str().unwrap().contains("stats"));
}

#[tokio::test]
async fn test_validate_demo_rejects_mismatched_score() {
    let stats_addr = start_mock_stats_server().await;
    let app = TestApp::new_with_demo_service(&format!("http://{stats_addr}")).await;

    // Claimed scores are inverted relative to the demo.
    let response = app
        .post_json(
            &format!(
                "/v1/matches/{}/evidence/validate-demo?participant1_steam_ids={}&participant2_steam_ids={}",
                uuid::Uuid::now_v7(),
                TEAM_ZAN_STEAM_IDS,
                TEAM_MAXYMIMI_STEAM_IDS
            ),
            &json!({
                "demo_name": DEMO_NAME,
                "map_id": "de_inferno",
                "participant1_score": 13,
                "participant2_score": 10
            }),
        )
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["is_valid"], false,
        "inverted score claim must not validate: {body}"
    );
    assert!(
        !body["data"]["errors"].as_array().unwrap().is_empty(),
        "score mismatch should produce errors"
    );
}

#[tokio::test]
async fn test_get_demo_stats_proxies_external_service() {
    let stats_addr = start_mock_stats_server().await;
    let app = TestApp::new_with_demo_service(&format!("http://{stats_addr}")).await;

    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence/demo-stats/{}",
            uuid::Uuid::now_v7(),
            DEMO_NAME
        ))
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["map_name"], "de_inferno");
    assert_eq!(body["data"]["team1_score"].as_i64().unwrap() + body["data"]["team2_score"].as_i64().unwrap(), 23);
}

// ============================================================================
// INTERNAL SCANNER ENDPOINTS (/v1/internal/demos/*)
// ============================================================================

/// Create an API key with the scanner's demo permissions; returns the raw key.
async fn create_scanner_api_key(pool: &portal_db::DbPool) -> String {
    use portal_api::extractors::api_key::hash_api_key;

    let raw_key = format!("cgp_test{}", uuid::Uuid::now_v7().simple());
    let key_hash = hash_api_key(&raw_key);
    let key_prefix = &raw_key[..8];

    let (key_id,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO api_keys (service_name, key_hash, key_prefix, is_active)
         VALUES ('test-internal-scanner', $1, $2, true) RETURNING id",
    )
    .bind(&key_hash)
    .bind(key_prefix)
    .fetch_one(pool)
    .await
    .expect("Failed to create API key");

    sqlx::query(
        "INSERT INTO api_key_permissions (api_key_id, permission_id)
         SELECT $1, id FROM permissions WHERE name = ANY($2)",
    )
    .bind(key_id)
    .bind(["demos.catalog", "demos.read", "demos.stats"])
    .execute(pool)
    .await
    .expect("Failed to grant permissions");

    raw_key
}

async fn api_key_request(
    app: &TestApp,
    method: &str,
    uri: &str,
    body: Option<&serde_json::Value>,
    api_key: &str,
) -> TestResponse {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("X-API-Key", api_key);
    let request = match body {
        Some(json_body) => {
            builder = builder.header("Content-Type", "application/json");
            builder
                .body(Body::from(serde_json::to_string(json_body).unwrap()))
                .unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };

    let response = app
        .app
        .clone()
        .oneshot(request)
        .await
        .expect("request failed");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    TestResponse { status, body }
}

#[tokio::test]
async fn test_internal_demo_pipeline_catalog_pending_stats() {
    let app = TestApp::new().await;
    let api_key = create_scanner_api_key(app.pool()).await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // 1. Batch-catalog a demo (the scanner's first call)
    let response = api_key_request(
        &app,
        "POST",
        "/v1/internal/demos/batch",
        Some(&json!({
            "game_id": game_id,
            "demos": [{
                "file_name": DEMO_NAME,
                "s3_bucket": "portal-demos",
                "s3_key": format!("demos/{DEMO_NAME}.bz2"),
                "file_size_bytes": 12345678
            }]
        })),
        &api_key,
    )
    .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["created"].as_array().unwrap().len(), 1);
    let demo_id = body["data"]["created"][0]["id"].as_str().unwrap().to_string();

    // Re-cataloging the same key is idempotent: existing, not created.
    let response = api_key_request(
        &app,
        "POST",
        "/v1/internal/demos/batch",
        Some(&json!({
            "game_id": game_id,
            "demos": [{
                "file_name": DEMO_NAME,
                "s3_bucket": "portal-demos",
                "s3_key": format!("demos/{DEMO_NAME}.bz2"),
                "file_size_bytes": 12345678
            }]
        })),
        &api_key,
    )
    .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["created"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["existing"].as_array().unwrap().len(), 1);

    // 2. The demo shows up as pending (the scanner's retry poll)
    let response =
        api_key_request(&app, "GET", "/v1/internal/demos/pending", None, &api_key).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let pending = body["data"].as_array().unwrap();
    assert!(
        pending.iter().any(|d| d["id"] == demo_id.as_str()),
        "cataloged demo should be pending: {body}"
    );

    // 3. Submit parsed stats (the scanner's final call)
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/demo_stats.json")).unwrap();
    let response = api_key_request(
        &app,
        "POST",
        &format!("/v1/internal/demos/{demo_id}/stats"),
        Some(&json!({
            "map_name": "de_inferno",
            "team1_name": "team_Zan",
            "team2_name": "team_Maxymimi",
            "team1_score": 10,
            "team2_score": 13,
            "raw_stats": fixture,
            "players": [
                {
                    "steam_id": "76561197969684583",
                    "player_name": "zan_player",
                    "team_name": "team_Zan",
                    "stats": {"kills": 20, "deaths": 15, "assists": 5}
                },
                {
                    "steam_id": "76561197962015608",
                    "player_name": "maxy_player",
                    "team_name": "team_Maxymimi",
                    "stats": {"kills": 25, "deaths": 12, "assists": 3}
                }
            ]
        })),
        &api_key,
    )
    .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "ready");
    assert_eq!(body["data"]["metadata"]["map_name"], "de_inferno");

    // 4. Stats landed: submitted players visible via the public API
    let response = app.get_auth(&format!("/v1/demos/{demo_id}/players")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        2,
        "both submitted players should be stored: {body}"
    );

    // 5. No longer pending
    let response =
        api_key_request(&app, "GET", "/v1/internal/demos/pending", None, &api_key).await;
    let body: serde_json::Value = response.json();
    assert!(
        !body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == demo_id.as_str())
    );
}

#[tokio::test]
async fn test_internal_demo_stats_failed_marks_demo() {
    let app = TestApp::new().await;
    let api_key = create_scanner_api_key(app.pool()).await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = api_key_request(
        &app,
        "POST",
        "/v1/internal/demos/batch",
        Some(&json!({
            "game_id": game_id,
            "demos": [{
                "file_name": "broken.dem",
                "s3_bucket": "portal-demos",
                "s3_key": "demos/broken.dem.bz2",
                "file_size_bytes": 1
            }]
        })),
        &api_key,
    )
    .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demo_id = body["data"]["created"][0]["id"].as_str().unwrap().to_string();

    let response = api_key_request(
        &app,
        "POST",
        &format!("/v1/internal/demos/{demo_id}/stats-failed"),
        Some(&json!({ "error": "parser exploded" })),
        &api_key,
    )
    .await;
    response.assert_status(StatusCode::OK);

    let response = app.get_auth(&format!("/v1/demos/{demo_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "failed");
}

#[tokio::test]
async fn test_internal_demos_require_api_key() {
    let app = TestApp::new().await;

    // No key at all
    let response = api_key_request(&app, "GET", "/v1/internal/demos/pending", None, "").await;
    assert!(
        response.status == StatusCode::UNAUTHORIZED || response.status == StatusCode::FORBIDDEN,
        "expected 401/403 without key, got {}",
        response.status
    );

    // Key without the demos permissions
    let api_key = {
        use portal_api::extractors::api_key::hash_api_key;
        let raw_key = format!("cgp_test{}", uuid::Uuid::now_v7().simple());
        let key_hash = hash_api_key(&raw_key);
        sqlx::query(
            "INSERT INTO api_keys (service_name, key_hash, key_prefix, is_active)
             VALUES ('test-unprivileged', $1, $2, true)",
        )
        .bind(&key_hash)
        .bind(&raw_key[..8])
        .execute(app.pool())
        .await
        .expect("insert key");
        raw_key
    };
    let response =
        api_key_request(&app, "GET", "/v1/internal/demos/pending", None, &api_key).await;
    response.assert_status(StatusCode::FORBIDDEN);
}
