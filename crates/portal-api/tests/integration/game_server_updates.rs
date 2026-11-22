//! Game-server updates through LinuxGSM
//! (docs/gameserver-lgsm-updates-design.md §7) against the wire contract of
//! portal-server-agent 0.3.0: capability gating, the start → progress →
//! result → ack path, at-least-once result delivery, jobs the portal did
//! not start, the players/reservation gates, allocation, the lost-job pass
//! and the portal-managed MatchZy overrides.
//!
//! The "server" is a [`FakeAgent`] connected over the dev-auth WebSocket.

use crate::common::TestApp;
use crate::common::agent::{
    AGENT_JOB_ID, DROP_CONNECTION, FakeAgent, FrameScript, update_script, wait_for,
};
use crate::tournaments::create_tournament_with_matches;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use portal_api::state::AppState;
use portal_core::ids::ServerReservationId;
use portal_domain::entities::ServerReservation;
use portal_domain::repositories::{CreateServerReservation, ServerReservationRepository};
use portal_domain::services::game_server::hash_token;
use serde_json::{Value, json};
use std::sync::Arc;

/// CS2 `status` with two humans and a bot.
const BUSY_STATUS: &str = "hostname: CS2 10 Mans #1\n\
map     : de_ancient\n\
players : 2 humans, 1 bots (12 max) (not hibernating) (unreserved)\n\
\n\
  id     time ping loss      state   rate adr name\n\
  0     BOT    0    0     active      0 BOT 'Kyle'\n\
  2  05:23   31    0     active 786432 203.0.113.9:27005 'Player One'\n\
  3  00:12   58    2     active 786432 198.51.100.4:27005 'Player Two'\n\
#end\n";

/// CS2 `status` of an empty server.
const EMPTY_STATUS: &str = "hostname: CS2 10 Mans #1\n\
map     : de_mirage\n\
players : 0 humans, 0 bots (12 max) (not hibernating) (unreserved)\n\
\n\
  id     time ping loss      state   rate adr name\n\
#end\n";

/// A job id as an agent would mint for a job the portal did not start.
const LOCAL_JOB: &str = "0193a5b0-7c1e-7000-8000-0000000010ca";

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

fn capable() -> Value {
    json!({ "capabilities": ["lgsm_update"] })
}

async fn overview(app: &TestApp, server_id: &str) -> Value {
    let response = app
        .get_auth(&format!("/v1/admin/game-servers/{server_id}/updates"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].clone()
}

async fn job(app: &TestApp, server_id: &str, job_id: &str) -> Value {
    let response = app
        .get_auth(&format!(
            "/v1/admin/game-servers/{server_id}/updates/{job_id}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].clone()
}

/// Poll a job until `probe` accepts it.
async fn job_when(
    app: &TestApp,
    server_id: &str,
    job_id: &str,
    probe: impl Fn(&Value) -> bool,
) -> Value {
    let uri = format!("/v1/admin/game-servers/{server_id}/updates/{job_id}");
    wait_for(10, || async {
        let response = app.get_auth(&uri).await;
        if response.status != StatusCode::OK {
            return None;
        }
        let body: Value = response.json();
        probe(&body["data"]).then(|| body["data"].clone())
    })
    .await
}

async fn history(app: &TestApp, server_id: &str) -> Vec<Value> {
    let response = app
        .get_auth(&format!(
            "/v1/admin/game-servers/{server_id}/console/history?limit=50"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].as_array().cloned().unwrap_or_default()
}

/// Connect an agent that speaks 0.3.0 and advertises `lgsm_update`, send
/// one idle heartbeat carrying `extra` on top, and wait until the portal
/// has stored it.
async fn connect_capable(
    app: &mut TestApp,
    server_id: &str,
    script: FrameScript,
    status_output: Option<&str>,
    extra: Value,
) -> FakeAgent {
    let addr = app.start_server().await;
    let agent = FakeAgent::connect_with(addr, server_id, script).await;
    let mut fields = capable();
    if let Some(extra) = extra.as_object() {
        for (k, v) in extra {
            fields[k.as_str()] = v.clone();
        }
    }
    agent.heartbeat_with("none", status_output, fields).await;
    wait_for(10, || async {
        let data = overview(app, server_id).await;
        (data["agent_connected"] == true && data["supported"] == true).then_some(())
    })
    .await;
    agent
}

async fn start(app: &TestApp, server_id: &str, body: &Value) -> (StatusCode, Value) {
    let response = app
        .post_json(&format!("/v1/admin/game-servers/{server_id}/updates"), body)
        .await;
    let status = response.status;
    let body = serde_json::from_str(&response.text()).unwrap_or(Value::Null);
    (status, body)
}

/// Start an update that must be accepted; returns the job id.
async fn start_ok(app: &TestApp, server_id: &str, body: &Value) -> String {
    let (status, body) = start(app, server_id, body).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    body["data"]["id"].as_str().unwrap().to_string()
}

fn progress(job_id: &str, operation: &str, phase: &str, trigger: &str) -> Value {
    json!({
        "type": "update_progress", "job_id": job_id, "operation": operation,
        "phase": phase, "status": "running", "trigger": trigger,
        "log_tail": format!("[{phase}] working\n"),
    })
}

/// A successful `mods` result in the README's shape.
fn mods_result(job_id: &str, trigger: &str) -> Value {
    json!({
        "type": "update_result",
        "job_id": job_id, "operation": "mods", "trigger": trigger,
        "status": "succeeded", "phase": "done", "failed_phase": null,
        "started_at": "2026-11-21T04:00:00Z", "finished_at": "2026-11-21T04:03:10Z",
        "before": {"lgsm": "v26.6.0", "cs2_build": "20012345", "metamod": null,
                   "cssharp": "1.0.370", "matchzy": "0.9.2"},
        "after": {"lgsm": "v26.6.0", "cs2_build": "20012345",
                  "metamod": "mmsource-2.0.0-git1491-linux.tar.gz",
                  "cssharp": "1.0.380", "matchzy": "0.9.3"},
        "latest": null,
        "load": {"metamod": true, "cssharp": true, "matchzy": true, "error": null},
        "demo_upload_block_present": true,
        "restarted": true, "needs_reload": false, "config_changed": true,
        "config_report": {
            "cvars_changed": [{"name": "matchzy_kick_when_no_match_loaded",
                               "before": "false", "after": "true"}],
            "cvars_removed": ["matchzy_max_tech_pauses_allowed"],
            "cvars_added": ["matchzy_max_tech_pauses"],
            "files_changed": ["config.cfg", "live.cfg"]
        },
        "error": null,
        "log_tail": "[done] all good\n",
        "pid": 12345
    })
}

fn failed_result(job_id: &str, operation: &str, error: &str) -> Value {
    json!({
        "type": "update_result", "job_id": job_id, "operation": operation,
        "trigger": "portal", "status": "failed", "phase": "starting",
        "failed_phase": "starting", "error": error, "log_tail": null,
    })
}

/// Allocate a reservation for `match_id`, the way the flow does.
async fn try_allocate(
    state: &AppState,
    game_id: &str,
    tournament_id: &str,
    match_id: &str,
) -> Option<ServerReservation> {
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let reservation = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id: match_id.parse().unwrap(),
            kind: portal_core::types::ReservationKind::Match,
            connect_password: "pw12345678".into(),
            gotv_password: None,
            config_token_hash: hash_token(&format!("cfg-{suffix}")),
            event_token_hash: hash_token(&format!("evt-{suffix}")),
            config_token_expires_at: Utc::now() + Duration::minutes(15),
        })
        .await
        .unwrap();
    let allocated = state
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
        .map(|(r, _)| r);
    if allocated.is_none() {
        // Leave nothing pending behind for the next attempt.
        state
            .server_reservation_repo
            .release(
                reservation.id,
                portal_core::types::ReservationStatus::Cancelled,
                Some("test: not allocated"),
            )
            .await
            .unwrap();
    }
    allocated
}

/// Make a job look as if nothing had been heard of it for 21 minutes.
async fn age_job(app: &TestApp, job_id: &str) {
    sqlx::query(
        "UPDATE game_server_update_jobs SET updated_at = NOW() - INTERVAL '21 minutes' \
         WHERE id = $1::uuid",
    )
    .bind(job_id)
    .execute(app.pool())
    .await
    .unwrap();
}

/// The server has exactly `n` jobs, every one closed by the safety gate
/// (`failed`, the gate's reasons as the error) — none left in flight.
async fn assert_gated(app: &TestApp, server_id: &str, n: usize) {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT status, error FROM game_server_update_jobs WHERE server_id = $1::uuid",
    )
    .bind(server_id)
    .fetch_all(app.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), n, "{rows:?}");
    for (status, error) in &rows {
        assert_eq!(status, "failed", "{rows:?}");
        assert!(
            error
                .as_deref()
                .unwrap_or_default()
                .starts_with("refused by the safety gate: "),
            "{rows:?}"
        );
    }
}

