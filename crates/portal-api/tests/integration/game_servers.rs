//! Game-server registry integration tests (MatchZy Phase 1).
//!
//! HTTP surface: admin CRUD, enrollment-token minting, bookings, and the
//! 503-when-unconfigured enrollment path. Service surface (via an AppState
//! sharing the TestApp database, like the lifecycle tests): CSR enrollment
//! round-trip, heartbeat busy_external rules (§6.7), and the staleness
//! sweep. The agent WebSocket framing has unit tests in
//! `websocket::agent_manager`; full socket flow is exercised by the real
//! agent against a dev stack.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_api::state::AppState;
use portal_core::types::{AgentGamestate, GameServerStatus};
use portal_domain::entities::HeartbeatUpdate;
use portal_domain::services::game_server::{CertificateAuthority, hash_token};
use portal_test::prelude::*;
use serde_json::json;

async fn state_for(app: &TestApp) -> AppState {
    AppState::new(app.pool().clone(), "test-jwt-secret").await
}

/// Create a game row and return its UUID string.
async fn seed_game(app: &TestApp) -> String {
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let game = GameBuilder::new()
        .slug(format!("cs2-{}", &suffix[..12]))
        .build_persisted(app.pool())
        .await;
    game.id.to_string()
}

async fn create_server(app: &TestApp, game_id: &str, name: &str) -> serde_json::Value {
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
    let body: serde_json::Value = response.json();
    body["data"].clone()
}

