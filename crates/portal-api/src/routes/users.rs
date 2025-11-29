//! User routes.

use crate::handlers::{leagues, users};
use crate::state::AppState;
use axum::routing::get;
use axum::Router;

/// Create user routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(users::get_current_user))
        .route("/me/leagues", get(leagues::get_my_leagues))
        .route("/me/league-invitations", get(leagues::get_my_invitations))
}
