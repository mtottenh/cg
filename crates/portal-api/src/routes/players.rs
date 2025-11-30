//! Player routes.

use crate::handlers::{players, uploads};
use crate::state::AppState;
use axum::routing::{get, post};
use axum::Router;

/// Create player routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(players::search_players))
        // /me routes must come before /{player_id} to avoid being captured
        .route("/me", get(players::get_my_profile).patch(players::update_my_profile))
        .route("/me/avatar", post(uploads::upload_player_avatar))
        .route("/me/banner", post(uploads::upload_player_banner))
        .route("/{player_id}", get(players::get_player))
        // TODO: Add player league team memberships route
}
