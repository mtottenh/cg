//! Loopback Prometheus exporter install (deploy docs:
//! observability-design.md §3/§4.1).
//!
//! Enabled by `METRICS_ADDR` (e.g. `127.0.0.1:9464`); unset or empty means
//! no listener and every `metrics::` call in the workspace stays a no-op.
//! The listener must never sit on the public service port or go through
//! Caddy.

use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};
use std::net::SocketAddr;
use tracing::info;

/// Default web buckets for HTTP request durations.
const HTTP_DURATION_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// S3 round-trips (uploads of 100+ MB demos can take a while).
const S3_DURATION_BUCKETS: &[f64] = &[0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0];

/// Pool acquire waits: sub-millisecond when healthy, up to the 30s
/// acquire timeout when saturated.
const POOL_WAIT_BUCKETS: &[f64] = &[0.001, 0.005, 0.025, 0.1, 0.5, 1.0, 5.0, 30.0];

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
            Matcher::Full("portal_api_http_request_duration_seconds".into()),
            HTTP_DURATION_BUCKETS,
        )
        .expect("bucket config")
        .set_buckets_for_metric(
            Matcher::Full("portal_api_s3_op_duration_seconds".into()),
            S3_DURATION_BUCKETS,
        )
        .expect("bucket config")
        .set_buckets_for_metric(
            Matcher::Full("portal_api_db_pool_wait_seconds".into()),
            POOL_WAIT_BUCKETS,
        )
        .expect("bucket config")
        .install()
        .unwrap_or_else(|e| panic!("failed to start metrics exporter on {addr}: {e}"));

    metrics::gauge!("portal_api_build_info", "version" => env!("CARGO_PKG_VERSION")).set(1.0);
    info!("metrics exporter listening on http://{addr}/metrics");
    true
}

/// Export migration state after the boot-time `sqlx::migrate!` run:
/// the total applied count (absolute counter) and a success timestamp.
pub async fn record_migrations(pool: &sqlx::PgPool) {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
    {
        Ok(applied) => {
            metrics::counter!("portal_api_migrations_applied_total")
                .absolute(applied.try_into().unwrap_or(0));
            metrics::gauge!("portal_api_migrations_last_success_timestamp_seconds")
                .set(portal_api::observability::unix_now());
        }
        Err(e) => tracing::warn!(error = %e, "could not count applied migrations for metrics"),
    }
}