async fn job_count(app: &TestApp, server_id: &str) -> i64 {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM game_server_update_jobs WHERE server_id = $1::uuid")
            .bind(server_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    n
}

// =============================================================================
// Capability gating and old agents
// =============================================================================

#[tokio::test]
async fn test_an_old_agent_is_unaffected_and_the_feature_is_gated() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Old agent box").await;

    // No agent at all: the overview still answers, everything else is 409.
    let data = overview(&app, &server_id).await;
    assert_eq!(data["agent_connected"], false);
    assert_eq!(data["supported"], false);
    assert_eq!(data["capabilities"], json!([]));
    assert!(data["installed"].is_null());
    assert!(data["check"].is_null());
    assert!(data["active_job"].is_null());
    assert_eq!(data["recent_jobs"], json!([]));
    assert!(data["safety"]["humans_connected"].is_null());
    let (status, body) = start(&app, &server_id, &json!({"operation": "lgsm"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("no agent connected"),
        "{body}"
    );

    // An agent 0.2.0: the old heartbeat, none of the new fields.
    let addr = app.start_server().await;
    let agent = FakeAgent::connect_with(addr, &server_id, update_script()).await;
    agent.heartbeat("none", Some(EMPTY_STATUS)).await;
    let uri = format!("/v1/admin/game-servers/{server_id}");
    wait_for(10, || async {
        let body: Value = app.get_auth(&uri).await.json();
        let data = &body["data"];
        (data["agent_connected"] == true
            && data["status"] == "available"
            && data["last_map"] == "de_mirage")
            .then_some(())
    })
    .await;

    let data = overview(&app, &server_id).await;
    assert_eq!(data["agent_connected"], true);
    assert_eq!(data["agent_version"], "0.2.0-test");
    assert_eq!(data["supported"], false, "{data}");
    assert!(data["demo_upload_wired"].is_null());
    assert!(data["overrides_applied"].is_null());
    assert_eq!(data["reload_pending"], false);
    assert!(data["agent_job"].is_null());
    assert_eq!(data["safety"]["humans_connected"], 0);

    // Every command to the helper is refused before it reaches the agent.
    for (uri, body) in [
        ("updates", json!({"operation": "mods"})),
        ("updates", json!({"operation": "lgsm", "force": true})),
        ("updates/check", json!({})),
        ("updates/repair-config", json!({})),
    ] {
        let response = app
            .post_json(&format!("/v1/admin/game-servers/{server_id}/{uri}"), &body)
            .await;
        response.assert_status(StatusCode::CONFLICT);
        let problem: Value = response.json();
        assert!(
            problem["detail"].as_str().unwrap().contains("lgsm_update"),
            "{uri}: {problem}"
        );
    }
    assert_eq!(job_count(&app, &server_id).await, 0);

    // Overrides are stored for later; the old agent is not sent them.
    let response = app
        .put_json(
            &format!("/v1/admin/game-servers/{server_id}/matchzy-overrides"),
            &json!({"overrides": {"matchzy_max_tech_pauses": 2}}),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let saved: Value = response.json();
    assert_eq!(saved["data"]["delivery"], "pending", "{saved}");
    assert!(
        saved["data"]["detail"]
            .as_str()
            .unwrap()
            .contains("lgsm_update"),
        "{saved}"
    );

    // A second heartbeat, and still nothing but silence towards the agent.
    agent.heartbeat("none", Some(EMPTY_STATUS)).await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(agent.frames().is_empty(), "{:?}", agent.frames());

    // The console still works for it.
    let response = app
        .get_auth(&format!("/v1/admin/game-servers/{server_id}/console"))
        .await;
    response.assert_status(StatusCode::OK);
    agent.close().await;
}

#[tokio::test]
async fn test_an_unknown_operation_or_job_is_rejected() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Strict box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;

    // `repair-config` and `check` have their own endpoints; anything else
    // is not an operation.
    // Rejected as a problem document (RFC 7807), not plain text.
    let bodies = ["repair-config", "check", "verify-config", "MODS", ""]
        .iter()
        .map(|op| json!({"operation": op}))
        .chain([
            json!({}),
            json!({"operation": 3}),
            json!("mods"),
            json!({"operation": "mods", "force": "yes"}),
        ]);
    for request in bodies {
        let (status, body) = start(&app, &server_id, &request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{request}: {body}");
        assert_eq!(body["status"], 400, "{request}: {body}");
        assert!(body["detail"].is_string(), "{request}: {body}");
    }
    // Overrides that are not an object of cvars likewise.
    let uri = format!("/v1/admin/game-servers/{server_id}/matchzy-overrides");
    for request in [
        json!({"overrides": ["matchzy_x"]}),
        json!({"overrides": "x"}),
        json!({}),
        json!(null),
    ] {
        let response = app.put_json(&uri, &request).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{request}");
        let body: Value = response.json();
        assert_eq!(body["status"], 400, "{request}: {body}");
    }
    assert!(agent.frames().is_empty());

    let response = app
        .get_auth(&format!(
            "/v1/admin/game-servers/{server_id}/updates/{}",
            uuid::Uuid::now_v7()
        ))
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
    let response = app
        .get_auth(&format!(
            "/v1/admin/game-servers/{server_id}/updates/not-a-uuid"
        ))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let response = app
        .get_auth(&format!(
            "/v1/admin/game-servers/{}/updates",
            uuid::Uuid::now_v7()
        ))
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
    agent.close().await;
}

// =============================================================================
// Start → progress → result → ack
// =============================================================================

#[tokio::test]
async fn test_start_progress_result_ack_happy_path() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Happy box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({"demo_upload_wired": true, "overrides_applied": null, "reload_pending": false}),
    )
    .await;

    // The agent's update_state on connect: a check that found updates.
    agent
        .send(json!({
            "type": "update_state",
            "check": {
                "checked_at": "2026-11-21T03:00:07Z", "job_id": AGENT_JOB_ID,
                "installed": {"lgsm": "v26.6.0", "cs2_build": "20012345", "metamod": null,
                              "cssharp": "1.0.370", "matchzy": "0.9.2"},
                "latest": {"lgsm": "v26.7.0", "cs2_build": "20012345",
                           "metamod": "mmsource-2.0.0-git1491-linux.tar.gz",
                           "cssharp": "1.0.380", "matchzy": "0.9.3"},
                "update_available": {"lgsm": true, "game": false, "metamod": null,
                                     "cssharp": true, "matchzy": true, "mods": true},
                "load": {"metamod": true, "cssharp": true, "matchzy": true, "error": null},
                "demo_upload_block_present": true
            },
            "installed": null
        }))
        .await;
    let data = wait_for(10, || async {
        let data = overview(&app, &server_id).await;
        (!data["check"].is_null()).then_some(data)
    })
    .await;
    assert_eq!(data["installed"]["matchzy"], "0.9.2", "{data}");
    assert!(data["installed"]["metamod"].is_null());
    assert_eq!(data["check"]["latest"]["lgsm"], "v26.7.0");
    assert_eq!(data["check"]["checked_at"], "2026-11-21T03:00:07Z");
    assert_eq!(data["check"]["update_available"]["lgsm"], true);
    assert_eq!(data["check"]["update_available"]["game"], false);
    assert!(data["check"]["update_available"]["metamod"].is_null());
    assert_eq!(data["check"]["update_available"]["mods"], true);
    assert_eq!(data["load"]["matchzy"], true);
    assert_eq!(data["demo_upload_wired"], true);
    assert_eq!(data["safety"]["humans_connected"], 0);
    assert_eq!(data["safety"]["restart_needs_force"], false);

    // Start: the job exists and the agent was told, under the portal's id.
    let (status, body) = start(&app, &server_id, &json!({"operation": "mods"})).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let started = &body["data"];
    let job_id = started["id"].as_str().unwrap().to_string();
    assert_eq!(started["operation"], "mods");
    assert_eq!(started["status"], "queued");
    assert_eq!(started["trigger"], "portal");
    assert_eq!(started["forced"], false);
    assert_eq!(started["blocks_allocation"], true);
    assert!(!started["requested_by"].is_null());
    let sent = agent.commands("update_start");
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["args"]["operation"], "mods");
    assert_eq!(sent[0]["args"]["job_id"], job_id.as_str());
    assert_eq!(
        job_id,
        job_id.to_lowercase(),
        "the agent wants a lowercase UUID"
    );
    assert_eq!(
        overview(&app, &server_id).await["active_job"]["id"],
        job_id.as_str()
    );

    // Progress.
    agent
        .send(progress(&job_id, "mods", "snapshot", "portal"))
        .await;
    agent
        .send(progress(&job_id, "mods", "mods-update", "portal"))
        .await;
    let running = job_when(&app, &server_id, &job_id, |j| j["phase"] == "mods-update").await;
    assert_eq!(running["status"], "running");
    assert_eq!(running["log_tail"], "[mods-update] working\n");
    assert!(!running["started_at"].is_null());
    assert!(running["finished_at"].is_null());
    assert!(
        agent.commands("update_ack").is_empty(),
        "progress is not acked"
    );

    // Result.
    agent.send(mods_result(&job_id, "portal")).await;
    let done = job_when(&app, &server_id, &job_id, |j| j["status"] == "succeeded").await;
    assert_eq!(done["phase"], "done");
    assert_eq!(done["restarted"], true);
    assert_eq!(done["agent_synthesized"], false);
    assert_eq!(done["blocks_allocation"], false);
    assert_eq!(done["before"]["matchzy"], "0.9.2");
    assert_eq!(done["after"]["matchzy"], "0.9.3");
    assert_eq!(done["load"]["cssharp"], true);
    assert_eq!(
        done["config_report"]["cvars_changed"][0]["name"],
        "matchzy_kick_when_no_match_loaded"
    );
    assert_eq!(done["config_report"]["cvars_changed"][0]["after"], "true");
    assert_eq!(
        done["config_report"]["cvars_removed"],
        json!(["matchzy_max_tech_pauses_allowed"])
    );
    assert_eq!(
        done["config_report"]["files_changed"],
        json!(["config.cfg", "live.cfg"])
    );
    assert_eq!(done["log_tail"], "[done] all good\n");
    assert_eq!(done["started_at"], "2026-11-21T04:00:00Z");
    assert_eq!(done["finished_at"], "2026-11-21T04:03:10Z");
    assert_eq!(
        done["requested_by_username"],
        started["requested_by_username"]
    );

    // Ack, for exactly this job.
    let acks = agent.wait_for_commands("update_ack", 1).await;
    assert_eq!(acks[0]["args"]["job_id"], job_id.as_str());

    // The overview moved on without a new check: the plugins are current,
    // LinuxGSM is still outdated.
    let data = overview(&app, &server_id).await;
    assert!(data["active_job"].is_null(), "{data}");
    assert_eq!(data["recent_jobs"][0]["id"], job_id.as_str());
    assert_eq!(data["recent_jobs"][0]["status"], "succeeded");
    assert!(
        data["recent_jobs"][0].get("log_tail").is_none(),
        "summaries carry no log"
    );
    assert_eq!(data["installed"]["matchzy"], "0.9.3");
    assert_eq!(
        data["installed"]["metamod"],
        "mmsource-2.0.0-git1491-linux.tar.gz"
    );
    assert_eq!(data["check"]["update_available"]["matchzy"], false);
    assert_eq!(data["check"]["update_available"]["metamod"], false);
    assert_eq!(data["check"]["update_available"]["mods"], false);
    assert_eq!(data["check"]["update_available"]["lgsm"], true);

    // Audited with the acting admin: the start and the finish.
    let rows = history(&app, &server_id).await;
    let start_row = rows
        .iter()
        .find(|r| r["kind"] == "update:mods")
        .expect("start audited");
    assert_eq!(start_row["ok"], true);
    assert_eq!(start_row["command"], format!("update_start mods {job_id}"));
    assert_eq!(
        start_row["admin_user_id"], started["requested_by"],
        "{start_row}"
    );
    let finish_row = rows
        .iter()
        .find(|r| r["kind"] == "update_result:mods")
        .expect("finish audited");
    assert_eq!(finish_row["ok"], true);
    assert_eq!(finish_row["output"], "succeeded");
    assert_eq!(finish_row["admin_user_id"], started["requested_by"]);
    agent.close().await;
}

