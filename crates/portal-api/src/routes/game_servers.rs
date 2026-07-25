//! Game-server integration routes.
//!
//! `admin_routes` mounts under `/admin/game-servers` (JWT +
//! `admin.servers.manage`). `agent_routes` mounts under `/gameserver` —
//! machine-facing endpoints authenticated by enrollment tokens / client
//! certificates, excluded from the public OpenAPI spec.

use crate::handlers::game_servers::{admin, agent, match_server, matchzy, substitutions};
use crate::state::AppState;
use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::{PeerIpKeyExtractor, SmartIpKeyExtractor};

/// Rate-limit the machine-facing endpoints (§6.2 — includes the
/// unauthenticated `/enroll`). Same env-driven shape as `routes/auth.rs`.
fn agent_rate_limited(routes: Router<AppState>) -> Router<AppState> {
    let burst: u32 = std::env::var("PORTAL_GAMESERVER_RATE_BURST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let per_second: u64 = std::env::var("PORTAL_GAMESERVER_RATE_PER_SECOND")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
        .max(1);
    let trust_forwarded = std::env::var("PORTAL_TRUST_FORWARDED_FOR")
        .is_ok_and(|v| matches!(v.as_str(), "true" | "1" | "yes"));

    if trust_forwarded {
        let config = GovernorConfigBuilder::default()
            .per_second(per_second)
            .burst_size(burst)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("valid governor config");
        routes.layer(GovernorLayer {
            config: Arc::new(config),
        })
    } else {
        let config = GovernorConfigBuilder::default()
            .per_second(per_second)
            .burst_size(burst)
            .key_extractor(PeerIpKeyExtractor)
            .finish()
            .expect("valid governor config");
        routes.layer(GovernorLayer {
            config: Arc::new(config),
        })
    }
}

/// Admin registry routes (mounted at `/admin/game-servers`).
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(admin::list_game_servers).post(admin::create_game_server),
        )
        .route(
            "/{server_id}",
            get(admin::get_game_server)
                .patch(admin::update_game_server)
                .delete(admin::delete_game_server),
        )
        .route(
            "/{server_id}/enrollment-token",
            post(admin::mint_enrollment_token),
        )
        .route("/{server_id}/revoke", post(admin::revoke_agent))
        .route("/{server_id}/command", post(admin::send_command))
        .route(
            "/{server_id}/bookings",
            get(admin::list_bookings).post(admin::create_booking),
        )
        .route(
            "/{server_id}/bookings/{booking_id}",
            axum::routing::delete(admin::delete_booking),
        )
}

/// Agent-facing routes (mounted at `/gameserver`).
pub fn agent_routes() -> Router<AppState> {
    agent_rate_limited(agent_routes_inner())
}

fn agent_routes_inner() -> Router<AppState> {
    Router::new()
        .route("/enroll", post(agent::enroll))
        .route("/renew", post(agent::renew))
        .route("/agent/ws", get(agent::ws_upgrade))
        .route("/match-config/{matchzy_id}", get(matchzy::get_match_config))
        .route("/events", post(matchzy::post_event))
        .route(
            "/backups",
            post(matchzy::post_backup)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route("/backups/{matchzy_id}/{filename}", get(matchzy::get_backup))
        // CS2 demos run 100-300 MB; the global 16 MiB default would reject
        // every upload. Cap at 1 GiB (matched by a Caddy path override).
        .route(
            "/demos",
            post(matchzy::post_demo)
                .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024 * 1024)),
        )
}

/// Match-facing server routes (mounted at `/matches`).
pub fn match_server_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/{match_id}/server",
            get(match_server::get_match_server).delete(match_server::cancel_match_server),
        )
        .route(
            "/{match_id}/server/assign",
            post(match_server::assign_match_server),
        )
        .route(
            "/{match_id}/server/restore",
            post(match_server::restore_match_server),
        )
        .route(
            "/{match_id}/substitutions",
            get(substitutions::list_substitutions).post(substitutions::create_substitution),
        )
        .route(
            "/{match_id}/substitutions/options",
            get(substitutions::substitution_options),
        )
        .route(
            "/{match_id}/substitutions/{substitution_id}",
            axum::routing::delete(substitutions::cancel_substitution),
        )
        .route(
            "/{match_id}/substitutions/{substitution_id}/approve",
            post(substitutions::approve_substitution),
        )
        .route(
            "/{match_id}/substitutions/{substitution_id}/reject",
            post(substitutions::reject_substitution),
        )
}
