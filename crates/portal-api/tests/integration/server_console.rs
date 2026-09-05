//! The CS2 server console (docs/server-console-design.md §5): the
//! heartbeat-fed snapshot and its live refresh, the raw passthrough's
//! deny-list and audit trail, map changes with and without a live
//! reservation, and the action table's gates and holds.
//!
//! The "server" is a [`FakeAgent`] connected over the dev-auth WebSocket.

use crate::common::TestApp;
use crate::common::agent::{FakeAgent, Script, wait_for};
use crate::tournaments::create_tournament_with_matches;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use portal_api::state::AppState;
use portal_core::ids::ServerReservationId;
use portal_domain::repositories::{CreateServerReservation, ServerReservationRepository};
use portal_domain::services::game_server::hash_token;
use serde_json::{Value, json};
use std::sync::Arc;

/// CS2 1.40 `status` with a bot, an anonymous human and a `[U:1:n]` human.
const CS2_STATUS: &str = "hostname: CS2 10 Mans #1\n\
version : 1.40.7.3/14073 10529 secure  public\n\
os/type : Linux dedicated\n\
map     : de_ancient\n\
players : 2 humans, 1 bots (12 max) (not hibernating) (unreserved)\n\
\n\
  id     time ping loss      state   rate adr name\n\
  0     BOT    0    0     active      0 BOT 'Kyle'\n\
  2  05:23   31    0     active 786432 203.0.113.9:27005 'Player One'\n\
  3  00:12   58    2     active 786432 198.51.100.4:27005 'Two [U:1:12345678]'\n\
#end\n";

/// What a live `status` says after the map moved on.
const LIVE_STATUS: &str = "hostname: CS2 10 Mans #1\n\
map     : de_mirage\n\
players : 0 humans, 0 bots (12 max) (not hibernating) (unreserved)\n\
\n\
  id     time ping loss      state   rate adr name\n\
#end\n";

async fn cs2_game_id(app: &TestApp) -> String {
    portal_test::helpers::get_game_id(app.pool(), "cs2")
        .await
        .to_string()
}

async fn create_server(app: &TestApp, game_id: &str, name: &str) -> String {
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
    let body: Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// A script that answers `status` with `LIVE_STATUS` and everything else
/// with a canned `ok`.
fn default_script() -> Script {
    Arc::new(|line: &str| match line {
        "status" => (true, LIVE_STATUS.to_string()),
        other => (true, format!("ran: {other}")),
    })
}

/// Connect a fake agent, send an idle heartbeat, and wait until the portal
/// sees it connected and available.
async fn connect_idle_agent(
    app: &mut TestApp,
    server_id: &str,
    script: Script,
    status_output: Option<&str>,
) -> FakeAgent {
    let addr = app.start_server().await;
    let agent = FakeAgent::connect(addr, server_id, script).await;
    agent.heartbeat("none", status_output).await;
    let uri = format!("/v1/admin/game-servers/{server_id}");
    wait_for(10, || async {
        let body: Value = app.get_auth(&uri).await.json();
        let data = &body["data"];
        (data["agent_connected"] == true && data["status"] == "available").then_some(())
    })
    .await;
    agent
}

async fn console(app: &TestApp, server_id: &str, live: bool) -> Value {
    let uri = format!(
        "/v1/admin/game-servers/{server_id}/console{}",
        if live { "?live=true" } else { "" }
    );
    let response = app.get_auth(&uri).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].clone()
}

