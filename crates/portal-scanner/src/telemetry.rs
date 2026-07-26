//! Loopback Prometheus exporter (deploy docs: observability-design.md §4.2).
//!
//! Enabled by `METRICS_ADDR` (e.g. `127.0.0.1:9465`); unset or empty means
//! no listener and every `metrics::` call is a no-op. Never public, never
//! through Caddy.

use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

/// Scan cycles list thousands of S3 objects — seconds, not milliseconds.
const SCAN_DURATION_BUCKETS: &[f64] = &[0.1, 0.5, 1.0, 5.0, 15.0, 30.0, 60.0, 120.0, 300.0];

/// Install the Prometheus recorder + loopback listener when `METRICS_ADDR`
/// is set. Returns whether metrics are enabled.
///
/// # Panics
/// Panics on an unparseable address or failed bind — a misconfigured
/// exporter should fail loudly at startup, not silently monitor nothing.
pub fn install_from_env() -> bool {
    let Some(addr) = std::env::var("METRICS_ADDR")
        .ok()
        .filter(|v| !v.trim().is_empty())
    else {
        info!("METRICS_ADDR not set — metrics exporter disabled");
        return false;
    };
    let addr: SocketAddr = addr
        .parse()
        .unwrap_or_else(|e| panic!("invalid METRICS_ADDR {addr:?}: {e}"));

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .set_buckets_for_metric(
            Matcher::Full("portal_scanner_scan_duration_seconds".into()),
            SCAN_DURATION_BUCKETS,
        )
        .expect("bucket config")
        .install()
        .unwrap_or_else(|e| panic!("failed to start metrics exporter on {addr}: {e}"));

    metrics::gauge!("portal_scanner_build_info", "version" => env!("CARGO_PKG_VERSION")).set(1.0);
    info!("metrics exporter listening on http://{addr}/metrics");
    true
}

/// Current Unix time in seconds, for `*_last_success_timestamp_seconds`.
#[must_use]
pub fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
