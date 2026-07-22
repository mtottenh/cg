//! Demo catalog routes.

use axum::Router;
use axum::routing::get;

use crate::handlers::demos;
use crate::state::AppState;

/// Create routes for demo endpoints.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Demo listing and search
        .route("/", get(demos::list_demos))
        // Single demo
        .route("/{id}", get(demos::get_demo))
        // Demo players
        .route("/{id}/players", get(demos::get_demo_players))
        // Demo match links
        .route("/{id}/links", get(demos::get_demo_links))
        // Demo download
        .route("/{id}/download", get(demos::get_demo_download))
}
