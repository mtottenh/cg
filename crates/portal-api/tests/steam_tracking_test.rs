//! End-to-end tests for steam tracking and discovered matches.
//!
//! Tests cover:
//! - User-facing steam tracking endpoints (JWT auth)
//! - Internal bot endpoints (API key auth)
//! - Full poller → enricher flow

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{TestApp, TestResponse};
use http_body_util::BodyExt;
use portal_api::extractors::api_key::hash_api_key;
use portal_db::DbPool;
use portal_test::builders::UserBuilder;
use portal_test::helpers::{create_test_token, TEST_JWT_SECRET};
use serde_json::json;
use tower::util::ServiceExt;
use uuid::Uuid;

// =============================================================================
// Test helpers
// =============================================================================

const TEST_STEAM_ID: &str = "76561198012345678";
const TEST_AUTH_CODE: &str = "ABCD-EFGHI-JKLM";

/// Create an API key in the database and return the raw key.
async fn create_test_api_key(pool: &DbPool, service_name: &str, permissions: &[&str]) -> String {
    let raw_key = format!(
        "cgp_test{}",
        Uuid::now_v7().to_string().replace('-', "")
    );
    let key_hash = hash_api_key(&raw_key);
    let key_prefix = &raw_key[..8];

    sqlx::query(
        "INSERT INTO api_keys (service_name, key_hash, key_prefix, permissions) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(service_name)
    .bind(&key_hash)
    .bind(key_prefix)
    .bind(permissions)
    .execute(pool)
    .await
    .expect("Failed to create test API key");

    raw_key
}

/// Create a user + player with a linked steam_id. Returns (user_id, player_id, jwt_token).
///
/// Note: `UserBuilder::build_persisted` creates both a user and a player with the same UUID.
/// We update the player's steam_id after creation.
async fn create_player_with_steam(pool: &DbPool) -> (Uuid, Uuid, String) {
    let user = UserBuilder::new().build_persisted(pool).await;
    let player_id = user.id; // UserBuilder uses the same UUID for user and player

    // Set steam_id on the auto-created player
    sqlx::query("UPDATE players SET steam_id = $1 WHERE id = $2")
        .bind(TEST_STEAM_ID)
        .bind(player_id)
        .execute(pool)
        .await
        .expect("Failed to set steam_id");

    let token = create_test_token(user.id, player_id, &user.username, TEST_JWT_SECRET);
    (user.id, player_id, token)
}

/// Create a user + player WITHOUT a linked steam_id. Returns (user_id, player_id, jwt_token).
async fn create_player_without_steam(pool: &DbPool) -> (Uuid, Uuid, String) {
    let user = UserBuilder::new().build_persisted(pool).await;
    let player_id = user.id;

    let token = create_test_token(user.id, player_id, &user.username, TEST_JWT_SECRET);
    (user.id, player_id, token)
}

// -- Raw request helpers for API key auth (TestApp only has JWT helpers) ------

async fn raw_request(app: &TestApp, req: Request<Body>) -> TestResponse {
    let response = app
        .app
        .clone()
        .oneshot(req)
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

async fn api_key_get(app: &TestApp, uri: &str, api_key: &str) -> TestResponse {
    raw_request(
        app,
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("X-API-Key", api_key)
            .body(Body::empty())
            .unwrap(),
    )
    .await
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

async fn api_key_patch_json(
    app: &TestApp,
    uri: &str,
    body: &serde_json::Value,
    api_key: &str,
) -> TestResponse {
    raw_request(
        app,
        Request::builder()
            .method("PATCH")
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("X-API-Key", api_key)
            .body(Body::from(serde_json::to_string(body).unwrap()))
            .unwrap(),
    )
    .await
}

// =============================================================================
// User-facing steam tracking tests (JWT auth)
// =============================================================================

#[tokio::test]
async fn test_register_steam_tracking() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;

    let response = app
        .post_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({
                "game_auth_code": TEST_AUTH_CODE,
                "game_slug": "cs2"
            }),
            &token,
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["steam_id_64"], TEST_STEAM_ID.parse::<i64>().unwrap());
    assert!(body["data"]["is_active"].as_bool().unwrap());
    // Auth code should be masked
    assert!(body["data"]["game_auth_code_prefix"].as_str().unwrap().contains("..."));
}

#[tokio::test]
async fn test_register_steam_tracking_no_steam_id() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_without_steam(app.pool()).await;

    let response = app
        .post_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({
                "game_auth_code": TEST_AUTH_CODE,
                "game_slug": "cs2"
            }),
            &token,
        )
        .await;

    // Should fail because player has no steam_id linked
    assert!(
        response.status == StatusCode::BAD_REQUEST
            || response.status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}. Body: {}",
        response.status,
        response.text()
    );
}