#[tokio::test]
async fn test_check_is_sent_and_heartbeat_health_is_shown() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Check box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        None,
        json!({"demo_upload_wired": false, "overrides_applied": false, "reload_pending": true,
               "update_job": {"job_id": AGENT_JOB_ID, "operation": "check",
                              "phase": "check", "status": "running"}}),
    )
    .await;

    let data = overview(&app, &server_id).await;
    assert_eq!(data["demo_upload_wired"], false, "{data}");
    assert_eq!(data["overrides_applied"], false);
    assert_eq!(data["reload_pending"], true);
    assert_eq!(data["agent_job"]["operation"], "check");
    assert_eq!(data["agent_job"]["job_id"], AGENT_JOB_ID);
    // A check in flight is not a job row.
    assert!(data["active_job"].is_null());

    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{server_id}/updates/check"),
            &json!({}),
        )
        .await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: Value = response.json();
    assert_eq!(body["data"]["job_id"], AGENT_JOB_ID);
    assert_eq!(body["data"]["operation"], "check");
    assert_eq!(agent.commands("update_check").len(), 1);
    assert!(
        history(&app, &server_id)
            .await
            .iter()
            .any(|r| r["kind"] == "update:check" && r["ok"] == true)
    );
    assert_eq!(job_count(&app, &server_id).await, 0);

    // A FAILED check is reported as a result and becomes a row.
    let mut failed = failed_result(AGENT_JOB_ID, "check", "check-update: steamcmd timed out");
    failed["phase"] = json!("check");
    agent.send(failed).await;
    let row = job_when(&app, &server_id, AGENT_JOB_ID, |j| j["status"] == "failed").await;
    assert_eq!(row["operation"], "check");
    assert_eq!(row["error"], "check-update: steamcmd timed out");
    assert!(row["requested_by"].is_null());
    agent.wait_for_commands("update_ack", 1).await;

    // The next heartbeat clears what the last one said.
    agent
        .heartbeat_with(
            "none",
            None,
            json!({"capabilities": ["lgsm_update"], "demo_upload_wired": true,
                   "overrides_applied": null, "reload_pending": false, "update_job": null}),
        )
        .await;
    let data = wait_for(10, || async {
        let data = overview(&app, &server_id).await;
        (data["demo_upload_wired"] == true).then_some(data)
    })
    .await;
    assert!(data["overrides_applied"].is_null());
    assert_eq!(data["reload_pending"], false);
    assert!(data["agent_job"].is_null());
    agent.close().await;
}

#[tokio::test]
async fn test_a_rolled_back_result_is_stored_with_its_report() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Rollback box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let job_id = start_ok(&app, &server_id, &json!({"operation": "all"})).await;

    agent
        .send(progress(&job_id, "all", "verify", "portal"))
        .await;
    let mut result = mods_result(&job_id, "portal");
    result["operation"] = json!("all");
    result["status"] = json!("rolled_back");
    result["phase"] = json!("rollback");
    result["failed_phase"] = json!("verify");
    result["after"] = result["before"].clone();
    result["load"] = json!({"metamod": true, "cssharp": true, "matchzy": true, "error": null});
    result["error"] = json!("MMS: Fatal error: plugin failed to load");
    result["log_tail"] = json!("x".repeat(20_000));
    agent.send(result).await;

    let done = job_when(&app, &server_id, &job_id, |j| j["status"] == "rolled_back").await;
    assert_eq!(done["phase"], "rollback");
    assert_eq!(done["failed_phase"], "verify");
    assert_eq!(done["error"], "MMS: Fatal error: plugin failed to load");
    assert_eq!(done["after"], done["before"]);
    assert_eq!(done["blocks_allocation"], false);
    // Whatever report the helper sent is kept.
    assert_eq!(
        done["config_report"]["cvars_added"],
        json!(["matchzy_max_tech_pauses"])
    );
    assert_eq!(
        done["log_tail"].as_str().unwrap().len(),
        8 * 1024,
        "the log tail is bounded"
    );
    agent.wait_for_commands("update_ack", 1).await;

    // A rollback leaves the previous set installed: nothing moved on.
    let data = overview(&app, &server_id).await;
    assert!(data["installed"].is_null(), "{data}");
    assert_eq!(
        data["load"]["matchzy"], true,
        "the job's load reading is shown"
    );
    let rows = history(&app, &server_id).await;
    let finish = rows
        .iter()
        .find(|r| r["kind"] == "update_result:all")
        .expect("finish audited");
    assert_eq!(finish["ok"], false);
    assert_eq!(
        finish["output"],
        "rolled_back: MMS: Fatal error: plugin failed to load"
    );

    // A rollback without a report (what the helper actually sends).
    let job2 = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;
    let mut result = mods_result(&job2, "portal");
    result["status"] = json!("rolled_back");
    result["config_report"] = json!(null);
    result["load"] = json!(null);
    agent.send(result).await;
    let done = job_when(&app, &server_id, &job2, |j| j["status"] == "rolled_back").await;
    assert!(done["config_report"].is_null());
    assert!(done["load"].is_null());
    agent.close().await;
}

// =============================================================================
// At-least-once delivery
// =============================================================================

#[tokio::test]
async fn test_a_replayed_result_is_idempotent_and_acked_again() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Replay box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let job_id = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;

    agent.send(mods_result(&job_id, "portal")).await;
    agent.wait_for_commands("update_ack", 1).await;
    let first = job(&app, &server_id, &job_id).await;
    assert_eq!(first["status"], "succeeded");

    // The same result again — and a variant claiming something else, which
    // must not overwrite what is stored.
    agent.send(mods_result(&job_id, "portal")).await;
    let mut imposter = mods_result(&job_id, "portal");
    imposter["status"] = json!("failed");
    imposter["error"] = json!("not what happened");
    agent.send(imposter).await;
    // A stray late progress frame must not bring the job back to life.
    agent
        .send(progress(&job_id, "mods", "verify", "portal"))
        .await;
    let acks = agent.wait_for_commands("update_ack", 3).await;
    assert!(
        acks.iter().all(|a| a["args"]["job_id"] == job_id.as_str()),
        "{acks:?}"
    );

    // Give the progress frame time to (not) land.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let after = job(&app, &server_id, &job_id).await;
    assert_eq!(after, first, "a replay changes nothing");
    assert_eq!(job_count(&app, &server_id).await, 1);
    assert!(overview(&app, &server_id).await["active_job"].is_null());
    // One finish in the audit trail, not three.
    let finishes = history(&app, &server_id)
        .await
        .iter()
        .filter(|r| r["kind"] == "update_result:mods")
        .count();
    assert_eq!(finishes, 1);

    // A reconnect replays it once more (the agent never saw our ack).
    agent.close().await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    agent.send(mods_result(&job_id, "portal")).await;
    agent.wait_for_commands("update_ack", 1).await;
    assert_eq!(job(&app, &server_id, &job_id).await, first);
    agent.close().await;
}

#[tokio::test]
async fn test_a_real_result_supersedes_a_synthesized_failure() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Synth box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let job_id = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;

    // The helper left no trace: the agent makes a failure up.
    // Exactly the agent's shape (`Updater::settle_pending` + `result_frame`):
    // no trigger, no timestamps, no versions, no log.
    let synthesized = json!({
        "type": "update_result", "job_id": job_id, "operation": "mods",
        "status": "failed", "phase": "starting", "failed_phase": "starting",
        "agent_synthesized": true,
        "error": "the update helper ended without reporting anything (it could not get \
                  the lock, or the unit failed - see journalctl)",
    });
    agent.send(synthesized.clone()).await;
    let failed = job_when(&app, &server_id, &job_id, |j| j["status"] == "failed").await;
    assert_eq!(failed["agent_synthesized"], true);
    assert_eq!(failed["phase"], "starting");
    // The portal's own record of who started it stands.
    assert_eq!(failed["trigger"], "portal");
    assert!(!failed["requested_by"].is_null());
    assert!(overview(&app, &server_id).await["active_job"].is_null());
    agent.wait_for_commands("update_ack", 1).await;

    // It had finished after all: the real result follows under the same id.
    agent.send(mods_result(&job_id, "portal")).await;
    let done = job_when(&app, &server_id, &job_id, |j| j["status"] == "succeeded").await;
    assert_eq!(done["agent_synthesized"], false);
    assert_eq!(done["phase"], "done");
    assert!(done["error"].is_null());
    assert_eq!(done["after"]["matchzy"], "0.9.3");
    agent.wait_for_commands("update_ack", 2).await;

    // The reverse never happens: a synthesized failure does not replace a
    // real outcome.
    agent.send(synthesized).await;
    agent.wait_for_commands("update_ack", 3).await;
    let still = job(&app, &server_id, &job_id).await;
    assert_eq!(still["status"], "succeeded");
    assert_eq!(still["agent_synthesized"], false);
    assert_eq!(job_count(&app, &server_id).await, 1);
    agent.close().await;
}

// =============================================================================
// Jobs the portal did not start
// =============================================================================

