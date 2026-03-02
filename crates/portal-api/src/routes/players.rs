//! Player routes.

use crate::handlers::{player_game_profiles, players, uploads};
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
        .route("/me/games", get(player_game_profiles::get_my_game_profiles))
        .route("/{player_id}", get(players::get_player))
        .route("/{player_id}/games", get(player_game_profiles::list_player_game_profiles))
        .route("/{player_id}/games/{game_id}", get(player_game_profiles::get_player_game_profile))
}