#[tokio::test]
async fn test_register_steam_tracking_invalid_auth_code() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;

    let response = app
        .post_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({
                "game_auth_code": "bad-code",
                "game_slug": "cs2"
            }),
            &token,
        )
        .await;

    assert!(
        response.status == StatusCode::BAD_REQUEST
            || response.status == StatusCode::UNPROCESSABLE_ENTITY,
        "Expected 400 or 422, got {}. Body: {}",
        response.status,
        response.text()
    );
}

#[tokio::test]
async fn test_register_steam_tracking_duplicate() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;

    // First registration — should succeed
    let response = app
        .post_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({
                "game_auth_code": TEST_AUTH_CODE,
                "game_slug": "cs2"
            }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Second registration — should conflict
    let response = app
        .post_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({
                "game_auth_code": TEST_AUTH_CODE,
                "game_slug": "cs2"
            }),
            &token,
        )
        .await;

    assert_eq!(
        response.status,
        StatusCode::CONFLICT,
        "Expected 409 CONFLICT for duplicate registration. Body: {}",
        response.text()
    );
}

#[tokio::test]
async fn test_get_steam_tracking() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;

    // Register first
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Get tracking status
    let response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["steam_id_64"], TEST_STEAM_ID.parse::<i64>().unwrap());
    assert!(body["data"]["is_active"].as_bool().unwrap());
}

#[tokio::test]
async fn test_get_steam_tracking_not_found() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;

    let response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_steam_tracking_auth_code() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;

    // Register
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Update auth code
    let new_code = "WXYZ-ABCDE-FGHI";
    let response = app
        .patch_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({ "game_auth_code": new_code }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    // Masked prefix should start with the first 4 chars of the new code
    assert!(
        body["data"]["game_auth_code_prefix"]
            .as_str()
            .unwrap()
            .starts_with("WXYZ"),
        "Expected masked prefix to start with WXYZ"
    );
}

