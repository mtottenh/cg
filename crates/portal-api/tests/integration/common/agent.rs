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

pub struct FakeAgent {
    outbox: mpsc::Sender<String>,
    received: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl FakeAgent {
    /// Connect as `server_id` and start answering frames from `script`.
    pub async fn connect(addr: SocketAddr, server_id: &str, script: Script) -> Self {
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
                            let cmd = frame["cmd"].as_str().unwrap_or_default().to_string();
                            let line = if cmd == "exec" {
                                frame["args"]["command"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_string()
                            } else {
                                cmd
                            };
                            let (ok, output) = script(&line);
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
