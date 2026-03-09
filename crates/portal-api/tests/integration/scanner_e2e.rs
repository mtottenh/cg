//! End-to-end tests for the portal-scanner daemon integrated with portal-api and MinIO.
//!
//! These tests verify the complete flow:
//! 1. S3 scan — scanner lists `.dem.bz2` file keys in MinIO (never downloads actual demos)
//! 2. Catalog — scanner registers S3 keys with portal API via `POST /v1/admin/demos/batch`
//! 3. Stats fetch — scanner calls a mock stats service to get pre-parsed `.stats.json`
//! 4. Stats submit — scanner sends parsed stats to portal API via `POST /v1/admin/demos/{id}/stats`

use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use portal_plugins::Cs2DemoClient;
use portal_scanner::api_client::PortalApiClient;
use portal_scanner::config::ScannerConfig;
use portal_scanner::{scanner, stats_converter};
use portal_test::prelude::*;
use tokio::sync::RwLock;

use crate::common::minio::{create_bucket_and_upload, create_s3_client, start_minio};
use crate::common::TestApp;

// ============================================================================
// TEST INFRASTRUCTURE
// ============================================================================

const DEMO_S3_KEY: &str =
    "demos/2024-09-14_20-17-30_9_de_inferno_team_Zan_vs_team_Maxymimi.dem.bz2";
const BUCKET_NAME: &str = "portal-demos";

/// Load the trimmed fixture JSON.
fn load_fixture() -> String {
    include_str!("../fixtures/demo_stats.json").to_string()
}