#[tokio::test]
async fn test_delete_steam_tracking() {
    let app = TestApp::new().await;
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;

    // Register
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Delete
    let response = app
        .delete_with_token("/v1/players/me/steam-tracking", &token)
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone
    let response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// =============================================================================
// API key auth checks
// =============================================================================

#[tokio::test]
async fn test_internal_endpoint_requires_api_key() {
    let app = TestApp::new().await;

    // No X-API-Key header → 401
    let response = app.get("/v1/internal/steam-tracking/active?game=cs2").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_internal_endpoint_invalid_api_key() {
    let app = TestApp::new().await;

    let response =
        api_key_get(&app, "/v1/internal/steam-tracking/active?game=cs2", "bad-key").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_internal_endpoint_wrong_permission() {
    let app = TestApp::new().await;

    // Key with only read permission cannot write
    let key = create_test_api_key(app.pool(), "test-bot", &["steam_tracking.read"]).await;

    let response = api_key_patch_json(
        &app,
        &format!("/v1/internal/steam-tracking/{}/poll-result", Uuid::nil()),
        &json!({ "last_known_code": "CSGO-xxx" }),
        &key,
    )
    .await;

    response.assert_status(StatusCode::FORBIDDEN);
}

// =============================================================================
// Internal steam tracking endpoints (API key auth)
// =============================================================================

#[tokio::test]
async fn test_get_active_tracking_entries() {
    let app = TestApp::new().await;

    // Create a player with tracking
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Bot fetches active entries
    let key = create_test_api_key(app.pool(), "cs2-poller", &["steam_tracking.read"]).await;
    let response =
        api_key_get(&app, "/v1/internal/steam-tracking/active?game=cs2", &key).await;
    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["steam_id_64"], TEST_STEAM_ID.parse::<i64>().unwrap());
    assert_eq!(body[0]["game_auth_code"], TEST_AUTH_CODE);
    assert!(body[0]["last_known_code"].is_null());
}

#[tokio::test]
async fn test_update_poll_result_with_share_code() {
    let app = TestApp::new().await;

    // Setup: player + tracking
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    let reg_response = app
        .post_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({
                "game_auth_code": TEST_AUTH_CODE,
                "game_slug": "cs2"
            }),
            &token,
        )
        .await;
    reg_response.assert_status(StatusCode::CREATED);
    let tracking_id = reg_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Bot updates poll result with a new share code
    let key = create_test_api_key(
        app.pool(),
        "cs2-poller",
        &["steam_tracking.read", "steam_tracking.write"],
    )
    .await;

    let response = api_key_patch_json(
        &app,
        &format!("/v1/internal/steam-tracking/{tracking_id}/poll-result"),
        &json!({
            "last_known_code": "CSGO-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx"
        }),
        &key,
    )
    .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify the cursor was updated
    let entries_response =
        api_key_get(&app, "/v1/internal/steam-tracking/active?game=cs2", &key).await;
    let entries: Vec<serde_json::Value> = entries_response.json();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]["last_known_code"],
        "CSGO-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx"
    );
}

#[tokio::test]
async fn test_update_poll_result_with_error() {
    let app = TestApp::new().await;

    // Setup: player + tracking
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    let reg_response = app
        .post_json_with_token(
            "/v1/players/me/steam-tracking",
            &json!({
                "game_auth_code": TEST_AUTH_CODE,
                "game_slug": "cs2"
            }),
            &token,
        )
        .await;
    reg_response.assert_status(StatusCode::CREATED);
    let tracking_id = reg_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Bot reports a poll error
    let key = create_test_api_key(
        app.pool(),
        "cs2-poller",
        &["steam_tracking.read", "steam_tracking.write"],
    )
    .await;

    let response = api_key_patch_json(
        &app,
        &format!("/v1/internal/steam-tracking/{tracking_id}/poll-result"),
        &json!({ "error": "Steam API rate limited" }),
        &key,
    )
    .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

// =============================================================================
// Discovered matches (internal)
// =============================================================================

#[tokio::test]
async fn test_submit_discovered_matches() {
    let app = TestApp::new().await;

    // Setup: player + tracking
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Get the tracking ID
    let tracking_response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    let tracking_id = tracking_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Submit discovered matches
    let key = create_test_api_key(
        app.pool(),
        "cs2-poller",
        &["discovered_matches.write"],
    )
    .await;

    let response = api_key_post_json(
        &app,
        "/v1/internal/discovered-matches",
        &json!({
            "tracking_id": tracking_id,
            "game": "cs2",
            "matches": [
                {
                    "share_code": "CSGO-aaaaa-bbbbb-ccccc-ddddd-eeeee",
                    "match_id": 123456789,
                    "outcome_id": 987654321,
                    "token": 42
                },
                {
                    "share_code": "CSGO-fffff-ggggg-hhhhh-iiiii-jjjjj",
                    "match_id": 123456790,
                    "outcome_id": 987654322,
                    "token": 43
                }
            ]
        }),
        &key,
    )
    .await;

    response.assert_status(StatusCode::CREATED);
    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 2);
    assert_eq!(body[0]["status"], "pending");
    assert_eq!(body[1]["status"], "pending");
}