async fn history(app: &TestApp, server_id: &str) -> Vec<Value> {
    let response = app
        .get_auth(&format!(
            "/v1/admin/game-servers/{server_id}/console/history?limit=10"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].as_array().cloned().unwrap_or_default()
}

async fn bookings(app: &TestApp, server_id: &str) -> Vec<Value> {
    let response = app
        .get_auth(&format!("/v1/admin/game-servers/{server_id}/bookings"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].as_array().cloned().unwrap_or_default()
}

fn reasons_starting_with(rows: &[Value], prefix: &str) -> usize {
    rows.iter()
        .filter(|b| b["reason"].as_str().is_some_and(|r| r.starts_with(prefix)))
        .count()
}

// =============================================================================
// Snapshot
// =============================================================================

#[tokio::test]
async fn test_heartbeat_status_feeds_the_table_and_the_snapshot() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Console box").await;
    let agent = connect_idle_agent(&mut app, &server_id, default_script(), Some(CS2_STATUS)).await;

    // The registry row carries what the table shows.
    let uri = format!("/v1/admin/game-servers/{server_id}");
    let row = wait_for(10, || async {
        let body: Value = app.get_auth(&uri).await.json();
        (body["data"]["last_map"] == "de_ancient").then_some(body["data"].clone())
    })
    .await;
    assert_eq!(row["last_player_count"], 3);
    assert!(row["last_status_at"].is_string(), "{row}");

    // The stored snapshot: parsed players, addresses gone, ids as strings.
    let snap = console(&app, &server_id, false).await;
    assert_eq!(snap["live"], false, "{snap}");
    assert_eq!(snap["agent"]["connected"], true);
    assert_eq!(snap["agent"]["reports_status"], true);
    assert_eq!(snap["agent"]["rcon_ok"], true);
    assert_eq!(snap["gamestate"], "none");
    assert_eq!(snap["status"]["map"], "de_ancient");
    assert_eq!(snap["status"]["humans"], 2);
    assert_eq!(snap["status"]["bots"], 1);
    let players = snap["status"]["players"].as_array().unwrap();
    assert_eq!(players.len(), 3, "{snap}");
    assert_eq!(players[0]["bot"], true);
    assert_eq!(players[0]["name"], "Kyle");
    assert_eq!(players[2]["steam_id64"], "76561197972611406");
    assert!(
        players[2]["player"].is_null(),
        "no portal account behind it"
    );
    let raw = snap["raw_status"].as_str().unwrap();
    assert!(
        raw.contains("<addr>") && !raw.contains("203.0.113.9"),
        "{raw}"
    );
    assert!(snap["reservation"].is_null());
    assert_eq!(snap["holds"].as_array().unwrap().len(), 0);

    // Live refresh runs `status` on the server and prefers its answer.
    agent.clear();
    let live = console(&app, &server_id, true).await;
    assert_eq!(live["live"], true, "{live}");
    assert!(live["live_error"].is_null());
    assert_eq!(live["status"]["map"], "de_mirage");
    assert_eq!(live["status"]["players"].as_array().unwrap().len(), 0);
    assert_eq!(agent.lines(), vec!["status".to_string()]);

    // A live refresh with no agent falls back to the store and says why.
    agent.close().await;
    wait_for(10, || async {
        let body: Value = app.get_auth(&uri).await.json();
        (body["data"]["agent_connected"] == false).then_some(())
    })
    .await;
    let fallback = console(&app, &server_id, true).await;
    assert_eq!(fallback["live"], false);
    assert!(fallback["live_error"].is_string(), "{fallback}");
    assert_eq!(
        fallback["status"]["map"], "de_ancient",
        "stored data survives"
    );
}

// =============================================================================
// Raw passthrough
// =============================================================================

#[tokio::test]
async fn test_raw_command_is_deny_listed_and_audited() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Raw box").await;
    let agent = connect_idle_agent(&mut app, &server_id, default_script(), None).await;
    let uri = format!("/v1/admin/game-servers/{server_id}/command");

    // Refused before anything reaches the agent.
    for refused in [
        "rcon_password hunter2",
        "sv_password secret",
        "quit",
        "exec server.cfg",
        "css_pause; quit",
        "matchzy_remote_log_url http://evil",
    ] {
        let response = app.post_json(&uri, &json!({ "command": refused })).await;
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "{refused} should be refused"
        );
    }
    let response = app.post_json(&uri, &json!({ "command": "" })).await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        agent.lines().is_empty(),
        "nothing refused may reach the agent"
    );
    assert!(history(&app, &server_id).await.is_empty());

    // An allowed line runs, answers, and leaves one audit row.
    let response = app
        .post_json(&uri, &json!({ "command": "css_pause" }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["ok"], true);
    assert_eq!(body["data"]["output"], "ran: css_pause");
    assert_eq!(agent.lines(), vec!["css_pause".to_string()]);

    let rows = history(&app, &server_id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["kind"], "raw");
    assert_eq!(rows[0]["command"], "css_pause");
    assert_eq!(rows[0]["output"], "ran: css_pause");
    assert_eq!(rows[0]["ok"], true);
    assert_eq!(rows[0]["force"], false);
    assert!(rows[0]["admin_username"].is_string(), "{rows:?}");
    assert!(rows[0]["reservation_id"].is_null());

    agent.close().await;
}

