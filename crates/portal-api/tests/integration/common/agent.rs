//! A scripted stand-in for portal-server-agent (server-console design §9).
//!
//! Connects over the agent WebSocket using dev-mode auth
//! (`TestApp::new_with_agent_dev_auth`), sends heartbeats on request, and
//! answers every command frame the portal pushes from a script while
//! recording the frames, so a test can assert exactly what reached the
//! "server".

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// Maps a console line (or a non-`exec` frame's `cmd`) to `(ok, output)`.
pub type Script = Arc<dyn Fn(&str) -> (bool, String) + Send + Sync>;

/// Maps a whole command frame (`{id, cmd, args}`) to `(ok, output)` — for
/// the agent 0.3.0 update commands, whose answers depend on `args`.
pub type FrameScript = Arc<dyn Fn(&Value) -> (bool, String) + Send + Sync>;

/// The job id [`update_script`] reports for jobs the "agent" starts itself
/// (`update_check`, the `repair-config` behind `set_matchzy_overrides`).
pub const AGENT_JOB_ID: &str = "0193a5b0-7c1e-7000-8000-00000000a6e7";

/// The operations `update_start` takes (`update::OPERATIONS` in the agent).
const OPERATIONS: [&str; 5] = ["lgsm", "game", "mods", "all", "repair-config"];

/// `^[0-9a-f-]{36}$` — the agent's `valid_job_id`, which it applies to
/// the job id of `update_start` and `update_ack`.
fn valid_job_id(id: &str) -> bool {
    id.len() == 36
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
}

/// A frame script that answers the agent 0.3.0 update commands the way
/// the real agent does (portal-server-agent `src/run.rs`
/// `execute_update_command` and `Updater::start`), refusals included: a
/// frame the real agent would reject is rejected here with the agent's
/// own error string, so a portal that drifts from the wire contract fails
/// these tests instead of production. Everything else gets a canned `ok`.
pub fn update_script() -> FrameScript {
    Arc::new(|frame: &Value| {
        let cmd = frame["cmd"].as_str().unwrap_or_default();
        let args = frame.get("args").filter(|a| !a.is_null());
        let refuse = |e: &str| (false, e.to_string());
        match cmd {
            "update_check" => (
                true,
                json!({"started": true, "job_id": AGENT_JOB_ID, "operation": "check"}).to_string(),
            ),
            "update_start" => {
                let Some(args) = args else {
                    return refuse("update_start requires args");
                };
                let Some(operation) = args["operation"].as_str() else {
                    return refuse("missing operation");
                };
                let Some(job_id) = args["job_id"].as_str() else {
                    return refuse("missing job_id");
                };
                if !OPERATIONS.contains(&operation) {
                    return refuse(&format!(
                        "unknown operation (expected one of: {})",
                        OPERATIONS.join(", ")
                    ));
                }
                if !valid_job_id(job_id) {
                    return refuse("job_id must be a lowercase UUID");
                }
                (
                    true,
                    json!({"started": true, "job_id": job_id, "operation": operation}).to_string(),
                )
            }
            "update_status" => (true, "null".to_string()),
            "update_ack" => {
                let Some(args) = args else {
                    return refuse("update_ack requires args");
                };
                let Some(job_id) = args["job_id"].as_str() else {
                    return refuse("missing job_id");
                };
                if !valid_job_id(job_id) {
                    return refuse("job_id must be a lowercase UUID");
                }
                (true, json!({"acked": job_id}).to_string())
            }
            "set_matchzy_overrides" => {
                let Some(args) = args else {
                    return refuse("set_matchzy_overrides requires args");
                };
                let Some(overrides) = args.get("overrides") else {
                    return refuse("missing overrides");
                };
                let Some(map) = overrides.as_object() else {
                    return refuse("overrides must be an object of cvar: value");
                };
                (
                    true,
                    json!({"written": map.len(), "started": true, "job_id": AGENT_JOB_ID,
                           "operation": "repair-config"})
                    .to_string(),
                )
            }
            other => (true, format!("ran: {other}")),
        }
    })
}

