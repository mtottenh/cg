//! Admin routes.

use axum::routing::{get, post};
use axum::Router;

use crate::handlers::{admin, bans};
use crate::state::AppState;

/// Create routes for admin endpoints.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats", get(admin::get_stats))
        // Ban routes
        .route("/bans", get(bans::list_bans).post(bans::create_ban))
        .route("/bans/{id}", get(bans::get_ban))
        .route("/bans/{id}/lift", post(bans::lift_ban))
        .route("/users/{user_id}/bans", get(bans::get_user_bans))
}
