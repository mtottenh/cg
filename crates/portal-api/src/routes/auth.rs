//! Auth routes.
//!
//! `/auth/*` is the one surface of the API where unauthenticated callers can
//! reach a CPU-heavy code path (Argon2id) and the account store. Without a
//! rate limit, a tiny botnet can both brute-force credentials and stall the
//! runtime (C4 offloads Argon2 to `spawn_blocking`, which keeps the request
//! path non-blocking, but does not bound per-IP throughput). This module
//! wraps the auth endpoints with a per-IP token-bucket via `tower_governor`.
//!
//! # Client IP
//!
//! The limiter extracts the client address from the TCP peer by default.
//! When the server runs behind a reverse proxy (nginx, Envoy, a CDN), that
//! becomes the proxy's IP — every request looks like it comes from the same
//! origin and the limit locks out all users together. In that topology,
//! deploy with an explicit forwarded-IP extractor and only trust the
//! forwarded chain from known proxies. For now the peer-IP default is
//! honest — if you see "everyone on one IP" in the metrics, that's the
//! signal to flip the extractor rather than a silent mis-attribution.
//!
//! # Tuning
//!
//! Defaults: 20 req/s burst, 5 req/s sustained. Generous enough that
//! integration tests (which hammer login/register) don't trip the limiter,
//! tight enough that a single IP can't spam a millions-long credential
//! dictionary. Override via:
//!
//! * `PORTAL_AUTH_RATE_BURST` — burst size (requests)
//! * `PORTAL_AUTH_RATE_PER_SECOND` — sustained rate (requests per second)

use crate::handlers::auth;
use crate::state::AppState;
use axum::routing::post;
use axum::Router;
use std::sync::Arc;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;

const DEFAULT_BURST: u32 = 20;
const DEFAULT_PER_SECOND: u64 = 5;

/// Auth routes.
pub fn routes() -> Router<AppState> {
    let burst = std::env::var("PORTAL_AUTH_RATE_BURST")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_BURST);
    let per_second = std::env::var("PORTAL_AUTH_RATE_PER_SECOND")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_PER_SECOND)
        // per_second = 0 would deadlock the bucket; clamp up to 1.
        .max(1);

    let config = GovernorConfigBuilder::default()
        .per_second(per_second)
        .burst_size(burst)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .expect("valid governor config: burst>=1, per_second>=1");

    let governor = GovernorLayer {
        config: Arc::new(config),
    };

    Router::new()
        .route("/register", post(auth::register))
        .route("/login", post(auth::login))
        .route("/refresh", post(auth::refresh))
        .layer(governor)
}
