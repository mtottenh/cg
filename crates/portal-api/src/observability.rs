//! Prometheus instrumentation: RED middleware, subsystem samplers, and
//! point-instrumentation helpers (deploy docs: observability-design.md §4.1).
//!
//! Only the `metrics` facade is used here — the exporter itself (loopback
//! listener, histogram buckets) is installed by the `portal-app` binary
//! from `METRICS_ADDR`. With no recorder installed every call below is a
//! no-op, so the library and its tests pay nothing.
//!
//! Cardinality rules (§3): labels are enums or route TEMPLATES, never IDs,
//! file names, or resolved paths. The one bounded exception is the
//! `server` label on agent heartbeats, capped by the admin-curated
//! `game_servers` registry.

use crate::state::AppState;
use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Interval between sampler passes (pool gauges, WS gauges, readiness).
const SAMPLE_INTERVAL: Duration = Duration::from_secs(15);

/// Current Unix time in seconds, for `*_timestamp_seconds` gauges.
#[must_use]
pub fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// RED metrics for every HTTP request.
///
/// The `route` label is the matched axum route TEMPLATE
/// (`/v1/tournaments/{id}`), never the resolved path. Because the
/// `tower_governor` layers sit inside this middleware, a 429 seen here is a
/// rate-limit rejection and feeds `portal_api_rate_limited_total` (and the
/// `rate-limited` auth-failure reason) as well.
pub async fn track_http(req: Request, next: Next) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_owned(), |p| p.as_str().to_owned());
    let method = req.method().as_str().to_owned();
    let start = Instant::now();

    let resp = next.run(req).await;

    let status = resp.status();
    if status == axum::http::StatusCode::TOO_MANY_REQUESTS {
        metrics::counter!("portal_api_rate_limited_total", "route_class" => route.clone())
            .increment(1);
        record_auth_failure("rate-limited");
    }
    let labels = [
        ("route", route),
        ("method", method),
        ("status", status.as_u16().to_string()),
    ];
    metrics::counter!("portal_api_http_requests_total", &labels).increment(1);
    metrics::histogram!("portal_api_http_request_duration_seconds", &labels[..2])
        .record(start.elapsed().as_secs_f64());
    resp
}

/// Time an S3 call and record `portal_api_s3_ops_total{bucket_role,op,outcome}`
/// plus the per-role duration histogram.
///
/// # Errors
/// Passes the wrapped future's result straight through.
pub async fn track_s3<T, E, F>(bucket_role: &'static str, op: &'static str, fut: F) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    let start = Instant::now();
    let result = fut.await;
    let outcome = if result.is_ok() { "ok" } else { "error" };
    metrics::histogram!("portal_api_s3_op_duration_seconds", "bucket_role" => bucket_role)
        .record(start.elapsed().as_secs_f64());
    metrics::counter!(
        "portal_api_s3_ops_total",
        "bucket_role" => bucket_role,
        "op" => op,
        "outcome" => outcome,
    )
    .increment(1);
    result
}

/// Record one saga execution outcome.
pub fn record_saga<T, E>(saga: &'static str, result: &Result<T, E>) {
    let outcome = if result.is_ok() { "ok" } else { "error" };
    metrics::counter!("portal_api_saga_executions_total", "saga" => saga, "outcome" => outcome)
        .increment(1);
}

/// Record one processed piece of evidence. `kind` is the evidence type
/// enum (screenshot / link / demo …), never user input.
pub fn record_evidence(kind: &str, outcome: &'static str) {
    metrics::counter!(
        "portal_api_evidence_processed_total",
        "type" => kind.to_owned(),
        "outcome" => outcome,
    )
    .increment(1);
}

/// Record an authentication failure by reason
/// (missing / expired / bad-token / bad-api-key / rate-limited / error).
pub fn record_auth_failure(reason: &'static str) {
    metrics::counter!("portal_api_auth_failures_total", "reason" => reason).increment(1);
}

/// Record a WebSocket message on the given channel kind and direction.
pub fn record_ws_message(kind: &'static str, direction: &'static str) {
    metrics::counter!(
        "portal_api_ws_messages_total",
        "kind" => kind,
        "direction" => direction,
    )
    .increment(1);
}

/// Stamp the last-heartbeat gauge for a game server.
///
/// The label set is bounded by the registered-server set; a decommissioned
/// server's series persists until the next API restart, which the
/// staleness alert treats as offline — accurate, if slightly conservative.
pub fn record_agent_heartbeat(server_id: &str) {
    metrics::gauge!(
        "portal_api_gameserver_agent_last_heartbeat_timestamp_seconds",
        "server" => server_id.to_owned(),
    )
    .set(unix_now());
}

/// Spawn the periodic metrics sampler.
///
/// Covers DB pool gauges + the acquire-wait probe, WS connection gauges,
/// the connected-agents gauge, and `portal_api_ready` — the same DB probe
/// the deploy health gate hits, exported continuously.
pub fn spawn_metrics_sampler(
    state: AppState,
    shutdown: Arc<Notify>,
    pool_max: u32,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .ok();
        let mut interval = tokio::time::interval(SAMPLE_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                () = shutdown.notified() => break,
            }
            sample(&state, http.as_ref(), pool_max).await;
        }
    })
}

async fn sample(state: &AppState, http: Option<&reqwest::Client>, pool_max: u32) {
    // DB pool. sqlx exposes no acquire-time hook, so the wait histogram
    // times a real (immediately released) acquire — under saturation this
    // measures the actual queue wait the handlers experience.
    metrics::gauge!("portal_api_db_pool_connections").set(f64::from(state.db_pool.size()));
    #[allow(clippy::cast_precision_loss)]
    metrics::gauge!("portal_api_db_pool_connections_idle").set(state.db_pool.num_idle() as f64);
    metrics::gauge!("portal_api_db_pool_max_connections").set(f64::from(pool_max));

    let start = Instant::now();
    let acquired = state.db_pool.acquire().await;
    metrics::histogram!("portal_api_db_pool_wait_seconds").record(start.elapsed().as_secs_f64());
    let db_ok = acquired.is_ok();
    drop(acquired);

    // WS gauges, sampled from the managers rather than counted at the
    // (many) connect/disconnect sites — a missed decrement can never drift.
    #[allow(clippy::cast_precision_loss)]
    metrics::gauge!("portal_api_ws_connections", "kind" => "lobby")
        .set(state.veto_lobby_manager.total_connections() as f64);
    #[allow(clippy::cast_precision_loss)]
    let agents = state.agent_manager.connected_count() as f64;
    metrics::gauge!("portal_api_ws_connections", "kind" => "gameserver-agent").set(agents);
    metrics::gauge!("portal_api_gameserver_agents_connected").set(agents);

    // Readiness mirrors /health/ready: DB down → not ready; the demo
    // service is optional and reported as its own gauge instead.
    metrics::gauge!("portal_api_ready").set(if db_ok { 1.0 } else { 0.0 });
    if let (Some(client), Some(base)) = (http, state.cs2_demo_base_url.as_deref()) {
        let up = client.get(format!("{base}/health")).send().await.is_ok();
        metrics::gauge!("portal_api_demo_service_up").set(if up { 1.0 } else { 0.0 });
    }
}