/// Start a mock stats HTTP server that serves fixture JSON.
///
/// The `Cs2DemoClient` fetches `{base_url}/stats/{name}.dem.stats.json`,
/// so we serve any `GET /stats/*` with the fixture.
///
/// `fixture_store` is shared so tests can swap between 404 and real responses.
async fn start_mock_stats_server(
    fixture_store: Arc<RwLock<Option<String>>>,
) -> SocketAddr {
    let app = axum::Router::new().route(
        "/stats/{*path}",
        axum::routing::get({
            let store = fixture_store.clone();
            move || {
                let store = store.clone();
                async move {
                    let guard = store.read().await;
                    match guard.as_ref() {
                        Some(json) => (
                            StatusCode::OK,
                            [("content-type", "application/json")],
                            json.clone(),
                        ),
                        None => (
                            StatusCode::NOT_FOUND,
                            [("content-type", "text/plain")],
                            "not found".to_string(),
                        ),
                    }
                }
            }
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

/// Grant admin role to the dev user.
async fn make_dev_user_admin(app: &TestApp) {
    let dev_user_id =
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
}

/// Create an API key in the database and return the raw key string.
///
/// The scanner endpoints use `X-API-Key` header + SHA-256 hash lookup,
/// so we need a real API key in the `api_keys` table with permissions
/// granted via the `api_key_permissions` join table.
async fn create_test_api_key(pool: &portal_db::DbPool) -> String {
    use portal_api::extractors::api_key::hash_api_key;

    let raw_key = format!("test-scanner-key-{}", uuid::Uuid::now_v7());
    let key_hash = hash_api_key(&raw_key);
    let key_prefix = &raw_key[..8.min(raw_key.len())];

    // Insert the API key
    let row: (uuid::Uuid,) = sqlx::query_as(
        r"INSERT INTO api_keys (service_name, key_hash, key_prefix, is_active)
          VALUES ($1, $2, $3, true)
          RETURNING id"
    )
    .bind("test-scanner")
    .bind(&key_hash)
    .bind(key_prefix)
    .fetch_one(pool)
    .await
    .expect("Failed to create test API key");

    let api_key_id = row.0;

    // Grant permissions via the join table
    for perm in &["demos.catalog", "demos.read", "demos.stats"] {
        sqlx::query(
            r"INSERT INTO api_key_permissions (api_key_id, permission_id)
              SELECT $1, id FROM permissions WHERE name = $2"
        )
        .bind(api_key_id)
        .bind(perm)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("Failed to grant permission {perm}: {e}"));
    }

    raw_key
}

/// Build a `ScannerConfig` for tests.
fn build_scanner_config(
    api_url: &str,
    s3_bucket: &str,
    game_id: &str,
    api_key: &str,
) -> ScannerConfig {
    ScannerConfig {
        s3_bucket: s3_bucket.to_string(),
        s3_prefix: "demos/".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_endpoint: None, // Not used — we pass the S3 client directly
        api_url: api_url.to_string(),
        api_key: api_key.to_string(),
        demo_service_url: String::new(), // Not used — we pass the client directly
        game_id: game_id.to_string(),
        interval_secs: 60,
        processing_interval_secs: 60,
    }
}

// ============================================================================
// TESTS
// ============================================================================

/// Full E2E: S3 scan → catalog → stats fetch → verify via API.
#[tokio::test]
async fn test_scanner_e2e_full_flow() {
    let mut app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let api_key = create_test_api_key(app.pool()).await;

    // Start MinIO
    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket_and_upload(&s3_client, BUCKET_NAME, DEMO_S3_KEY).await;

    // Start mock stats server with fixture
    let fixture = load_fixture();
    let fixture_store = Arc::new(RwLock::new(Some(fixture)));
    let stats_addr = start_mock_stats_server(fixture_store).await;
    let demo_client = Cs2DemoClient::new(format!("http://{stats_addr}"));

    // Start real HTTP server for the scanner API client
    let api_addr = app.start_server().await;
    let api_url = format!("http://{api_addr}");
    let api_client = PortalApiClient::new(api_url.clone(), api_key.clone());

    let config = build_scanner_config(&api_url, BUCKET_NAME, &game_id.to_string(), &api_key);

    // Run scanner
    scanner::scan_and_process(&s3_client, &api_client, &demo_client, &config)
        .await
        .expect("scan_and_process failed");

    // Verify via API — list demos
    let response = app.get_auth("/v1/demos").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demos = body["data"]["demos"].as_array().unwrap();
    assert_eq!(demos.len(), 1, "Should have exactly 1 demo");

    let demo = &demos[0];
    assert_eq!(demo["status"], "ready");
    assert!(
        demo["file_name"]
            .as_str()
            .unwrap()
            .contains("de_inferno_team_Zan_vs_team_Maxymimi"),
    );

    // Verify metadata (map, scores are inside demo.metadata)
    let meta = &demo["metadata"];
    assert_eq!(meta["map_name"], "de_inferno");
    // Scores: team_Maxymimi 13, team_Zan 10 — ordering depends on HashMap iteration
    let t1_score = meta["team1_score"].as_i64().unwrap();
    let t2_score = meta["team2_score"].as_i64().unwrap();
    assert!(
        (t1_score == 13 && t2_score == 10) || (t1_score == 10 && t2_score == 13),
        "Scores should be 13-10 in some order, got {t1_score}-{t2_score}"
    );

    // Verify players
    let demo_id = demo["id"].as_str().unwrap();
    let response = app
        .get_auth(&format!("/v1/demos/{demo_id}/players"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let players = body["data"].as_array().unwrap();
    assert_eq!(players.len(), 10, "Should have 10 players");

    // Spot-check a known player (MiT — highest kills at 25)
    let mit = players
        .iter()
        .find(|p| p["steam_id"].as_str() == Some("76561199496416662"));
    assert!(mit.is_some(), "Player MiT should be in demo players");
    let mit = mit.unwrap();
    assert_eq!(mit["player_name"], "MiT");
    // Stats are typed — check kills via stats object
    assert_eq!(mit["stats"]["kills"].as_i64().unwrap(), 25);
}

/// Re-scanning the same S3 key doesn't duplicate demos.
#[tokio::test]
async fn test_scanner_e2e_idempotent() {
    let mut app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let api_key = create_test_api_key(app.pool()).await;

    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket_and_upload(&s3_client, BUCKET_NAME, DEMO_S3_KEY).await;

    let fixture = load_fixture();
    let fixture_store = Arc::new(RwLock::new(Some(fixture)));
    let stats_addr = start_mock_stats_server(fixture_store).await;
    let demo_client = Cs2DemoClient::new(format!("http://{stats_addr}"));

    let api_addr = app.start_server().await;
    let api_url = format!("http://{api_addr}");
    let api_client = PortalApiClient::new(api_url.clone(), api_key.clone());
    let config = build_scanner_config(&api_url, BUCKET_NAME, &game_id.to_string(), &api_key);

    // Run twice
    scanner::scan_and_process(&s3_client, &api_client, &demo_client, &config)
        .await
        .expect("first scan failed");
    scanner::scan_and_process(&s3_client, &api_client, &demo_client, &config)
        .await
        .expect("second scan failed");

    // Still only 1 demo
    let response = app.get_auth("/v1/demos").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demos = body["data"]["demos"].as_array().unwrap();
    assert_eq!(demos.len(), 1, "Should still have exactly 1 demo after re-scan");
}

/// Pending retry: stats unavailable initially → catalog as pending → retry succeeds.
#[tokio::test]
async fn test_scanner_e2e_process_pending_retries() {
    let mut app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let api_key = create_test_api_key(app.pool()).await;

    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket_and_upload(&s3_client, BUCKET_NAME, DEMO_S3_KEY).await;

    // Start mock stats server returning 404 initially
    let fixture_store = Arc::new(RwLock::new(None));
    let stats_addr = start_mock_stats_server(fixture_store.clone()).await;
    let demo_client = Cs2DemoClient::new(format!("http://{stats_addr}"));

    let api_addr = app.start_server().await;
    let api_url = format!("http://{api_addr}");
    let api_client = PortalApiClient::new(api_url.clone(), api_key.clone());
    let config = build_scanner_config(&api_url, BUCKET_NAME, &game_id.to_string(), &api_key);

    // First scan — stats not available, demo gets cataloged but stays pending/failed
    scanner::scan_and_process(&s3_client, &api_client, &demo_client, &config)
        .await
        .expect("first scan failed");

    // Verify demo exists but not ready
    let response = app.get_auth("/v1/demos").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demos = body["data"]["demos"].as_array().unwrap();
    assert_eq!(demos.len(), 1);
    let status = demos[0]["status"].as_str().unwrap();
    assert_ne!(status, "ready", "Demo should not be ready without stats");

    // Now enable stats
    {
        let mut store = fixture_store.write().await;
        *store = Some(load_fixture());
    }

    // Process pending — should retry and succeed
    scanner::process_pending(&api_client, &demo_client)
        .await
        .expect("process_pending failed");

    // Verify demo is now ready
    let response = app.get_auth("/v1/demos").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demos = body["data"]["demos"].as_array().unwrap();
    assert_eq!(demos.len(), 1);
    assert_eq!(demos[0]["status"], "ready");
    assert_eq!(demos[0]["metadata"]["map_name"], "de_inferno");
}

/// Unit test: stats_converter with real fixture data.
#[tokio::test]
async fn test_stats_converter_with_real_fixture() {
    let fixture_json = load_fixture();
    let stats: portal_plugins::Cs2DemoStats =
        serde_json::from_str(&fixture_json).expect("Failed to parse fixture as Cs2DemoStats");

    let request = stats_converter::convert_stats(&stats);

    // Map
    assert_eq!(request.map_name.as_deref(), Some("de_inferno"));

    // Teams and scores
    let t1 = request.team1_name.as_deref().unwrap();
    let t2 = request.team2_name.as_deref().unwrap();
    let s1 = request.team1_score.unwrap();
    let s2 = request.team2_score.unwrap();

    // One team is team_Maxymimi (13), the other is team_Zan (10)
    if t1 == "team_Maxymimi" {
        assert_eq!(s1, 13);
        assert_eq!(t2, "team_Zan");
        assert_eq!(s2, 10);
    } else {
        assert_eq!(t1, "team_Zan");
        assert_eq!(s1, 10);
        assert_eq!(t2, "team_Maxymimi");
        assert_eq!(s2, 13);
    }

    // 10 players
    assert_eq!(request.players.len(), 10);

    // All players have non-zero kills
    for player in &request.players {
        let kills = player.stats["kills"].as_i64().unwrap();
        assert!(kills > 0, "Player {} should have kills > 0", player.player_name);
    }

    // Top fragger check (MiT with 25 kills)
    let mit = request
        .players
        .iter()
        .find(|p| p.steam_id == "76561199496416662")
        .expect("MiT should be in players");
    assert_eq!(mit.player_name, "MiT");
    assert_eq!(mit.stats["kills"], 25);
    assert_eq!(mit.stats["deaths"], 14);
}

/// Batch processing: 3 different demos in one scan cycle.
#[tokio::test]
async fn test_scanner_e2e_multiple_demos() {
    let mut app = TestApp::new().await;
    make_dev_user_admin(&app).await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let api_key = create_test_api_key(app.pool()).await;

    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;

    // Create bucket and upload 3 different stubs
    s3_client
        .create_bucket()
        .bucket(BUCKET_NAME)
        .send()
        .await
        .expect("Failed to create bucket");

    for name in &[
        "demos/match_001.dem.bz2",
        "demos/match_002.dem.bz2",
        "demos/match_003.dem.bz2",
    ] {
        s3_client
            .put_object()
            .bucket(BUCKET_NAME)
            .key(*name)
            .body(aws_sdk_s3::primitives::ByteStream::from_static(b"x"))
            .send()
            .await
            .expect("Failed to upload stub");
    }

    // Mock stats server serves fixture for all lookups
    let fixture = load_fixture();
    let fixture_store = Arc::new(RwLock::new(Some(fixture)));
    let stats_addr = start_mock_stats_server(fixture_store).await;
    let demo_client = Cs2DemoClient::new(format!("http://{stats_addr}"));

    let api_addr = app.start_server().await;
    let api_url = format!("http://{api_addr}");
    let api_client = PortalApiClient::new(api_url.clone(), api_key.clone());
    let config = build_scanner_config(&api_url, BUCKET_NAME, &game_id.to_string(), &api_key);

    scanner::scan_and_process(&s3_client, &api_client, &demo_client, &config)
        .await
        .expect("scan_and_process failed");

    // Verify 3 demos in catalog
    let response = app.get_auth("/v1/demos").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let demos = body["data"]["demos"].as_array().unwrap();
    assert_eq!(demos.len(), 3, "Should have 3 demos");

    // All should be ready
    for demo in demos {
        assert_eq!(demo["status"], "ready", "All demos should be ready");
    }
}
