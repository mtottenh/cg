//! Availability routes.

use crate::handlers::availability;
use crate::state::AppState;
use axum::Router;
use axum::routing::{delete, get, post};

/// Create availability routes for current player (/players/me/availability).
pub fn player_availability_routes() -> Router<AppState> {
    Router::new()
        // Availability windows
        .route(
            "/windows",
            get(availability::get_player_windows).post(availability::create_player_window),
        )
        .route(
            "/windows/{window_id}",
            delete(availability::delete_player_window).patch(availability::update_player_window),
        )
        // Availability overrides
        .route(
            "/overrides",
            get(availability::get_player_overrides).post(availability::create_player_override),
        )
        .route(
            "/overrides/{override_id}",
            delete(availability::delete_player_override),
        )
        // Date availability
        .route("/date", get(availability::get_player_date_availability))
}

/// Create public availability routes for a specific player (/players/{player_id}/availability).
pub fn player_public_availability_routes() -> Router<AppState> {
    Router::new().route(
        "/date",
        get(availability::get_player_date_availability_public),
    )
}

/// Create match suggestion routes (/tournaments/{tournament_id}/matches/{match_id}/suggestions).
pub fn match_suggestion_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(availability::get_suggestions))
        .route("/generate", post(availability::generate_suggestions))
}
