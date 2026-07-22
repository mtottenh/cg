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
//! The limiter keys on the **TCP peer address** by default
//! ([`PeerIpKeyExtractor`]). Forwarded headers (`X-Forwarded-For`,
//! `X-Real-IP`, `Forwarded`) are client-controlled: if the API can be
//! reached directly, an attacker rotates the header value and gets a fresh
//! token bucket per request, which defeats the brute-force limit entirely.
//!
//! Behind a reverse proxy that *overwrites* the forwarded chain (our Caddy
//! deployment sets `header_up X-Forwarded-For {remote}`), the peer address
//! is the proxy and every user shares one bucket — there you want the
//! forwarded-header extractor. Opt in explicitly with:
//!
//! * `PORTAL_TRUST_FORWARDED_FOR=true` — key on X-Forwarded-For / X-Real-IP
//!   ([`SmartIpKeyExtractor`]). Only set this when a trusted proxy is the
//!   *only* path to the listener (bind loopback and/or firewall port 3000).
//!
//! Default (unset or anything other than `true`/`1`): peer IP.
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

use crate::handlers::{auth, steam_auth};
use crate::state::AppState;
use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::{PeerIpKeyExtractor, SmartIpKeyExtractor};

const DEFAULT_BURST: u32 = 20;
const DEFAULT_PER_SECOND: u64 = 5;

/// Whether to key the rate limiter on forwarded headers instead of the TCP
/// peer address. Off unless explicitly enabled — see the module docs.
fn trust_forwarded_for() -> bool {
    std::env::var("PORTAL_TRUST_FORWARDED_FOR")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "true" || v == "1" || v == "yes"
        })
        .unwrap_or(false)
}

/// The rate-limited auth endpoints, without the limiter layer.
fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(auth::register))
        .route("/login", post(auth::login))
        .route("/refresh", post(auth::refresh))
        .route("/logout", post(auth::logout))
        .route("/logout-all", post(auth::logout_all))
        // Steam OpenID sign-in shares the same per-IP limiter — the
        // callback reaches the account store just like login does.
        .route("/steam/login", get(steam_auth::steam_login))
        .route("/steam/callback", get(steam_auth::steam_callback))
}

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

    // The two extractors are distinct types, so the governor layer has to be
    // built (and applied) inside each arm rather than swapped in as a value.
    if trust_forwarded_for() {
        tracing::warn!(
            "PORTAL_TRUST_FORWARDED_FOR is set — auth rate limiting keys on X-Forwarded-For/X-Real-IP. \
             Only safe when a trusted proxy that overwrites those headers is the sole path to this listener."
        );
        let config = GovernorConfigBuilder::default()
            .per_second(per_second)
            .burst_size(burst)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("valid governor config: burst>=1, per_second>=1");
        auth_routes().layer(GovernorLayer {
            config: Arc::new(config),
        })
    } else {
        let config = GovernorConfigBuilder::default()
            .per_second(per_second)
            .burst_size(burst)
            .key_extractor(PeerIpKeyExtractor)
            .finish()
            .expect("valid governor config: burst>=1, per_second>=1");
        auth_routes().layer(GovernorLayer {
            config: Arc::new(config),
        })
    }
}