#[tokio::test]
async fn test_jobs_started_on_the_host_get_a_row() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "upd-local").await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Cron box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;

    // The nightly cron: progress arrives for an id the portal never made.
    agent
        .send(progress(LOCAL_JOB, "game", "game-update", "local"))
        .await;
    let running = job_when(&app, &server_id, LOCAL_JOB, |j| j["status"] == "running").await;
    assert_eq!(running["operation"], "game");
    assert_eq!(running["trigger"], "local");
    assert!(running["requested_by"].is_null());
    assert!(running["requested_by_username"].is_null());
    assert_eq!(running["forced"], false);
    assert_eq!(running["blocks_allocation"], true);
    let data = overview(&app, &server_id).await;
    assert_eq!(data["active_job"]["id"], LOCAL_JOB);

    // It holds the server out of allocation and blocks a portal job, like
    // any other.
    assert!(
        try_allocate(&state, &game_id, &tournament_id, &match_id)
            .await
            .is_none()
    );
    let (status, body) = start(&app, &server_id, &json!({"operation": "lgsm"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains(LOCAL_JOB),
        "{body}"
    );

    let mut result = mods_result(LOCAL_JOB, "local");
    result["operation"] = json!("game");
    result["config_report"] = json!(null);
    result["after"]["cs2_build"] = json!("20012399");
    agent.send(result).await;
    let done = job_when(&app, &server_id, LOCAL_JOB, |j| j["status"] == "succeeded").await;
    assert_eq!(done["trigger"], "local");
    assert_eq!(done["after"]["cs2_build"], "20012399");
    let acks = agent.wait_for_commands("update_ack", 1).await;
    assert_eq!(acks[0]["args"]["job_id"], LOCAL_JOB);
    assert_eq!(
        overview(&app, &server_id).await["installed"]["cs2_build"],
        "20012399"
    );

    // An auto-repair the portal only hears about when it is over.
    let repair_id = "0193a5b0-7c1e-7000-8000-00000000a070";
    agent
        .send(json!({
            "type": "update_result", "job_id": repair_id, "operation": "repair-config",
            "trigger": "auto_repair", "status": "succeeded", "phase": "done",
            "restarted": false, "needs_reload": true, "config_changed": true,
            "config_report": {"cvars_changed": [], "cvars_removed": [], "cvars_added": [],
                              "files_changed": ["config.cfg"]},
            "load": null, "error": null, "log_tail": "re-asserted the demo upload block\n",
        }))
        .await;
    let repaired = job_when(&app, &server_id, repair_id, |j| j["status"] == "succeeded").await;
    assert_eq!(repaired["operation"], "repair-config");
    assert_eq!(repaired["trigger"], "auto_repair");
    assert_eq!(repaired["restarted"], false);
    assert_eq!(
        repaired["config_report"]["files_changed"],
        json!(["config.cfg"])
    );
    assert!(repaired["requested_by"].is_null());
    agent.wait_for_commands("update_ack", 2).await;

    // No admin started either: nothing is attributed in the audit trail.
    assert!(
        history(&app, &server_id)
            .await
            .iter()
            .all(|r| !r["kind"].as_str().unwrap().starts_with("update_result")),
    );

    // Frames the portal cannot use are dropped without harm: no job id, an
    // unknown operation (acknowledged, or it would replay forever), a
    // frame type from the future.
    agent
        .send(json!({"type": "update_result", "status": "succeeded"}))
        .await;
    agent
        .send(json!({"type": "update_progress", "job_id": "nope"}))
        .await;
    let odd = "0193a5b0-7c1e-7000-8000-000000000dd0";
    agent
        .send(
            json!({"type": "update_result", "job_id": odd, "operation": "defrag",
                     "status": "succeeded"}),
        )
        .await;
    agent
        .send(json!({"type": "update_telemetry", "x": 1}))
        .await;
    let acks = agent.wait_for_commands("update_ack", 3).await;
    assert_eq!(acks[2]["args"]["job_id"], odd);
    assert_eq!(job_count(&app, &server_id).await, 2);
    // The connection survived all of it.
    assert_eq!(overview(&app, &server_id).await["agent_connected"], true);
    agent.close().await;
}

#[tokio::test]
async fn test_one_server_cannot_touch_another_servers_job() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_a = create_server(&app, &game_id, "Box A").await;
    let server_b = create_server(&app, &game_id, "Box B").await;
    let agent_a = connect_capable(
        &mut app,
        &server_a,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let agent_b = connect_capable(&mut app, &server_b, update_script(), None, json!({})).await;
    let job_id = start_ok(&app, &server_a, &json!({"operation": "mods"})).await;

    // B reports on A's job id.
    agent_b
        .send(progress(&job_id, "mods", "stop", "portal"))
        .await;
    let mut forged = mods_result(&job_id, "portal");
    forged["status"] = json!("failed");
    agent_b.send(forged).await;
    agent_b.wait_for_commands("update_ack", 1).await;

    let untouched = job(&app, &server_a, &job_id).await;
    assert_eq!(untouched["status"], "queued", "{untouched}");
    assert!(untouched["phase"].is_null());
    // And the job is not visible through B.
    let response = app
        .get_auth(&format!(
            "/v1/admin/game-servers/{server_b}/updates/{job_id}"
        ))
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(job_count(&app, &server_b).await, 0);
    agent_a.close().await;
    agent_b.close().await;
}

// =============================================================================
// Safety gates
// =============================================================================

#[tokio::test]
async fn test_connected_players_gate_restarting_updates_unless_forced() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Busy box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(BUSY_STATUS),
        json!({}),
    )
    .await;

    let data = overview(&app, &server_id).await;
    assert_eq!(data["safety"]["humans_connected"], 2, "{data}");
    assert_eq!(
        data["safety"]["players"],
        json!(["Player One", "Player Two"])
    );
    assert_eq!(data["safety"]["restart_needs_force"], true);
    assert_eq!(
        data["safety"]["reasons"],
        json!(["2 players are connected"])
    );

    for operation in ["game", "mods", "all"] {
        let (status, body) = start(&app, &server_id, &json!({"operation": operation})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{operation}: {body}");
        let detail = body["detail"].as_str().unwrap();
        assert!(detail.contains("2 players are connected"), "{detail}");
        assert!(detail.contains("force"), "{detail}");
        // `force: false` spelled out is the same.
        let (status, _) = start(
            &app,
            &server_id,
            &json!({"operation": operation, "force": false}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }
    assert!(agent.commands("update_start").is_empty());
    // Each refusal is a job the gate closed before anything was sent.
    assert_gated(&app, &server_id, 6).await;

    // `lgsm` and `repair-config` restart nothing: no gate.
    let lgsm = start_ok(&app, &server_id, &json!({"operation": "lgsm"})).await;
    let started = job(&app, &server_id, &lgsm).await;
    assert_eq!(started["forced"], false);
    assert_eq!(started["blocks_allocation"], false);
    let mut result = mods_result(&lgsm, "portal");
    result["operation"] = json!("lgsm");
    agent.send(result).await;
    job_when(&app, &server_id, &lgsm, |j| j["status"] == "succeeded").await;

    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{server_id}/updates/repair-config"),
            &json!({}),
        )
        .await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: Value = response.json();
    let repair = body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["operation"], "repair-config");
    assert_eq!(
        agent.commands("update_start").last().unwrap()["args"]["operation"],
        "repair-config"
    );
    let mut result = mods_result(&repair, "portal");
    result["operation"] = json!("repair-config");
    agent.send(result).await;
    job_when(&app, &server_id, &repair, |j| j["status"] == "succeeded").await;

    // Forced: it goes ahead and says so.
    let (status, body) = start(
        &app,
        &server_id,
        &json!({"operation": "mods", "force": true}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["data"]["forced"], true);
    let forced_id = body["data"]["id"].as_str().unwrap();
    let rows = history(&app, &server_id).await;
    let row = rows
        .iter()
        .find(|r| r["command"] == format!("update_start mods {forced_id}"))
        .expect("forced start audited");
    assert_eq!(row["force"], true);
    agent.close().await;
}

#[tokio::test]
async fn test_a_live_reservation_or_an_imminent_booking_gates_restarting_updates() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "upd-gate").await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Reserved box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    wait_for(10, || async {
        let body: Value = app
            .get_auth(&format!("/v1/admin/game-servers/{server_id}"))
            .await
            .json();
        (body["data"]["status"] == "available").then_some(())
    })
    .await;

    // A booking that starts in ten minutes.
    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{server_id}/bookings"),
            &json!({
                "reason": "Community night",
                "starts_at": Utc::now() + Duration::minutes(10),
                "ends_at": Utc::now() + Duration::hours(3),
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let booking: Value = response.json();
    let booking_id = booking["data"]["id"].as_str().unwrap().to_string();

    let data = overview(&app, &server_id).await;
    assert_eq!(data["safety"]["humans_connected"], 0);
    assert_eq!(data["safety"]["bookings"][0]["reason"], "Community night");
    assert_eq!(data["safety"]["restart_needs_force"], true, "{data}");
    let (status, body) = start(&app, &server_id, &json!({"operation": "game"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("a booking starts at"),
        "{body}"
    );
    assert!(body["detail"].as_str().unwrap().contains("Community night"));

    // A booking further out is not imminent.
    app.delete_auth(&format!(
        "/v1/admin/game-servers/{server_id}/bookings/{booking_id}"
    ))
    .await
    .assert_status(StatusCode::NO_CONTENT);
    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{server_id}/bookings"),
            &json!({
                "reason": "Tomorrow",
                "starts_at": Utc::now() + Duration::minutes(45),
                "ends_at": Utc::now() + Duration::hours(3),
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let data = overview(&app, &server_id).await;
    assert_eq!(data["safety"]["bookings"], json!([]));
    assert_eq!(data["safety"]["restart_needs_force"], false, "{data}");

    // A live reservation.
    let reservation = try_allocate(&state, &game_id, &tournament_id, &match_id)
        .await
        .expect("the idle server is allocatable");
    let data = overview(&app, &server_id).await;
    assert_eq!(data["safety"]["reservation"]["match_id"], match_id.as_str());
    assert_eq!(
        data["safety"]["reservation"]["id"],
        reservation.id.to_string()
    );
    assert_eq!(data["safety"]["restart_needs_force"], true);
    for operation in ["game", "mods", "all"] {
        let (status, body) = start(&app, &server_id, &json!({"operation": operation})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{operation}: {body}");
        let detail = body["detail"].as_str().unwrap();
        assert!(detail.contains("reservation is live"), "{detail}");
        assert!(detail.contains(&match_id), "{detail}");
    }
    assert!(agent.commands("update_start").is_empty());
    // Each refusal is a job the gate closed before anything was sent.
    assert_gated(&app, &server_id, 4).await;

    // Forced through.
    let (status, body) = start(
        &app,
        &server_id,
        &json!({"operation": "all", "force": true}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["data"]["forced"], true);
    assert_eq!(agent.commands("update_start").len(), 1);
    agent.close().await;
}

/// The gate fails closed: a player count it cannot know, or a match the
/// server reports, needs `force` like connected players do.
#[tokio::test]
async fn test_unknown_players_or_a_loaded_match_gate_restarting_updates() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Unknown box").await;
    // Connected and capable, but no CS2 `status` in its heartbeat.
    let agent = connect_capable(&mut app, &server_id, update_script(), None, json!({})).await;

    let data = overview(&app, &server_id).await;
    assert!(data["safety"]["humans_connected"].is_null(), "{data}");
    assert_eq!(data["safety"]["restart_needs_force"], true, "{data}");
    assert_eq!(
        data["safety"]["reasons"],
        json!(["player count unknown (no fresh CS2 status from the agent in the last 2 minutes)"])
    );
    for operation in ["game", "mods", "all"] {
        let (status, body) = start(&app, &server_id, &json!({"operation": operation})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{operation}: {body}");
        assert!(
            body["detail"]
                .as_str()
                .unwrap()
                .contains("player count unknown"),
            "{body}"
        );
    }
    // `lgsm` restarts nothing and is not gated.
    let lgsm = start_ok(&app, &server_id, &json!({"operation": "lgsm"})).await;
    let mut result = mods_result(&lgsm, "portal");
    result["operation"] = json!("lgsm");
    agent.send(result).await;
    job_when(&app, &server_id, &lgsm, |j| j["status"] == "succeeded").await;

    // Nobody connected, but a match is live the portal did not set up.
    agent
        .heartbeat_with("live", Some(EMPTY_STATUS), capable())
        .await;
    let data = wait_for(10, || async {
        let data = overview(&app, &server_id).await;
        (data["safety"]["humans_connected"] == 0
            && data["safety"]["reasons"].as_array().is_some_and(|r| {
                r.iter()
                    .any(|x| x == "MatchZy has a match loaded (gamestate live)")
            }))
        .then_some(data)
    })
    .await;
    assert_eq!(data["safety"]["restart_needs_force"], true);
    let (status, body) = start(&app, &server_id, &json!({"operation": "mods"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("gamestate live"),
        "{body}"
    );
    assert!(
        agent.commands("update_start").len() == 1,
        "only lgsm was sent"
    );

    // Force overrides every one of them.
    let (status, body) = start(
        &app,
        &server_id,
        &json!({"operation": "mods", "force": true}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(agent.commands("update_start").len(), 2);
    agent.close().await;
}

// =============================================================================
// One job at a time
// =============================================================================

#[tokio::test]
async fn test_one_job_at_a_time_and_a_refused_start_leaves_no_queued_row() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Serial box").await;
    // The agent's answer to update_start is switched by the operation.
    let script: FrameScript = Arc::new(|frame: &Value| {
        if frame["cmd"] == "update_start" {
            match frame["args"]["operation"].as_str().unwrap_or_default() {
                "game" => (
                    false,
                    "refused: update job 0193a5b0-7c1e-7000-8000-0000000010ca (game) is \
                     already running"
                        .to_string(),
                ),
                "all" => (
                    false,
                    "could not start portal-gs-update@all.service: Interactive authentication \
                     required."
                        .to_string(),
                ),
                _ => update_script()(frame),
            }
        } else {
            update_script()(frame)
        }
    });
    let agent = connect_capable(&mut app, &server_id, script, Some(EMPTY_STATUS), json!({})).await;

    // The agent refuses (cron got there first): 409, and the row is closed.
    let (status, body) = start(&app, &server_id, &json!({"operation": "game"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("is already running"),
        "{body}"
    );
    let data = overview(&app, &server_id).await;
    assert!(data["active_job"].is_null(), "{data}");
    assert_eq!(data["recent_jobs"][0]["status"], "failed");
    assert_eq!(data["recent_jobs"][0]["operation"], "game");
    assert!(
        data["recent_jobs"][0]["error"]
            .as_str()
            .unwrap()
            .starts_with("refused: update job"),
    );
    assert!(!data["recent_jobs"][0]["finished_at"].is_null());

    // systemd/polkit said no: that is the host's failure, 502.
    let (status, body) = start(&app, &server_id, &json!({"operation": "all"})).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(
        body["detail"].as_str().unwrap().contains("could not start"),
        "{body}"
    );
    let data = overview(&app, &server_id).await;
    assert!(data["active_job"].is_null());
    assert_eq!(data["recent_jobs"][0]["status"], "failed");
    let rows = history(&app, &server_id).await;
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"].as_str().unwrap().starts_with("update:") && r["ok"] == false)
            .count(),
        2,
        "both refusals are audited"
    );

    // An accepted job, then everything else is a conflict naming it.
    let job_id = start_ok(&app, &server_id, &json!({"operation": "lgsm"})).await;
    for body in [
        json!({"operation": "lgsm"}),
        json!({"operation": "mods"}),
        json!({"operation": "mods", "force": true}),
    ] {
        let (status, problem) = start(&app, &server_id, &body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert!(
            problem["detail"].as_str().unwrap().contains(&job_id),
            "{problem}"
        );
    }
    let response = app
        .post_json(
            &format!("/v1/admin/game-servers/{server_id}/updates/repair-config"),
            &json!({}),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
    assert_eq!(agent.commands("update_start").len(), 3);
    assert_eq!(job_count(&app, &server_id).await, 3);

    // Over: the next one may start.
    let mut result = mods_result(&job_id, "portal");
    result["operation"] = json!("lgsm");
    agent.send(result).await;
    job_when(&app, &server_id, &job_id, |j| j["status"] == "succeeded").await;
    start_ok(&app, &server_id, &json!({"operation": "mods"})).await;
    agent.close().await;
}

#[tokio::test]
async fn test_a_start_the_agent_drops_is_lost_and_revived_by_progress() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Dropped box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;

    // The agent goes away between gate and send: nothing was started.
    agent.close().await;
    wait_for(10, || async {
        (overview(&app, &server_id).await["agent_connected"] == false).then_some(())
    })
    .await;
    let (status, _) = start(&app, &server_id, &json!({"operation": "mods"})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(job_count(&app, &server_id).await, 0);

    // A portal-side `lost` verdict is a guess; the agent saying the job is
    // running is better evidence, and its result settles it.
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let job_id = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;
    sqlx::query(
        "UPDATE game_server_update_jobs SET status = 'lost', error = 'gave up', \
         finished_at = NOW() WHERE id = $1::uuid",
    )
    .bind(&job_id)
    .execute(app.pool())
    .await
    .unwrap();
    assert!(overview(&app, &server_id).await["active_job"].is_null());

    agent
        .send(progress(&job_id, "mods", "verify", "portal"))
        .await;
    let revived = job_when(&app, &server_id, &job_id, |j| j["status"] == "running").await;
    assert_eq!(revived["phase"], "verify");
    assert!(revived["error"].is_null());
    assert!(revived["finished_at"].is_null());
    assert_eq!(
        overview(&app, &server_id).await["active_job"]["id"],
        job_id.as_str()
    );

    agent.send(mods_result(&job_id, "portal")).await;
    job_when(&app, &server_id, &job_id, |j| j["status"] == "succeeded").await;
    agent.close().await;
}

/// `update_start` went out and the agent vanished before answering: the
/// helper may be running. The job is not declared over — it stays queued
/// (and the server out of allocation) until the agent says what happened.
#[tokio::test]
async fn test_an_unanswered_start_stays_queued_until_the_agent_reports() {
    let (mut app, state) = TestApp::new_with_agent_dev_auth_and_state().await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "upd-unknown").await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Unanswered box").await;
    let dropper: FrameScript = Arc::new(|frame: &Value| {
        if frame["cmd"] == "update_start" {
            (true, DROP_CONNECTION.to_string())
        } else {
            update_script()(frame)
        }
    });
    let agent = connect_capable(&mut app, &server_id, dropper, Some(EMPTY_STATUS), json!({})).await;

    let (status, body) = start(&app, &server_id, &json!({"operation": "mods"})).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    let detail = body["detail"].as_str().unwrap();
    assert!(
        detail.contains("whether the update started is unknown"),
        "{detail}"
    );
    assert!(detail.contains("stays queued"), "{detail}");
    assert!(detail.contains("not allocated to matches"), "{detail}");
    assert_eq!(agent.commands("update_start").len(), 1);
    let job_id = agent.commands("update_start")[0]["args"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(detail.contains(&job_id), "{detail}");
    agent.close().await;

    let queued = job(&app, &server_id, &job_id).await;
    assert_eq!(queued["status"], "queued", "{queued}");
    assert!(queued["finished_at"].is_null());
    assert_eq!(queued["blocks_allocation"], true);
    let rows = history(&app, &server_id).await;
    let row = rows
        .iter()
        .find(|r| r["command"] == format!("update_start mods {job_id}"))
        .expect("the unanswered start is audited");
    assert_eq!(row["ok"], false);

    // The agent is back and idle, but the job may still restart CS2: the
    // server stays out of allocation, and nothing else may start.
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let server_uri = format!("/v1/admin/game-servers/{server_id}");
    wait_for(10, || async {
        let body: Value = app.get_auth(&server_uri).await.json();
        (body["data"]["status"] == "available").then_some(())
    })
    .await;
    assert!(
        try_allocate(&state, &game_id, &tournament_id, &match_id)
            .await
            .is_none(),
        "an unanswered mods start keeps the server out of allocation"
    );
    let (status, body) = start(&app, &server_id, &json!({"operation": "lgsm"})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // It had started after all: progress moves it on, the result ends it.
    let stalled = state
        .game_server_updates
        .service
        .find_job(server_id.parse().unwrap(), job_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    agent
        .send(progress(&job_id, "mods", "mods-update", "portal"))
        .await;
    job_when(&app, &server_id, &job_id, |j| j["status"] == "running").await;
    // A lost verdict reached on the job as it was before the progress
    // landed changes nothing.
    let verdict = state
        .game_server_updates
        .service
        .mark_lost(&stalled, "test: a stale verdict")
        .await
        .unwrap();
    assert!(verdict.is_none());
    assert_eq!(job(&app, &server_id, &job_id).await["status"], "running");

    agent.send(mods_result(&job_id, "portal")).await;
    job_when(&app, &server_id, &job_id, |j| j["status"] == "succeeded").await;

    // Never heard of again: the lost-job pass settles it.
    let dropper: FrameScript = Arc::new(|frame: &Value| {
        if frame["cmd"] == "update_start" {
            (true, DROP_CONNECTION.to_string())
        } else {
            update_script()(frame)
        }
    });
    agent.close().await;
    let agent = connect_capable(&mut app, &server_id, dropper, Some(EMPTY_STATUS), json!({})).await;
    let (status, _) = start(&app, &server_id, &json!({"operation": "all"})).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let job2 = agent.commands("update_start")[0]["args"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    agent.close().await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    assert_eq!(job(&app, &server_id, &job2).await["status"], "queued");
    age_job(&app, &job2).await;
    let summary = state.game_server_updates.run_lost_job_pass().await;
    assert_eq!((summary.lost, summary.errors), (1, 0));
    assert_eq!(job(&app, &server_id, &job2).await["status"], "lost");
    agent.close().await;
}

// =============================================================================
// Allocation
// =============================================================================

#[tokio::test]
async fn test_allocation_skips_a_server_with_a_restarting_job_in_flight() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "upd-alloc").await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Only box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let server_uri = format!("/v1/admin/game-servers/{server_id}");
    wait_for(10, || async {
        let body: Value = app.get_auth(&server_uri).await.json();
        (body["data"]["status"] == "available").then_some(())
    })
    .await;

    // A job that restarts nothing does not take the server away.
    let lgsm = start_ok(&app, &server_id, &json!({"operation": "lgsm"})).await;
    let reservation = try_allocate(&state, &game_id, &tournament_id, &match_id)
        .await
        .expect("an lgsm job does not block allocation");
    state
        .server_reservation_repo
        .release(
            reservation.id,
            portal_core::types::ReservationStatus::Cancelled,
            Some("test"),
        )
        .await
        .unwrap();
    let mut result = mods_result(&lgsm, "portal");
    result["operation"] = json!("lgsm");
    agent.send(result).await;
    job_when(&app, &server_id, &lgsm, |j| j["status"] == "succeeded").await;

    // Queued: already out of allocation, though its stored status is
    // untouched and it heartbeats as idle.
    let job_id = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;
    let body: Value = app.get_auth(&server_uri).await.json();
    assert_eq!(body["data"]["status"], "available");
    assert!(
        try_allocate(&state, &game_id, &tournament_id, &match_id)
            .await
            .is_none(),
        "a queued mods job keeps the server out of allocation"
    );

    // Running: still out, heartbeats or not.
    agent
        .send(progress(&job_id, "mods", "stop", "portal"))
        .await;
    job_when(&app, &server_id, &job_id, |j| j["status"] == "running").await;
    agent
        .heartbeat_with("none", Some(EMPTY_STATUS), capable())
        .await;
    assert!(
        try_allocate(&state, &game_id, &tournament_id, &match_id)
            .await
            .is_none(),
        "a running mods job keeps the server out of allocation"
    );

    // Any terminal status releases it — here a rollback.
    let mut result = mods_result(&job_id, "portal");
    result["status"] = json!("rolled_back");
    agent.send(result).await;
    job_when(&app, &server_id, &job_id, |j| j["status"] == "rolled_back").await;
    let reservation = try_allocate(&state, &game_id, &tournament_id, &match_id)
        .await
        .expect("the server is allocatable again once the job is over");
    assert_eq!(
        reservation.server_id.map(|s| s.to_string()).as_deref(),
        Some(server_id.as_str())
    );
    agent.close().await;
}

/// The restart gate and allocation race for the same idle server. The job
/// row goes in before the gate reads, so whichever side gets there second
/// sees the other: here the job insert is held back (the table lock) while
/// allocation runs, so the gate must see the reservation and refuse.
#[tokio::test]
async fn test_a_match_allocated_while_an_update_starts_is_seen_by_the_gate() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "upd-race").await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Race box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let server_uri = format!("/v1/admin/game-servers/{server_id}");
    wait_for(10, || async {
        let body: Value = app.get_auth(&server_uri).await.json();
        (body["data"]["status"] == "available").then_some(())
    })
    .await;
    assert_eq!(
        overview(&app, &server_id).await["safety"]["restart_needs_force"],
        false
    );

    // Hold INSERTs into the jobs table (reads still run).
    let mut locker = app.pool().begin().await.unwrap();
    sqlx::query("LOCK TABLE game_server_update_jobs IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *locker)
        .await
        .unwrap();

    let mods = json!({"operation": "mods"});
    let ((status, body), allocated) = tokio::join!(start(&app, &server_id, &mods), async {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let r = try_allocate(&state, &game_id, &tournament_id, &match_id).await;
        locker.commit().await.unwrap();
        r
    });

    let allocated = allocated.expect("the job was not in yet: the server was allocatable");
    assert_eq!(
        allocated.server_id.map(|s| s.to_string()).as_deref(),
        Some(server_id.as_str())
    );
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let detail = body["detail"].as_str().unwrap();
    assert!(detail.contains("reservation is live"), "{detail}");
    assert!(agent.commands("update_start").is_empty());
    assert_gated(&app, &server_id, 1).await;
    agent.close().await;
}

// =============================================================================
// Lost jobs
// =============================================================================

#[tokio::test]
async fn test_lost_job_pass_asks_the_agent_and_otherwise_gives_up() {
    let (mut app, state) = TestApp::new_with_agent_dev_auth_and_state().await;
    let alloc_state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "upd-lost").await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Silent box").await;

    // What the agent answers to update_status, switchable per step.
    let status_answer = Arc::new(std::sync::Mutex::new("null".to_string()));
    let answer = Arc::clone(&status_answer);
    let script: FrameScript = Arc::new(move |frame: &Value| {
        if frame["cmd"] == "update_status" {
            (true, answer.lock().unwrap().clone())
        } else {
            update_script()(frame)
        }
    });
    let agent = connect_capable(
        &mut app,
        &server_id,
        script.clone(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let updates = &state.game_server_updates;

    // A fresh job is left alone.
    let job_id = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;
    let summary = updates.run_lost_job_pass().await;
    assert_eq!(
        (summary.reconciled, summary.lost, summary.errors),
        (0, 0, 0)
    );
    assert!(agent.commands("update_status").is_empty());

    // Silent for 21 minutes, but the helper is still on it: it stays.
    age_job(&app, &job_id).await;
    *status_answer.lock().unwrap() =
        json!({"job_id": job_id, "operation": "mods", "status": "running",
               "phase": "mods-update", "trigger": "portal"})
        .to_string();
    let summary = updates.run_lost_job_pass().await;
    assert_eq!(
        (summary.reconciled, summary.lost, summary.errors),
        (1, 0, 0)
    );
    assert_eq!(agent.commands("update_status").len(), 1);
    let still = job(&app, &server_id, &job_id).await;
    assert_eq!(still["status"], "running");
    assert_eq!(still["phase"], "mods-update");
    // Its clock was reset: the next pass does not ask again.
    let summary = updates.run_lost_job_pass().await;
    assert_eq!((summary.reconciled, summary.lost), (0, 0));
    assert_eq!(agent.commands("update_status").len(), 1);

    // Silent again, and the helper had in fact finished: the real outcome
    // is stored from the state file.
    age_job(&app, &job_id).await;
    let mut finished = mods_result(&job_id, "portal");
    finished.as_object_mut().unwrap().remove("type");
    *status_answer.lock().unwrap() = finished.to_string();
    let summary = updates.run_lost_job_pass().await;
    assert_eq!(
        (summary.reconciled, summary.lost, summary.errors),
        (1, 0, 0)
    );
    let done = job(&app, &server_id, &job_id).await;
    assert_eq!(done["status"], "succeeded");
    assert_eq!(done["after"]["matchzy"], "0.9.3");
    assert!(
        history(&app, &server_id)
            .await
            .iter()
            .any(|r| r["kind"] == "update_result:mods" && r["output"] == "succeeded")
    );

    // Silent, and the host has moved on to something else: lost.
    let job2 = start_ok(&app, &server_id, &json!({"operation": "all"})).await;
    age_job(&app, &job2).await;
    *status_answer.lock().unwrap() =
        json!({"job_id": LOCAL_JOB, "operation": "game", "status": "succeeded"}).to_string();
    assert!(
        try_allocate(&alloc_state, &game_id, &tournament_id, &match_id)
            .await
            .is_none(),
        "held while the job is in flight"
    );
    let summary = updates.run_lost_job_pass().await;
    assert_eq!(
        (summary.reconciled, summary.lost, summary.errors),
        (0, 1, 0)
    );
    let lost = job(&app, &server_id, &job2).await;
    assert_eq!(lost["status"], "lost");
    assert!(
        lost["error"]
            .as_str()
            .unwrap()
            .contains("no record of this job"),
        "{lost}"
    );
    assert!(!lost["finished_at"].is_null());
    assert_eq!(lost["blocks_allocation"], false);
    assert!(overview(&app, &server_id).await["active_job"].is_null());
    assert!(
        history(&app, &server_id)
            .await
            .iter()
            .any(|r| r["kind"] == "update_result:all"
                && r["ok"] == false
                && r["output"].as_str().unwrap().starts_with("lost:"))
    );
    // A lost job no longer holds the server.
    let reservation = try_allocate(&alloc_state, &game_id, &tournament_id, &match_id)
        .await
        .expect("released when the job is declared lost");
    alloc_state
        .server_reservation_repo
        .release(
            reservation.id,
            portal_core::types::ReservationStatus::Cancelled,
            Some("test"),
        )
        .await
        .unwrap();

    // `null` (nothing has run since boot) is the same verdict.
    let job3 = start_ok(&app, &server_id, &json!({"operation": "lgsm"})).await;
    age_job(&app, &job3).await;
    *status_answer.lock().unwrap() = "null".to_string();
    let summary = updates.run_lost_job_pass().await;
    assert_eq!((summary.reconciled, summary.lost), (0, 1));
    assert_eq!(job(&app, &server_id, &job3).await["status"], "lost");

    // No agent to ask: lost — and the whole lifecycle pass reports it.
    let job4 = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;
    agent.close().await;
    wait_for(10, || async {
        (overview(&app, &server_id).await["agent_connected"] == false).then_some(())
    })
    .await;
    age_job(&app, &job4).await;
    let pass = portal_api::background::run_lifecycle_pass(
        &state,
        &portal_api::background::LifecycleConfig::from_env(),
        false,
    )
    .await;
    assert_eq!(pass.update_jobs_lost, 1);
    assert_eq!(pass.update_jobs_reconciled, 0);
    let lost = job(&app, &server_id, &job4).await;
    assert_eq!(lost["status"], "lost");
    assert!(
        lost["error"]
            .as_str()
            .unwrap()
            .contains("could not be asked"),
        "{lost}"
    );

    // The job had been running all along: the result replayed on reconnect
    // replaces the guess.
    let agent = connect_capable(&mut app, &server_id, script, Some(EMPTY_STATUS), json!({})).await;
    agent.send(mods_result(&job4, "portal")).await;
    let done = job_when(&app, &server_id, &job4, |j| j["status"] == "succeeded").await;
    assert!(done["error"].is_null());
    agent.wait_for_commands("update_ack", 1).await;
    agent.close().await;
}

/// A verdict reached on a stalled job that has since been settled by the
/// agent changes nothing, and so writes no second audit row.
#[tokio::test]
async fn test_settling_an_already_settled_job_changes_nothing() {
    let (mut app, state) = TestApp::new_with_agent_dev_auth_and_state().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Settled box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let service = &state.game_server_updates.service;
    let sid = server_id.parse().unwrap();

    let job_id = start_ok(&app, &server_id, &json!({"operation": "mods"})).await;
    age_job(&app, &job_id).await;
    // The pass reads it as stalled ...
    let stalled = service
        .find_job(sid, job_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    // ... and meanwhile its result arrives.
    agent.send(mods_result(&job_id, "portal")).await;
    job_when(&app, &server_id, &job_id, |j| j["status"] == "succeeded").await;
    let audited = |rows: &[Value]| {
        rows.iter()
            .filter(|r| r["kind"] == "update_result:mods")
            .count()
    };
    // The finish is audited just after the row is stored.
    wait_for(10, || async {
        (audited(&history(&app, &server_id).await) == 1).then_some(())
    })
    .await;

    let mut state_file = mods_result(&job_id, "portal");
    state_file.as_object_mut().unwrap().remove("type");
    let settled = service
        .reconcile_stalled(&stalled, Some(&state_file))
        .await
        .unwrap();
    assert!(
        matches!(
            settled,
            portal_domain::services::game_server::Reconciled::Finished(None)
        ),
        "{settled:?}"
    );
    let settled = service.reconcile_stalled(&stalled, None).await.unwrap();
    assert!(
        matches!(
            settled,
            portal_domain::services::game_server::Reconciled::Lost(None)
        ),
        "{settled:?}"
    );
    assert_eq!(job(&app, &server_id, &job_id).await["status"], "succeeded");

    // And a pass over nothing stalled asks nobody and writes nothing.
    let summary = state.game_server_updates.run_lost_job_pass().await;
    assert_eq!(
        (summary.reconciled, summary.lost, summary.errors),
        (0, 0, 0)
    );
    assert_eq!(audited(&history(&app, &server_id).await), 1);
    assert!(agent.commands("update_status").is_empty());
    agent.close().await;
}

// =============================================================================
// MatchZy overrides
// =============================================================================

#[tokio::test]
async fn test_overrides_validation_rejects_what_the_agent_would() {
    let app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Strict overrides box").await;
    let uri = format!("/v1/admin/game-servers/{server_id}/matchzy-overrides");

    let mut too_many = serde_json::Map::new();
    for i in 0..65 {
        too_many.insert(format!("matchzy_cvar_{i}"), json!(i));
    }
    let long_name = format!("matchzy_{}", "a".repeat(128));
    let long_value = "v".repeat(257);
    let rejected: Vec<(Value, &str)> = vec![
        (
            json!({"matchzy_demo_upload_url": "https://evil.example"}),
            "portal-owned",
        ),
        (
            json!({"matchzy_demo_upload_header_value": "x"}),
            "portal-owned",
        ),
        (
            json!({"matchzy_remote_log_url": "https://evil.example"}),
            "portal-owned",
        ),
        (json!({"matchzy_loadmatch_url": "x"}), "portal-owned"),
        (json!({"matchzy_loadmatch": "x"}), "portal-owned"),
        (json!({"matchzy_loadbackup_url": "x"}), "portal-owned"),
        (json!({"sv_cheats": 1}), "name must match"),
        (json!({"matchzy_": 1}), "name must match"),
        (json!({"matchzy_Upper": 1}), "name must match"),
        (json!({"matchzy_a b": 1}), "name must match"),
        (json!({"matchzy_a;quit": 1}), "name must match"),
        (json!({ long_name.as_str(): 1 }), "name must match"),
        (json!({"matchzy_x": "a\"b"}), "printable ASCII"),
        (json!({"matchzy_x": "a;b"}), "printable ASCII"),
        (json!({"matchzy_x": "a\\b"}), "printable ASCII"),
        (json!({"matchzy_x": "a\nb"}), "printable ASCII"),
        (json!({"matchzy_x": "caf\u{e9}"}), "printable ASCII"),
        (json!({"matchzy_x": "a\u{2028}b"}), "printable ASCII"),
        (json!({"matchzy_x": long_value}), "longer than 256"),
        (json!({"matchzy_x": null}), "string, number or boolean"),
        (json!({"matchzy_x": ["a"]}), "string, number or boolean"),
        (json!({"matchzy_x": {"a": 1}}), "string, number or boolean"),
        (Value::Object(too_many), "too many overrides"),
        // One bad entry rejects the lot.
        (
            json!({"matchzy_good": 1, "matchzy_remote_backup_url": "x"}),
            "portal-owned",
        ),
    ];
    for (overrides, why) in rejected {
        let response = app.put_json(&uri, &json!({ "overrides": overrides })).await;
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "{overrides}: {}",
            response.text()
        );
        let problem: Value = response.json();
        assert!(
            problem["detail"].as_str().unwrap().contains(why),
            "{overrides}: {problem}"
        );
    }
    // Not an object at all.
    let response = app.put_json(&uri, &json!({ "overrides": ["a"] })).await;
    assert!(
        response.status == StatusCode::UNPROCESSABLE_ENTITY
            || response.status == StatusCode::BAD_REQUEST,
        "{}",
        response.status
    );

    // Nothing was stored along the way.
    let response = app.get_auth(&uri).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["overrides"], json!({}));
    assert_eq!(body["data"]["pending"], false);
    assert!(body["data"]["applied"].is_null());

    let response = app
        .put_json(
            &format!(
                "/v1/admin/game-servers/{}/matchzy-overrides",
                uuid::Uuid::now_v7()
            ),
            &json!({"overrides": {}}),
        )
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_overrides_are_pushed_on_put_when_the_agent_is_connected() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Overrides box").await;
    let uri = format!("/v1/admin/game-servers/{server_id}/matchzy-overrides");
    // The second push is answered "written, not started" (a job is running).
    let pushes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&pushes);
    let script: FrameScript = Arc::new(move |frame: &Value| {
        if frame["cmd"] == "set_matchzy_overrides"
            && seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1
        {
            return (
                true,
                json!({"written": 1, "started": false, "job_id": null,
                       "reason": "refused: update job x (mods) is already running"})
                .to_string(),
            );
        }
        update_script()(frame)
    });
    let agent = connect_capable(&mut app, &server_id, script, Some(EMPTY_STATUS), json!({})).await;

    let wanted = json!({
        "matchzy_kick_when_no_match_loaded": false,
        "matchzy_max_tech_pauses": 2,
        "matchzy_chat_prefix": "  [10 Mans]  ",
    });
    let response = app.put_json(&uri, &json!({ "overrides": wanted })).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let stored = json!({
        "matchzy_kick_when_no_match_loaded": false,
        "matchzy_max_tech_pauses": 2,
        "matchzy_chat_prefix": "[10 Mans]",
    });
    assert_eq!(body["data"]["overrides"], stored, "strings are trimmed");
    assert_eq!(body["data"]["delivery"], "applying");
    assert_eq!(body["data"]["job_id"], AGENT_JOB_ID);
    assert!(body["data"]["detail"].is_null());

    let sent = agent.commands("set_matchzy_overrides");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["args"]["overrides"], stored);

    let body: Value = app.get_auth(&uri).await.json();
    assert_eq!(body["data"]["overrides"], stored);
    assert_eq!(body["data"]["pending"], false);

    // The agent's repair-config for it is a job like any other.
    let mut result = mods_result(AGENT_JOB_ID, "overrides");
    result["operation"] = json!("repair-config");
    agent.send(result).await;
    let row = job_when(&app, &server_id, AGENT_JOB_ID, |j| {
        j["status"] == "succeeded"
    })
    .await;
    assert_eq!(row["trigger"], "overrides");
    assert!(row["requested_by"].is_null());

    // Written but not applied now.
    let response = app
        .put_json(&uri, &json!({"overrides": {"matchzy_max_tech_pauses": 3}}))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["delivery"], "written", "{body}");
    assert!(body["data"]["job_id"].is_null());
    assert!(
        body["data"]["detail"]
            .as_str()
            .unwrap()
            .contains("already running"),
        "{body}"
    );
    let body: Value = app.get_auth(&uri).await.json();
    assert_eq!(body["data"]["pending"], false, "the agent has the file");

    // An empty map clears, and the agent is told.
    let response = app.put_json(&uri, &json!({"overrides": {}})).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["overrides"], json!({}));
    assert_eq!(body["data"]["delivery"], "applying");
    let sent = agent.commands("set_matchzy_overrides");
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[2]["args"]["overrides"], json!({}));

    // Audited: names, not values.
    let rows = history(&app, &server_id).await;
    let audit: Vec<&Value> = rows
        .iter()
        .filter(|r| r["kind"] == "matchzy_overrides")
        .collect();
    assert_eq!(audit.len(), 3);
    assert!(
        audit.iter().any(|r| r["command"]
            .as_str()
            .unwrap()
            .contains("matchzy_chat_prefix")
            && !r["command"].as_str().unwrap().contains("10 Mans")),
        "{audit:?}"
    );
    agent.close().await;
}

/// PUTs racing each other: whatever order their pushes run in, the host
/// ends up with what the database holds, and `pending` is false only when
/// it does.
#[tokio::test]
async fn test_concurrent_override_puts_leave_the_host_with_the_stored_set() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Racing overrides box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({"overrides_applied": true}),
    )
    .await;
    let uri = format!("/v1/admin/game-servers/{server_id}/matchzy-overrides");

    for round in 0..3 {
        agent.clear();
        let bodies: Vec<Value> = (0..8)
            .map(|i| json!({"overrides": {"matchzy_max_tech_pauses": round * 10 + i}}))
            .collect();
        let puts = bodies.iter().map(|b| app.put_json(&uri, b));
        for response in futures_util::future::join_all(puts).await {
            response.assert_status(StatusCode::OK);
        }

        let response = app.get_auth(&uri).await;
        response.assert_status(StatusCode::OK);
        let stored: Value = response.json();
        let pushed = agent.commands("set_matchzy_overrides");
        assert!(!pushed.is_empty());
        let last = &pushed.last().unwrap()["args"]["overrides"];
        assert_eq!(stored["data"]["pending"], false, "{stored}");
        assert_eq!(
            last, &stored["data"]["overrides"],
            "round {round}: the host's last set is not the stored one"
        );
    }
    agent.close().await;
}

#[tokio::test]
async fn test_overrides_saved_while_the_agent_is_away_are_pushed_on_connect() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Away box").await;
    let uri = format!("/v1/admin/game-servers/{server_id}/matchzy-overrides");
    let overrides = json!({"matchzy_max_tech_pauses": 2});

    let response = app.put_json(&uri, &json!({ "overrides": overrides })).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["delivery"], "pending");
    assert!(
        body["data"]["detail"]
            .as_str()
            .unwrap()
            .contains("no agent connected"),
        "{body}"
    );
    let body: Value = app.get_auth(&uri).await.json();
    assert_eq!(body["data"]["pending"], true);
    assert_eq!(body["data"]["overrides"], overrides);

    // The agent connects and reports the overrides are not in the file:
    // pushed after that first heartbeat, exactly once.
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        None,
        json!({"overrides_applied": false}),
    )
    .await;
    let sent = agent.wait_for_commands("set_matchzy_overrides", 1).await;
    assert_eq!(sent[0]["args"]["overrides"], overrides);
    wait_for(10, || async {
        let body: Value = app.get_auth(&uri).await.json();
        (body["data"]["pending"] == false).then_some(())
    })
    .await;
    for _ in 0..2 {
        agent
            .heartbeat_with(
                "none",
                None,
                json!({"capabilities": ["lgsm_update"], "overrides_applied": false}),
            )
            .await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert_eq!(
        agent.commands("set_matchzy_overrides").len(),
        1,
        "once per connection: the agent repairs from its own file after that"
    );
    agent.close().await;

    // Reconnect with the overrides in place: nothing to push.
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        None,
        json!({"overrides_applied": true}),
    )
    .await;
    agent
        .heartbeat_with(
            "none",
            None,
            json!({"capabilities": ["lgsm_update"], "overrides_applied": true}),
        )
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(agent.commands("set_matchzy_overrides").is_empty());
    agent.close().await;

    // Reconnect not knowing (`null`): the first heartbeat may just be too
    // early to say, the second means the host never had any — push.
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        None,
        json!({"overrides_applied": null}),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(
        agent.commands("set_matchzy_overrides").is_empty(),
        "not on the first heartbeat"
    );
    agent
        .heartbeat_with(
            "none",
            None,
            json!({"capabilities": ["lgsm_update"], "overrides_applied": null}),
        )
        .await;
    let sent = agent.wait_for_commands("set_matchzy_overrides", 1).await;
    assert_eq!(sent[0]["args"]["overrides"], overrides);
    agent.close().await;
    wait_for(10, || async {
        (overview(&app, &server_id).await["agent_connected"] == false).then_some(())
    })
    .await;

    // Cleared while away: the host still has the old file, so the clear is
    // pushed on connect even though the agent says "applied".
    let response = app.put_json(&uri, &json!({"overrides": {}})).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["delivery"], "pending");
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        None,
        json!({"overrides_applied": true}),
    )
    .await;
    let sent = agent.wait_for_commands("set_matchzy_overrides", 1).await;
    assert_eq!(sent[0]["args"]["overrides"], json!({}));
    wait_for(10, || async {
        let body: Value = app.get_auth(&uri).await.json();
        (body["data"]["pending"] == false).then_some(())
    })
    .await;
    agent.close().await;
}

// =============================================================================
// Permissions
// =============================================================================

#[tokio::test]
async fn test_every_update_endpoint_requires_admin_servers_manage() {
    let mut app = TestApp::new_with_agent_dev_auth().await;
    let game_id = cs2_game_id(&app).await;
    let server_id = create_server(&app, &game_id, "Guarded box").await;
    let agent = connect_capable(
        &mut app,
        &server_id,
        update_script(),
        Some(EMPTY_STATUS),
        json!({}),
    )
    .await;
    let job_id = start_ok(&app, &server_id, &json!({"operation": "lgsm"})).await;
    agent.clear();

    // A registered user with no roles.
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "updates_noperm",
                "email": "updates_noperm@example.com",
                "password": "SecurePass123!",
                "display_name": "updates_noperm"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let token = body["data"]["access_token"].as_str().unwrap().to_string();

    let base = format!("/v1/admin/game-servers/{server_id}");
    let gets = [
        format!("{base}/updates"),
        format!("{base}/updates/{job_id}"),
        format!("{base}/matchzy-overrides"),
    ];
    for uri in &gets {
        let response = app.get_with_token(uri, &token).await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "GET {uri}");
        let response = app.get(uri).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "GET {uri}");
    }
    let posts = [
        (
            format!("{base}/updates"),
            json!({"operation": "mods", "force": true}),
        ),
        (format!("{base}/updates/check"), json!({})),
        (format!("{base}/updates/repair-config"), json!({})),
    ];
    for (uri, body) in &posts {
        let response = app.post_json_with_token(uri, body, &token).await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "POST {uri}");
        let response = app.post_json_no_auth(uri, body).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "POST {uri}");
    }
    let response = app
        .put_json_with_token(
            &format!("{base}/matchzy-overrides"),
            &json!({"overrides": {"matchzy_max_tech_pauses": 9}}),
            &token,
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    // Nothing reached the agent or the database.
    assert!(agent.frames().is_empty(), "{:?}", agent.frames());
    assert_eq!(job_count(&app, &server_id).await, 1);
    let body: Value = app
        .get_auth(&format!("{base}/matchzy-overrides"))
        .await
        .json();
    assert_eq!(body["data"]["overrides"], json!({}));
    agent.close().await;
}

