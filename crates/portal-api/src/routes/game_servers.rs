//! Game-server integration routes.
//!
//! `admin_routes` mounts under `/admin/game-servers` (JWT +
//! `admin.servers.manage`). `agent_routes` mounts under `/gameserver` —
//! machine-facing endpoints authenticated by enrollment tokens / client
//! certificates, excluded from the public OpenAPI spec.

use crate::handlers::game_servers::{admin, agent};
use crate::state::AppState;
use axum::Router;
use axum::routing::{get, post};

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
    Router::new()
        .route("/enroll", post(agent::enroll))
        .route("/agent/ws", get(agent::ws_upgrade))
}