pub struct FakeAgent {
    outbox: mpsc::Sender<String>,
    received: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl FakeAgent {
    /// Connect as `server_id` and start answering frames from `script`.
    pub async fn connect(addr: SocketAddr, server_id: &str, script: Script) -> Self {
        let by_frame: FrameScript = Arc::new(move |frame: &Value| {
            let cmd = frame["cmd"].as_str().unwrap_or_default();
            let line = if cmd == "exec" {
                frame["args"]["command"].as_str().unwrap_or_default()
            } else {
                cmd
            };
            script(line)
        });
        Self::connect_with(addr, server_id, by_frame).await
    }

    /// Connect as `server_id` and answer every command frame from `script`,
    /// which sees the whole frame.
    pub async fn connect_with(addr: SocketAddr, server_id: &str, script: FrameScript) -> Self {
        let mut request = format!("ws://{addr}/v1/gameserver/agent/ws")
            .into_client_request()
            .expect("agent ws url");
        request
            .headers_mut()
            .insert("x-dev-server-id", server_id.parse().expect("header value"));
        let (mut ws, _) = connect_async(request)
            .await
            .expect("agent websocket connects (dev auth on?)");

        let received = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&received);
        let (outbox, mut outbox_rx) = mpsc::channel::<String>(16);
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    frame = outbox_rx.recv() => {
                        let Some(text) = frame else {
                            // The handle was dropped: say goodbye and stop.
                            let _ = ws.send(Message::Close(None)).await;
                            break;
                        };
                        if ws.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    },
                    msg = ws.next() => match msg {
                        Some(Ok(Message::Text(text))) => {
                            let frame: Value =
                                serde_json::from_str(&text).expect("portal frame is JSON");
                            seen.lock().unwrap().push(frame.clone());
                            let (ok, output) = script(&frame);
                            let reply = if ok {
                                json!({ "id": frame["id"], "ok": true, "output": output })
                            } else {
                                json!({ "id": frame["id"], "ok": false, "error": output })
                            };
                            if ws.send(Message::Text(reply.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                        Some(Ok(_)) => {}
                    },
                }
            }
        });

        Self {
            outbox,
            received,
            task,
        }
    }

    /// Send one heartbeat: `gamestate` as MatchZy reports it (`none`,
    /// `warmup`, `live`, ...) and, for an agent 0.2.0 shape, CS2's `status`.
    pub async fn heartbeat(&self, gamestate: &str, status_output: Option<&str>) {
        let mut frame = json!({
            "type": "heartbeat",
            "agent_version": "0.2.0-test",
            "rcon_ok": true,
            "get5_status": { "gamestate": gamestate },
        });
        if let Some(output) = status_output {
            frame["status_output"] = json!(output);
        }
        self.outbox
            .send(frame.to_string())
            .await
            .expect("agent task alive");
    }

    /// Send one agent 0.3.0 heartbeat: the 0.2.0 shape plus whatever
    /// update fields `extra` carries (`capabilities`, `demo_upload_wired`,
    /// `overrides_applied`, `reload_pending`, `update_job`).
    pub async fn heartbeat_with(&self, gamestate: &str, status_output: Option<&str>, extra: Value) {
        let mut frame = json!({
            "type": "heartbeat",
            "agent_version": "0.3.0-test",
            "rcon_ok": true,
            "get5_status": { "gamestate": gamestate },
        });
        if let Some(output) = status_output {
            frame["status_output"] = json!(output);
        }
        if let Some(extra) = extra.as_object() {
            for (k, v) in extra {
                frame[k.as_str()] = v.clone();
            }
        }
        self.send(frame).await;
    }

    /// Push one frame to the portal as-is (`update_progress`,
    /// `update_result`, `update_state`, ...).
    pub async fn send(&self, frame: Value) {
        self.outbox
            .send(frame.to_string())
            .await
            .expect("agent task alive");
    }

    /// The command frames the portal sent with this `cmd`, in order.
    pub fn commands(&self, cmd: &str) -> Vec<Value> {
        self.frames()
            .into_iter()
            .filter(|f| f["cmd"] == cmd)
            .collect()
    }

    /// Wait (up to 10 s) until the portal has sent at least `n` frames
    /// with this `cmd`, and return them.
    pub async fn wait_for_commands(&self, cmd: &str, n: usize) -> Vec<Value> {
        wait_for(10, || async {
            let got = self.commands(cmd);
            (got.len() >= n).then_some(got)
        })
        .await
    }

    /// Every frame the portal sent, in order.
    pub fn frames(&self) -> Vec<Value> {
        self.received.lock().unwrap().clone()
    }

    /// The console lines sent through `exec`, plus the bare `cmd` of any
    /// other frame (`end_match`, `status`, ...), in order.
    pub fn lines(&self) -> Vec<String> {
        self.frames()
            .iter()
            .map(|f| {
                if f["cmd"] == "exec" {
                    f["args"]["command"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string()
                } else {
                    f["cmd"].as_str().unwrap_or_default().to_string()
                }
            })
            .collect()
    }

    /// Forget the frames recorded so far.
    pub fn clear(&self) {
        self.received.lock().unwrap().clear();
    }

    /// Close the socket and wait for the task to finish.
    pub async fn close(self) {
        drop(self.outbox);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }
}

/// Poll `probe` every 100 ms until it returns `Some`, for up to `secs`.
pub async fn wait_for<T, F, Fut>(secs: u64, mut probe: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(v) = probe().await {
            return v;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "condition not met within {secs}s"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
