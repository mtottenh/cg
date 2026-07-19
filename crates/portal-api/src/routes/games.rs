//! Game routes.

use axum::Router;
use axum::routing::{get, patch, post, put};

use crate::handlers::{awards, games};
use crate::state::AppState;

/// Create routes for game endpoints.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Public endpoints
        .route("/", get(games::list_games))
        .route("/{game_id}", get(games::get_game))
        .route("/{game_id}/maps", get(games::get_maps))
        .route("/{game_id}/rank-tiers", get(games::get_rank_tiers))
        .route("/{game_id}/stat-catalog", get(awards::get_stat_catalog))
        .route(
            "/{game_id}/award-templates",
            get(awards::list_award_templates),
        )
        // Admin endpoints
        .route("/{game_id}", patch(games::update_game))
        .route("/{game_id}/maps", put(games::set_map_pool))
        .route("/{game_id}/enable", post(games::enable_game))
        .route("/{game_id}/disable", post(games::disable_game))
        // Map catalog management (admin)
        .route("/{game_id}/maps/catalog", post(games::add_map))
        .route(
            "/{game_id}/maps/catalog/{map_id}",
            patch(games::update_map).delete(games::remove_map),
        )
        // Rank tiers management (admin)
        .route("/{game_id}/rank-tiers", put(games::set_rank_tiers))
        // Team size management (admin)
        .route("/{game_id}/team-size", patch(games::update_team_size))
}
