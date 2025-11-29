//! Game routes.

use axum::routing::{get, patch, post, put};
use axum::Router;

use crate::handlers::games;
use crate::state::AppState;

/// Create routes for game endpoints.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Public endpoints
        .route("/", get(games::list_games))
        .route("/{game_id}", get(games::get_game))
        .route("/{game_id}/maps", get(games::get_maps))
        .route("/{game_id}/rank-tiers", get(games::get_rank_tiers))
        // Admin endpoints
        .route("/{game_id}", patch(games::update_game))
        .route("/{game_id}/maps", put(games::set_map_pool))
        .route("/{game_id}/enable", post(games::enable_game))
        .route("/{game_id}/disable", post(games::disable_game))
}
