//! Dispute routes.

use axum::routing::{get, post};
use axum::Router;

use crate::handlers::dispute;
use crate::state::AppState;

/// Dispute routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{dispute_id}", get(dispute::get_dispute))
        .route("/{dispute_id}/messages", post(dispute::add_dispute_message))
}