#[tokio::test]
async fn test_submit_discovered_matches_idempotent() {
    let app = TestApp::new().await;

    // Setup
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let tracking_response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    let tracking_id = tracking_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let key = create_test_api_key(
        app.pool(),
        "cs2-poller",
        &["discovered_matches.write"],
    )
    .await;

    let match_payload = json!({
        "tracking_id": tracking_id,
        "game": "cs2",
        "matches": [{
            "share_code": "CSGO-idmpt-idmpt-idmpt-idmpt-idmpt",
            "match_id": 111111111,
            "outcome_id": 222222222,
            "token": 99
        }]
    });

    // Submit once
    let r1 = api_key_post_json(&app, "/v1/internal/discovered-matches", &match_payload, &key).await;
    r1.assert_status(StatusCode::CREATED);
    let id1 = r1.json::<Vec<serde_json::Value>>()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Submit again (same share_code) — should return same ID (upsert)
    let r2 = api_key_post_json(&app, "/v1/internal/discovered-matches", &match_payload, &key).await;
    r2.assert_status(StatusCode::CREATED);
    let id2 = r2.json::<Vec<serde_json::Value>>()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(id1, id2, "Idempotent upsert should return the same ID");
}

#[tokio::test]
async fn test_get_pending_matches() {
    let app = TestApp::new().await;

    // Setup: player + tracking + submit a match
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let tracking_response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    let tracking_id = tracking_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let write_key = create_test_api_key(
        app.pool(),
        "cs2-poller",
        &["discovered_matches.write"],
    )
    .await;

    api_key_post_json(
        &app,
        "/v1/internal/discovered-matches",
        &json!({
            "tracking_id": tracking_id,
            "game": "cs2",
            "matches": [{
                "share_code": "CSGO-pend1-pend1-pend1-pend1-pend1",
                "match_id": 333333333,
                "outcome_id": 444444444,
                "token": 55
            }]
        }),
        &write_key,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Enricher bot fetches pending matches
    let read_key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read"],
    )
    .await;

    let response = api_key_get(
        &app,
        "/v1/internal/discovered-matches/pending?game=cs2&limit=10",
        &read_key,
    )
    .await;
    response.assert_status(StatusCode::OK);

    let pending: Vec<serde_json::Value> = response.json();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["match_id"], 333333333);
    assert_eq!(pending[0]["outcome_id"], 444444444);
    assert_eq!(pending[0]["token"], 55);
}

#[tokio::test]
async fn test_claim_match() {
    let app = TestApp::new().await;

    // Setup: submit a pending match
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let tracking_response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    let tracking_id = tracking_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    // Submit a match
    let submit_response = api_key_post_json(
        &app,
        "/v1/internal/discovered-matches",
        &json!({
            "tracking_id": tracking_id,
            "game": "cs2",
            "matches": [{
                "share_code": "CSGO-claim-claim-claim-claim-claim",
                "match_id": 555555555,
                "outcome_id": 666666666,
                "token": 77
            }]
        }),
        &key,
    )
    .await;
    submit_response.assert_status(StatusCode::CREATED);
    let match_id = submit_response.json::<Vec<serde_json::Value>>()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Claim the match
    let claim_response = api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/claim"),
        &json!({}),
        &key,
    )
    .await;
    claim_response.assert_status(StatusCode::OK);

    // Claiming again should return CONFLICT
    let claim2_response = api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/claim"),
        &json!({}),
        &key,
    )
    .await;
    claim2_response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_submit_enriched_match() {
    let app = TestApp::new().await;

    // Setup: submit and claim a match
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let tracking_response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    let tracking_id = tracking_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    // Submit
    let submit_response = api_key_post_json(
        &app,
        "/v1/internal/discovered-matches",
        &json!({
            "tracking_id": tracking_id,
            "game": "cs2",
            "matches": [{
                "share_code": "CSGO-enrch-enrch-enrch-enrch-enrch",
                "match_id": 777777777,
                "outcome_id": 888888888,
                "token": 99
            }]
        }),
        &key,
    )
    .await;
    let match_id = submit_response.json::<Vec<serde_json::Value>>()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Claim
    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/claim"),
        &json!({}),
        &key,
    )
    .await
    .assert_status(StatusCode::OK);

    // Submit enriched data
    let enriched_response = api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/enriched"),
        &json!({
            "gc_data": {
                "match_id": 777777777,
                "map": "de_dust2",
                "team_scores": [16, 13],
                "players": [
                    { "account_id": 12345, "kills": 25, "deaths": 18 },
                    { "account_id": 67890, "kills": 20, "deaths": 15 }
                ]
            },
            "demo_url": "http://replay1.2.3.4.valve.net/730/777777777_999.dem.bz2"
        }),
        &key,
    )
    .await;
    enriched_response.assert_status(StatusCode::OK);

    // Verify the match is no longer pending
    let pending = api_key_get(
        &app,
        "/v1/internal/discovered-matches/pending?game=cs2&limit=10",
        &key,
    )
    .await;
    let pending_matches: Vec<serde_json::Value> = pending.json();
    assert!(
        pending_matches.is_empty()
            || pending_matches
                .iter()
                .all(|m| m["id"].as_str().unwrap() != match_id),
        "Enriched match should not appear in pending list"
    );
}

