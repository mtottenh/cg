//! WebSocket routes.

use axum::{Router, routing::get};

use crate::handlers::{pug_ws, veto_ws};
use crate::state::AppState;

/// Create WebSocket routes.
///
/// These routes handle WebSocket upgrades for real-time features.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Veto lobby WebSocket endpoint
        .route("/veto/{match_id}", get(veto_ws::ws_upgrade))
        // PUG lobby WebSocket endpoint (doorbell frames; auth in-band)
        .route("/pug/{pug_id}", get(pug_ws::ws_upgrade))
}
