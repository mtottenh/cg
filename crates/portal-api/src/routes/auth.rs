//! Auth routes.

use crate::handlers::auth;
use crate::state::AppState;
use axum::routing::post;
use axum::Router;

/// Auth routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(auth::register))
        .route("/login", post(auth::login))
        .route("/refresh", post(auth::refresh))
}