#[tokio::test]
async fn test_mark_match_failed() {
    let app = TestApp::new().await;

    // Setup: submit and claim
    let (_user_id, _player_id, token) = create_player_with_steam(app.pool()).await;
    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let tracking_response = app.get_with_token("/v1/players/me/steam-tracking", &token).await;
    let tracking_id = tracking_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    // Submit
    let submit_response = api_key_post_json(
        &app,
        "/v1/internal/discovered-matches",
        &json!({
            "tracking_id": tracking_id,
            "game": "cs2",
            "matches": [{
                "share_code": "CSGO-fail1-fail1-fail1-fail1-fail1",
                "match_id": 111222333,
                "outcome_id": 444555666,
                "token": 11
            }]
        }),
        &key,
    )
    .await;
    let match_id = submit_response.json::<Vec<serde_json::Value>>()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Claim
    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/claim"),
        &json!({}),
        &key,
    )
    .await
    .assert_status(StatusCode::OK);

    // Mark as failed
    let failed_response = api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{match_id}/failed"),
        &json!({ "error": "GC timeout after 15 seconds" }),
        &key,
    )
    .await;
    failed_response.assert_status(StatusCode::OK);

    // Failed match with retry_count < max_retries should reappear in pending
    let pending = api_key_get(
        &app,
        "/v1/internal/discovered-matches/pending?game=cs2&limit=10",
        &key,
    )
    .await;
    let pending_matches: Vec<serde_json::Value> = pending.json();
    assert!(
        pending_matches.iter().any(|m| m["id"].as_str().unwrap() == match_id),
        "Failed match should reappear in pending list (retry_count < max_retries)"
    );
    let failed_match = pending_matches
        .iter()
        .find(|m| m["id"].as_str().unwrap() == match_id)
        .unwrap();
    assert_eq!(failed_match["retry_count"], 1);
}

// =============================================================================
// Full end-to-end flow
// =============================================================================