/// The fake agent must refuse what the real one refuses (portal-server-agent
/// `src/run.rs`), or a portal that drifted from the wire contract would
/// pass these tests and break in production.
#[test]
fn test_the_fake_agent_refuses_what_the_real_agent_refuses() {
    let script = update_script();
    let frame = |cmd: &str, args: Value| json!({"id": "c1", "cmd": cmd, "args": args});
    let job = "0193a5b0-7c1e-7000-8000-000000000001";

    let (ok, out) = script(&frame(
        "update_start",
        json!({"operation": "mods", "job_id": job}),
    ));
    assert!(ok, "{out}");
    let out: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        out,
        json!({"started": true, "job_id": job, "operation": "mods"})
    );

    for (args, error) in [
        (
            json!({"operation": "mods", "job_id": job.to_uppercase()}),
            "job_id must be a lowercase UUID",
        ),
        (
            json!({"operation": "mods", "job_id": job.replace('-', "")}),
            "job_id must be a lowercase UUID",
        ),
        (
            json!({"operation": "check", "job_id": job}),
            "unknown operation (expected one of: lgsm, game, mods, all, repair-config)",
        ),
        (
            json!({"operation": "repair_config", "job_id": job}),
            "unknown operation (expected one of: lgsm, game, mods, all, repair-config)",
        ),
        (json!({"job_id": job}), "missing operation"),
        (json!({"operation": "mods"}), "missing job_id"),
        (json!(null), "update_start requires args"),
    ] {
        assert_eq!(
            script(&frame("update_start", args.clone())),
            (false, error.to_string()),
            "{args}"
        );
    }

    let (ok, out) = script(&frame("update_ack", json!({"job_id": job})));
    assert!(ok);
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap(),
        json!({"acked": job})
    );
    assert_eq!(
        script(&frame("update_ack", json!({"job_id": "J"}))),
        (false, "job_id must be a lowercase UUID".to_string())
    );
    assert_eq!(
        script(&frame("set_matchzy_overrides", json!({"matchzy_x": 1}))),
        (false, "missing overrides".to_string())
    );
    assert_eq!(
        script(&frame("set_matchzy_overrides", json!({"overrides": []}))),
        (
            false,
            "overrides must be an object of cvar: value".to_string()
        )
    );
}