// =============================================================================
// Map change
// =============================================================================

#[tokio::test]
async fn test_map_change_resolves_catalogue_and_custom_targets() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Map box").await;
    let agent = connect_idle_agent(&mut app, &server_id, default_script(), None).await;
    let uri = format!("/v1/admin/game-servers/{server_id}/console/map");

    // Bad targets cost nothing.
    for bad in [
        json!({}),
        json!({ "map_id": "no_such_map" }),
        json!({ "custom": "de_x; quit" }),
        json!({ "custom": "" }),
    ] {
        let response = app.post_json(&uri, &bad).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert!(agent.lines().is_empty());

    // A stock catalogue map: changelevel, with a short allocator hold.
    let response = app.post_json(&uri, &json!({ "map_id": "de_mirage" })).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["command"], "changelevel de_mirage");
    assert_eq!(body["data"]["ok"], true);
    assert_eq!(body["data"]["target"]["engine_name"], "de_mirage");
    assert_eq!(body["data"]["target"]["workshop"], false);
    assert!(body["data"]["hold_until"].is_string(), "{body}");
    assert!(body["data"]["cancelled_match_id"].is_null());
    assert_eq!(agent.lines(), vec!["changelevel de_mirage".to_string()]);
    let holds = bookings(&app, &server_id).await;
    assert_eq!(reasons_starting_with(&holds, "Map change (console)"), 1);

    // A workshop link: host_workshop_map with the numeric id.
    agent.clear();
    let response = app
        .post_json(
            &uri,
            &json!({ "custom": "https://steamcommunity.com/sharedfiles/filedetails/?id=3070210099" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["command"], "host_workshop_map 3070210099");
    assert_eq!(body["data"]["target"]["workshop"], true);
    assert_eq!(
        agent.lines(),
        vec!["host_workshop_map 3070210099".to_string()]
    );

    // Both are in the audit trail, newest first.
    let rows = history(&app, &server_id).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0]["kind"], "map_change");
    assert_eq!(rows[0]["command"], "host_workshop_map 3070210099");
    assert_eq!(rows[1]["command"], "changelevel de_mirage");

    // The snapshot shows the holds.
    let snap = console(&app, &server_id, false).await;
    assert_eq!(snap["holds"].as_array().unwrap().len(), 2, "{snap}");

    agent.close().await;
}

#[tokio::test]
async fn test_map_change_under_a_live_reservation_needs_force() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "console-force").await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Reserved box").await;
    let agent = connect_idle_agent(&mut app, &server_id, default_script(), None).await;

    // Allocate a reservation onto the server the way the flow does.
    let reservation = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id: match_id.parse().unwrap(),
            kind: portal_core::types::ReservationKind::Match,
            connect_password: "pw12345678".into(),
            gotv_password: None,
            config_token_hash: hash_token("cfg"),
            event_token_hash: hash_token("evt"),
            config_token_expires_at: Utc::now() + Duration::minutes(15),
        })
        .await
        .unwrap();
    let (reservation, _server) = state
        .server_reservation_repo
        .allocate(
            reservation.id,
            game_id.parse().unwrap(),
            tournament_id.parse().unwrap(),
            None,
            Utc::now() - Duration::seconds(90),
            Utc::now(),
            None,
            false,
        )
        .await
        .unwrap()
        .expect("the idle server is allocatable");
    assert_eq!(
        reservation.server_id.map(|s| s.to_string()).as_deref(),
        Some(server_id.as_str())
    );

    let snap = console(&app, &server_id, false).await;
    assert_eq!(snap["reservation"]["match_id"], match_id, "{snap}");

    let uri = format!("/v1/admin/game-servers/{server_id}/console/map");
    let response = app.post_json(&uri, &json!({ "map_id": "de_mirage" })).await;
    response.assert_status(StatusCode::CONFLICT);
    assert!(agent.lines().is_empty(), "a refused change sends nothing");

    // force: the reservation is cancelled through the flow (end_match to
    // the agent, released as cancelled) and only then the level changes.
    let response = app
        .post_json(&uri, &json!({ "map_id": "de_mirage", "force": true }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["cancelled_match_id"], match_id, "{body}");
    assert_eq!(
        agent.lines(),
        vec!["end_match".to_string(), "changelevel de_mirage".to_string()]
    );
    let live = state
        .server_reservation_repo
        .find_live_by_server(server_id.parse().unwrap())
        .await
        .unwrap();
    assert!(live.is_none(), "reservation released");

    let rows = history(&app, &server_id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["force"], true);
    assert_eq!(
        rows[0]["reservation_id"],
        reservation.id.to_string(),
        "audited against the reservation it cancelled"
    );

    agent.close().await;
}

