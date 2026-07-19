//! Tournament routes.

use crate::handlers::{availability, awards, dispute, forfeit, tournaments};
use crate::state::AppState;
use axum::Router;
use axum::routing::{delete, get, patch, post};

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
        .route(
            "/{tournament_id}/publish",
            post(tournaments::publish_tournament),
        )
        .route(
            "/{tournament_id}/open-registration",
            post(tournaments::open_registration),
        )
        .route(
            "/{tournament_id}/start",
            post(tournaments::start_tournament),
        )
        .route(
            "/{tournament_id}/close-registration",
            post(tournaments::close_registration),
        )
        .route(
            "/{tournament_id}/reopen-registration",
            post(tournaments::reopen_registration),
        )
        .route(
            "/{tournament_id}/cancel",
            post(tournaments::cancel_tournament),
        )
        .route(
            "/{tournament_id}/complete",
            post(tournaments::complete_tournament),
        )
        .route(
            "/{tournament_id}/finalize",
            post(tournaments::finalize_tournament),
        )
        // Tournament stages
        .route("/{tournament_id}/stages", post(tournaments::create_stage))
        .route("/{tournament_id}/stages", get(tournaments::get_stages))
        // Tournament registrations
        .route(
            "/{tournament_id}/registrations",
            get(tournaments::get_registrations),
        )
        .route(
            "/{tournament_id}/registrations/team",
            post(tournaments::register_team),
        )
        .route(
            "/{tournament_id}/registrations/player",
            post(tournaments::register_player),
        )
        .route(
            "/{tournament_id}/registrations/{registration_id}/check-in",
            post(tournaments::check_in),
        )
        // Registration management
        .route(
            "/{tournament_id}/registrations/{registration_id}",
            delete(tournaments::withdraw),
        )
        .route(
            "/{tournament_id}/registrations/{registration_id}/approve",
            post(tournaments::approve_registration),
        )
        .route(
            "/{tournament_id}/registrations/{registration_id}/reject",
            post(tournaments::reject_registration),
        )
        .route(
            "/{tournament_id}/registrations/{registration_id}/disqualify",
            post(tournaments::disqualify),
        )
        .route(
            "/{tournament_id}/registrations/{registration_id}/admin-check-in",
            post(tournaments::admin_check_in),
        )
        // Check-in status
        .route(
            "/{tournament_id}/check-in-status",
            get(tournaments::get_check_in_status),
        )
        .route(
            "/{tournament_id}/process-no-shows",
            post(tournaments::process_no_shows),
        )
        // Seeding
        .route("/{tournament_id}/seeding", get(tournaments::get_seeding))
        .route(
            "/{tournament_id}/seeding",
            delete(tournaments::clear_seeding),
        )
        .route(
            "/{tournament_id}/seeding/auto",
            post(tournaments::auto_seed),
        )
        .route(
            "/{tournament_id}/seeding/manual",
            post(tournaments::manual_seed),
        )
        // Tournament brackets, matches, and standings
        .route("/{tournament_id}/brackets", get(tournaments::get_brackets))
        .route(
            "/{tournament_id}/brackets/{bracket_id}/standings",
            get(tournaments::get_bracket_standings),
        )
        .route("/{tournament_id}/matches", get(tournaments::get_matches))
        .route(
            "/{tournament_id}/matches/{match_id}",
            get(tournaments::get_match),
        )
        // Match lifecycle
        .route(
            "/{tournament_id}/matches/{match_id}/status",
            get(tournaments::get_match_status),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/status-history",
            get(tournaments::get_match_status_history),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/check-in",
            post(tournaments::match_check_in),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/schedule",
            post(tournaments::schedule_match),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/forfeit",
            post(tournaments::forfeit_match),
        )
        // Match scheduling (proposal workflow)
        .route(
            "/{tournament_id}/matches/{match_id}/schedule/propose",
            post(tournaments::propose_schedule),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/schedule/accept",
            post(tournaments::accept_schedule_proposal),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/schedule/reject",
            post(tournaments::reject_schedule_proposal),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/schedule/counter",
            post(tournaments::counter_propose),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/schedule/active",
            get(tournaments::get_active_proposal),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/schedule/history",
            get(tournaments::get_proposal_history),
        )
        // Match time suggestions (availability-based)
        .route(
            "/{tournament_id}/matches/{match_id}/suggestions",
            get(availability::get_suggestions),
        )
        .route(
            "/{tournament_id}/matches/{match_id}/suggestions/generate",
            post(availability::generate_suggestions),
        )
        // Forfeit routes (participant)
        .route(
            "/{tournament_id}/registrations/{registration_id}/withdraw",
            post(forfeit::withdraw_from_tournament),
        )
        // Dispute routes (participant)
        .route(
            "/{tournament_id}/matches/{match_id}/dispute",
            get(dispute::get_match_dispute).post(dispute::raise_dispute),
        )
        // Map pool routes
        .route(
            "/{tournament_id}/map-pool",
            get(tournaments::get_tournament_map_pool)
                .put(tournaments::set_tournament_map_pool)
                .delete(tournaments::delete_tournament_map_pool),
        )
        // Awards + leaderboards
        .route(
            "/{tournament_id}/awards",
            get(awards::list_tournament_awards).post(awards::create_tournament_award),
        )
        .route(
            "/{tournament_id}/awards/{award_id}",
            patch(awards::update_tournament_award).delete(awards::void_tournament_award),
        )
        .route(
            "/{tournament_id}/awards/{award_id}/standings",
            get(awards::get_tournament_award_standings),
        )
        .route(
            "/{tournament_id}/awards/{award_id}/finalize",
            post(awards::finalize_tournament_award),
        )
        .route(
            "/{tournament_id}/leaderboards",
            get(awards::get_tournament_leaderboard),
        )
}