#[tokio::test]
async fn test_full_poller_to_enricher_flow() {
    let app = TestApp::new().await;

    // 1. Player registers for steam tracking
    let (_user_id, _player_id, user_token) = create_player_with_steam(app.pool()).await;

    app.post_json_with_token(
        "/v1/players/me/steam-tracking",
        &json!({
            "game_auth_code": TEST_AUTH_CODE,
            "game_slug": "cs2"
        }),
        &user_token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // 2. Poller bot reads active tracking entries
    let poller_key = create_test_api_key(
        app.pool(),
        "cs2-poller",
        &[
            "steam_tracking.read",
            "steam_tracking.write",
            "discovered_matches.write",
        ],
    )
    .await;

    let active_response = api_key_get(
        &app,
        "/v1/internal/steam-tracking/active?game=cs2",
        &poller_key,
    )
    .await;
    active_response.assert_status(StatusCode::OK);

    let entries: Vec<serde_json::Value> = active_response.json();
    assert_eq!(entries.len(), 1);
    let tracking_id = entries[0]["id"].as_str().unwrap().to_string();

    // 3. Poller discovers new share codes and submits them
    let submit_response = api_key_post_json(
        &app,
        "/v1/internal/discovered-matches",
        &json!({
            "tracking_id": tracking_id,
            "game": "cs2",
            "matches": [
                {
                    "share_code": "CSGO-e2e01-e2e01-e2e01-e2e01-e2e01",
                    "match_id": 100000001,
                    "outcome_id": 200000001,
                    "token": 301
                },
                {
                    "share_code": "CSGO-e2e02-e2e02-e2e02-e2e02-e2e02",
                    "match_id": 100000002,
                    "outcome_id": 200000002,
                    "token": 302
                }
            ]
        }),
        &poller_key,
    )
    .await;
    submit_response.assert_status(StatusCode::CREATED);

    // 4. Poller updates the cursor to the newest share code
    let cursor_response = api_key_patch_json(
        &app,
        &format!("/v1/internal/steam-tracking/{tracking_id}/poll-result"),
        &json!({ "last_known_code": "CSGO-e2e02-e2e02-e2e02-e2e02-e2e02" }),
        &poller_key,
    )
    .await;
    cursor_response.assert_status(StatusCode::NO_CONTENT);

    // Verify cursor was updated
    let active2 = api_key_get(
        &app,
        "/v1/internal/steam-tracking/active?game=cs2",
        &poller_key,
    )
    .await;
    let entries2: Vec<serde_json::Value> = active2.json();
    assert_eq!(
        entries2[0]["last_known_code"],
        "CSGO-e2e02-e2e02-e2e02-e2e02-e2e02"
    );

    // 5. Enricher bot fetches pending matches
    let enricher_key = create_test_api_key(
        app.pool(),
        "cs2-enricher",
        &["discovered_matches.read", "discovered_matches.write"],
    )
    .await;

    let pending_response = api_key_get(
        &app,
        "/v1/internal/discovered-matches/pending?game=cs2&limit=10",
        &enricher_key,
    )
    .await;
    pending_response.assert_status(StatusCode::OK);

    let pending: Vec<serde_json::Value> = pending_response.json();
    assert_eq!(pending.len(), 2);

    // 6. Enricher claims and enriches the first match
    let first_match_id = pending[0]["id"].as_str().unwrap().to_string();

    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{first_match_id}/claim"),
        &json!({}),
        &enricher_key,
    )
    .await
    .assert_status(StatusCode::OK);

    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{first_match_id}/enriched"),
        &json!({
            "gc_data": {
                "match_id": 100000001,
                "map": "de_inferno",
                "team_scores": [13, 16],
                "players": []
            },
            "demo_url": "http://replay1.2.3.4.valve.net/730/100000001_999.dem.bz2"
        }),
        &enricher_key,
    )
    .await
    .assert_status(StatusCode::OK);

    // 7. Enricher fails the second match
    let second_match_id = pending[1]["id"].as_str().unwrap().to_string();

    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{second_match_id}/claim"),
        &json!({}),
        &enricher_key,
    )
    .await
    .assert_status(StatusCode::OK);

    api_key_post_json(
        &app,
        &format!("/v1/internal/discovered-matches/{second_match_id}/failed"),
        &json!({ "error": "GC returned empty match list" }),
        &enricher_key,
    )
    .await
    .assert_status(StatusCode::OK);

    // 8. Verify: only the failed match (with retry available) is in pending now
    let final_pending = api_key_get(
        &app,
        "/v1/internal/discovered-matches/pending?game=cs2&limit=10",
        &enricher_key,
    )
    .await;
    let final_pending_list: Vec<serde_json::Value> = final_pending.json();

    assert_eq!(final_pending_list.len(), 1, "Only the failed match should be pending");
    assert_eq!(
        final_pending_list[0]["id"].as_str().unwrap(),
        second_match_id,
        "The failed match should be the one still pending"
    );
    assert_eq!(final_pending_list[0]["retry_count"], 1);
}