#[tokio::test]
async fn test_admin_crud_round_trip() {
    let app = TestApp::new().await;
    let game_id = seed_game(&app).await;

    let server = create_server(&app, &game_id, "London #1").await;
    let id = server["id"].as_str().unwrap();
    assert_eq!(server["status"], "offline");
    assert_eq!(server["agent_connected"], false);
    assert_eq!(server["enrollment_open"], false);

    // List contains it
    let response = app.get_auth("/v1/admin/game-servers").await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == *id)
    );

    // Patch name + region
    let response = app
        .patch_json(
            &format!("/v1/admin/game-servers/{id}"),
            &json!({ "name": "London #1 (renamed)", "region": "eu-north" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "London #1 (renamed)");
    assert_eq!(body["data"]["region"], "eu-north");
    // Untouched fields survive the partial update
    assert_eq!(body["data"]["ip_address"], "203.0.113.10");
    assert_eq!(body["data"]["gotv_port"], 27020);

    // Delete (offline server → allowed)
    let response = app
        .delete_auth(&format!("/v1/admin/game-servers/{id}"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
    let response = app.get_auth(&format!("/v1/admin/game-servers/{id}")).await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_validation_rejects_bad_input() {
    let app = TestApp::new().await;
    let game_id = seed_game(&app).await;

    // Invalid IP
    let response = app
        .post_json(
            "/v1/admin/game-servers",
            &json!({
                "name": "bad", "game_id": game_id,
                "ip_address": "not-an-ip", "port": 27015, "region": "eu"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // GOTV port colliding with game port
    let response = app
        .post_json(
            "/v1/admin/game-servers",
            &json!({
                "name": "bad", "game_id": game_id,
                "ip_address": "203.0.113.11", "port": 27015,
                "gotv_port": 27015, "region": "eu"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_surface_requires_auth() {
    let app = TestApp::new().await;
    let response = app.get("/v1/admin/game-servers").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_enrollment_round_trip_and_single_use() {
    let app = TestApp::new().await;
    let game_id = seed_game(&app).await;
    let server = create_server(&app, &game_id, "Enroll me").await;
    let id = server["id"].as_str().unwrap();

    // Mint a token over HTTP
    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{id}/enrollment-token"),
            &json!({}),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let token = body["data"]["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("cgs_"));

    // Server now reports an open enrollment window
    let response = app.get_auth(&format!("/v1/admin/game-servers/{id}")).await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["enrollment_open"], true);

    // Enroll at the service level with a real CA + CSR
    let state = state_for(&app).await;
    let ca_material = CertificateAuthority::generate("test-ca").unwrap();
    let ca = CertificateAuthority::from_pem(&ca_material.cert_pem, &ca_material.key_pem).unwrap();

    let agent_key = rcgen::KeyPair::generate().unwrap();
    let csr_pem = rcgen::CertificateParams::default()
        .serialize_request(&agent_key)
        .unwrap()
        .pem()
        .unwrap();

    let result = state
        .game_server_registry
        .enroll(&token, &csr_pem, &ca)
        .await
        .expect("enrollment succeeds");
    assert!(result.cert_pem.contains("BEGIN CERTIFICATE"));
    assert_eq!(result.server.id.to_string(), id);

    // Cert serial resolves back to the server (the WS auth path)
    let authed = state
        .game_server_registry
        .authenticate_agent(&result.certificate.serial)
        .await
        .expect("serial authenticates");
    assert_eq!(authed.id.to_string(), id);

    // Token is single-use
    let err = state
        .game_server_registry
        .enroll(&token, &csr_pem, &ca)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("enrollment token"));

    // Revocation kills the serial
    let response = app
        .post_json(&format!("/v1/admin/game-servers/{id}/revoke"), &json!({}))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["revoked_count"], 1);
    assert!(
        state
            .game_server_registry
            .authenticate_agent(&result.certificate.serial)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_enroll_http_is_503_without_ca() {
    let app = TestApp::new().await;
    let response = app
        .post_json_no_auth(
            "/v1/gameserver/enroll",
            &json!({ "enrollment_token": "cgs_x", "csr_pem": "junk" }),
        )
        .await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_heartbeat_busyness_rules() {
    let app = TestApp::new().await;
    let game_id = seed_game(&app).await;
    let server = create_server(&app, &game_id, "Busy box").await;
    let id: portal_core::ids::GameServerId = server["id"].as_str().unwrap().parse().unwrap();
    let state = state_for(&app).await;
    let registry = &state.game_server_registry;

    let hb = |gamestate, rcon_ok| HeartbeatUpdate {
        agent_version: "0.1.0-test".into(),
        rcon_ok,
        gamestate,
        reported_matchzy_id: None,
    };

    // Idle heartbeat: offline → available
    let s = registry
        .record_heartbeat(id, hb(Some(AgentGamestate::None), true), false)
        .await
        .unwrap();
    assert_eq!(s.status, GameServerStatus::Available);

    // Match loaded with NO live reservation → out-of-band pug (§6.7)
    let s = registry
        .record_heartbeat(id, hb(Some(AgentGamestate::Live), true), false)
        .await
        .unwrap();
    assert_eq!(s.status, GameServerStatus::BusyExternal);

    // Back to idle → available again
    let s = registry
        .record_heartbeat(id, hb(Some(AgentGamestate::None), true), false)
        .await
        .unwrap();
    assert_eq!(s.status, GameServerStatus::Available);

    // Match loaded WITH a live reservation → ours, not external
    let s = registry
        .record_heartbeat(id, hb(Some(AgentGamestate::Live), true), true)
        .await
        .unwrap();
    assert_eq!(s.status, GameServerStatus::InMatch);

    // RCON down → error
    let s = registry
        .record_heartbeat(id, hb(None, false), false)
        .await
        .unwrap();
    assert_eq!(s.status, GameServerStatus::Error);

    // Deleting a busy server is refused; idle again → allowed
    registry
        .record_heartbeat(id, hb(Some(AgentGamestate::Live), true), true)
        .await
        .unwrap();
    let response = app
        .delete_auth(&format!("/v1/admin/game-servers/{id}"))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    registry
        .record_heartbeat(id, hb(Some(AgentGamestate::None), true), false)
        .await
        .unwrap();
    let response = app
        .delete_auth(&format!("/v1/admin/game-servers/{id}"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_stale_sweep_marks_offline() {
    let app = TestApp::new().await;
    let game_id = seed_game(&app).await;
    let server = create_server(&app, &game_id, "Stale box").await;
    let id: portal_core::ids::GameServerId = server["id"].as_str().unwrap().parse().unwrap();
    let state = state_for(&app).await;

    state
        .game_server_registry
        .record_heartbeat(
            id,
            HeartbeatUpdate {
                agent_version: "0.1.0-test".into(),
                rcon_ok: true,
                gamestate: Some(AgentGamestate::None),
                reported_matchzy_id: None,
            },
            false,
        )
        .await
        .unwrap();

    // Sweep as if 10 minutes have passed: the heartbeat is stale.
    let transitioned = state
        .game_server_registry
        .sweep_stale(chrono::Utc::now() + chrono::Duration::minutes(10))
        .await
        .unwrap();
    assert!(transitioned.contains(&id));

    let s = state.game_server_registry.get(id).await.unwrap();
    assert_eq!(s.status, GameServerStatus::Offline);
}

#[tokio::test]
async fn test_bookings_crud_and_validation() {
    let app = TestApp::new().await;
    let game_id = seed_game(&app).await;
    let server = create_server(&app, &game_id, "Booked box").await;
    let id = server["id"].as_str().unwrap();
    let now = chrono::Utc::now();

    // Window ending before it starts → 400
    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{id}/bookings"),
            &json!({
                "reason": "backwards",
                "starts_at": (now + chrono::Duration::hours(2)).to_rfc3339(),
                "ends_at": (now + chrono::Duration::hours(1)).to_rfc3339(),
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Valid hard hold
    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{id}/bookings"),
            &json!({
                "reason": "community night",
                "starts_at": (now + chrono::Duration::hours(1)).to_rfc3339(),
                "ends_at": (now + chrono::Duration::hours(4)).to_rfc3339(),
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let booking_id = body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["tournament_id"], serde_json::Value::Null);

    // Listed
    let response = app
        .get_auth(&format!("/v1/admin/game-servers/{id}/bookings"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // Deleted
    let response = app
        .delete_auth(&format!(
            "/v1/admin/game-servers/{id}/bookings/{booking_id}"
        ))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_enrollment_token_hash_never_leaves_the_db_raw() {
    // The stored hash must be the SHA-256 of the raw token, not the token.
    let app = TestApp::new().await;
    let game_id = seed_game(&app).await;
    let server = create_server(&app, &game_id, "Hash check").await;
    let id = server["id"].as_str().unwrap();

    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{id}/enrollment-token"),
            &json!({}),
        )
        .await;
    let body: serde_json::Value = response.json();
    let token = body["data"]["token"].as_str().unwrap();

    let (stored_hash,): (String,) =
        sqlx::query_as("SELECT enrollment_token_hash FROM game_servers WHERE id = $1::uuid")
            .bind(id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(stored_hash, hash_token(token));
    assert_ne!(stored_hash, token);
}
