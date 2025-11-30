//! Tournament routes.

use crate::handlers::tournaments;
use crate::state::AppState;
use axum::routing::{get, post, patch};
use axum::Router;

/// Tournament routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Tournament CRUD
        .route("/", post(tournaments::create_tournament))
        .route("/", get(tournaments::list_tournaments))
        .route("/{tournament_id}", get(tournaments::get_tournament))
        .route("/{tournament_id}", patch(tournaments::update_tournament))
        .route("/by-slug/{slug}", get(tournaments::get_tournament_by_slug))
        // Tournament lifecycle
        .route("/{tournament_id}/publish", post(tournaments::publish_tournament))
        .route("/{tournament_id}/open-registration", post(tournaments::open_registration))
        .route("/{tournament_id}/start", post(tournaments::start_tournament))
        // Tournament stages
        .route("/{tournament_id}/stages", post(tournaments::create_stage))
        .route("/{tournament_id}/stages", get(tournaments::get_stages))
        // Tournament registrations
        .route("/{tournament_id}/registrations", get(tournaments::get_registrations))
        .route("/{tournament_id}/registrations/team", post(tournaments::register_team))
        .route("/{tournament_id}/registrations/player", post(tournaments::register_player))
        .route(
            "/{tournament_id}/registrations/{registration_id}/check-in",
            post(tournaments::check_in),
        )
        // Tournament brackets and matches
        .route("/{tournament_id}/brackets", get(tournaments::get_brackets))
        .route("/{tournament_id}/matches", get(tournaments::get_matches))
}
