//! WebSocket routes.

use axum::{Router, routing::get};

use crate::handlers::veto_ws;
use crate::state::AppState;

/// Create WebSocket routes.
///
/// These routes handle WebSocket upgrades for real-time features.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Veto lobby WebSocket endpoint
        .route("/veto/{match_id}", get(veto_ws::ws_upgrade))
}
