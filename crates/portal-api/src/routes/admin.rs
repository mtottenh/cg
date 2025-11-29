//! Admin routes.

use axum::routing::get;
use axum::Router;

use crate::handlers::admin;
use crate::state::AppState;

/// Create routes for admin endpoints.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats", get(admin::get_stats))
}
