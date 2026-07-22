//! League team routes.
//!
//! Route structure:
//! - `/league-seasons` - Season CRUD, team creation/listing
//! - `/league-teams` - Persistent team identity operations
//! - `/league-team-seasons` - Seasonal roster management
//! - `/league-team-invitations` - Invitation management

use crate::handlers::{awards, league_teams, uploads};
use crate::state::AppState;
use axum::Router;
use axum::routing::{delete, get, patch, post};

/// League season routes (nested under /league-seasons).
pub fn season_routes() -> Router<AppState> {
    Router::new()
        // Season CRUD
        .route("/", post(league_teams::create_season))
        .route("/", get(league_teams::list_seasons))
        .route("/{season_id}", get(league_teams::get_season))
        .route("/{season_id}", patch(league_teams::update_season))
        // Teams in a season (list team seasons, create new team)
        .route(
            "/{season_id}/teams",
            get(league_teams::list_teams_in_season),
        )
        .route("/{season_id}/teams", post(league_teams::create_team))
        .route(
            "/{season_id}/teams/register",
            post(league_teams::register_team_for_season),
        )
        // Awards + leaderboards
        .route(
            "/{season_id}/awards",
            get(awards::list_season_awards).post(awards::create_season_award),
        )
        .route(
            "/{season_id}/awards/{award_id}",
            patch(awards::update_season_award).delete(awards::void_season_award),
        )
        .route(
            "/{season_id}/awards/{award_id}/standings",
            get(awards::get_season_award_standings),
        )
        .route(
            "/{season_id}/awards/{award_id}/finalize",
            post(awards::finalize_season_award),
        )
        .route(
            "/{season_id}/leaderboards",
            get(awards::get_season_leaderboard),
        )
        .route(
            "/{season_id}/stats-leaderboard",
            get(awards::get_season_player_stats),
        )
}

/// League team routes (nested under /league-teams).
/// These operate on the persistent team identity.
pub fn team_routes() -> Router<AppState> {
    Router::new()
        // Team CRUD (persistent identity)
        .route("/{team_id}", get(league_teams::get_team))
        .route("/{team_id}", patch(league_teams::update_team))
        .route("/{team_id}", delete(league_teams::disband_team))
        .route(
            "/{team_id}/transfer-ownership",
            post(league_teams::transfer_ownership),
        )
        // Image uploads (team settings manage permission — i.e. owner/captain/admin)
        .route("/{team_id}/logo", post(uploads::upload_team_logo))
        .route("/{team_id}/banner", post(uploads::upload_team_banner))
}

/// League team season routes (nested under /league-team-seasons).
/// These operate on seasonal rosters.
pub fn team_season_routes() -> Router<AppState> {
    Router::new()
        // Team season info
        .route("/{team_season_id}", get(league_teams::get_team_season))
        // Roster management
        .route(
            "/{team_season_id}/members",
            get(league_teams::get_team_season_members),
        )
        .route(
            "/{team_season_id}/members",
            post(league_teams::add_team_member),
        )
        .route(
            "/{team_season_id}/members/{player_id}",
            delete(league_teams::remove_team_member),
        )
        .route(
            "/{team_season_id}/members/{player_id}/promote",
            post(league_teams::promote_to_captain),
        )
        .route(
            "/{team_season_id}/members/{player_id}/demote",
            post(league_teams::demote_from_captain),
        )
        .route("/{team_season_id}/leave", post(league_teams::leave_team))
        // Invitations for the team season
        .route(
            "/{team_season_id}/invitations",
            get(league_teams::get_team_invitations),
        )
        .route(
            "/{team_season_id}/invitations",
            post(league_teams::invite_to_team),
        )
        .route("/{team_season_id}/apply", post(league_teams::apply_to_team))
}

/// League team invitation routes (user-centric, for responding to invitations).
pub fn invitation_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(league_teams::get_my_invitations))
        .route(
            "/{invitation_id}/accept",
            post(league_teams::accept_invitation),
        )
        .route(
            "/{invitation_id}/decline",
            post(league_teams::decline_invitation),
        )
        .route("/{invitation_id}", delete(league_teams::cancel_invitation))
}

/// Player league team routes (for viewing player's teams).
pub fn player_league_team_routes() -> Router<AppState> {
    Router::new()
        .route("/me/league-teams", get(league_teams::get_my_league_teams))
        .route(
            "/{player_id}/league-teams",
            get(league_teams::get_player_league_teams),
        )
}