// =============================================================================
// Actions
// =============================================================================

#[tokio::test]
async fn test_actions_are_gated_quoted_and_hold_the_server() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Action box").await;
    let agent = connect_idle_agent(&mut app, &server_id, default_script(), None).await;
    let uri = format!("/v1/admin/game-servers/{server_id}/console/action");

    // Gates: confirmation and arguments.
    for bad in [
        json!({ "action": "end_match" }),
        json!({ "action": "kick_player", "args": { "userid": 3 } }),
        json!({ "action": "broadcast" }),
        json!({ "action": "broadcast", "args": { "message": "say \"hi\"" } }),
        json!({ "action": "kick_player", "confirm": true }),
        json!({ "action": "practice_start", "args": { "until": "2000-01-01T00:00:00Z" } }),
        json!({ "action": "no_such_action" }),
    ] {
        let response = app.post_json(&uri, &bad).await;
        assert!(
            matches!(
                response.status,
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "{bad} → {}",
            response.status
        );
    }
    assert!(agent.lines().is_empty());

    // Quoting.
    let response = app
        .post_json(
            &uri,
            &json!({ "action": "broadcast", "args": { "message": "GG all" } }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["commands"], json!(["say \"GG all\""]));
    assert_eq!(body["data"]["ok"], true);

    let response = app
        .post_json(
            &uri,
            &json!({ "action": "kick_player", "confirm": true, "args": { "userid": 3 } }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(
        body["data"]["commands"],
        json!(["kickid 3 \"Kicked by an admin\""])
    );

    // Idle server: restart_warmup is the two-line form.
    let response = app
        .post_json(&uri, &json!({ "action": "restart_warmup" }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(
        body["data"]["commands"],
        json!(["mp_warmup_start", "mp_restartgame 1"])
    );

    // end_match with no reservation is a plain css_endmatch.
    let response = app
        .post_json(&uri, &json!({ "action": "end_match", "confirm": true }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["commands"], json!(["css_endmatch"]));
    assert!(body["data"]["cancelled_match_id"].is_null());

    assert_eq!(
        agent.lines(),
        vec![
            "say \"GG all\"".to_string(),
            "kickid 3 \"Kicked by an admin\"".to_string(),
            "mp_warmup_start".to_string(),
            "mp_restartgame 1".to_string(),
            "css_endmatch".to_string(),
        ]
    );

    // Practice: a hard hold for the window, released by practice_stop.
    agent.clear();
    let response = app
        .post_json(&uri, &json!({ "action": "practice_start" }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert!(body["data"]["hold_until"].is_string(), "{body}");
    assert_eq!(
        body["data"]["commands"],
        json!(["matchzy_kick_when_no_match_loaded false", "css_prac"])
    );
    let holds = bookings(&app, &server_id).await;
    assert_eq!(
        reasons_starting_with(&holds, "Practice (console)"),
        1,
        "{holds:?}"
    );

    let response = app
        .post_json(&uri, &json!({ "action": "practice_stop" }))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(
        body["data"]["commands"],
        json!(["css_exitprac", "matchzy_kick_when_no_match_loaded true"])
    );
    let holds = bookings(&app, &server_id).await;
    assert_eq!(
        reasons_starting_with(&holds, "Practice (console)"),
        0,
        "{holds:?}"
    );

    // Every action left a row.
    let rows = history(&app, &server_id).await;
    assert_eq!(rows.len(), 6, "{rows:?}");
    assert_eq!(rows[0]["kind"], "action:practice_stop");
    assert_eq!(rows[5]["kind"], "action:broadcast");

    agent.close().await;
}
