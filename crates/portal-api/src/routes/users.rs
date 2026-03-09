//! User routes.

use crate::handlers::{leagues, users};
use crate::state::AppState;
use axum::routing::get;
use axum::Router;

/// Create user routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(users::get_current_user))
        .route("/me/roles", get(users::get_my_roles))
        .route("/me/leagues", get(leagues::get_my_leagues))
        .route("/me/league-invitations", get(leagues::get_my_invitations))
        .route("/me/matches", get(users::get_my_matches))
        .route("/me/action-items", get(users::get_my_action_items))
}
