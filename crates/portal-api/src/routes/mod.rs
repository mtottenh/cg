//! API routes.

pub mod admin;
pub mod auth;
pub mod games;
pub mod league_teams;
pub mod leagues;
pub mod players;
pub mod tournaments;
pub mod users;

use axum::Router;
use crate::state::AppState;

/// Create all API routes.
pub fn api_routes() -> Router<AppState> {
    Router::new()
        .nest("/admin", admin::routes())
        .nest("/auth", auth::routes())
        .nest("/users", users::routes())
        .nest("/players", players::routes())
        .nest("/games", games::routes())
        .nest("/leagues", leagues::routes())
        .nest("/league-invitations", leagues::invitation_routes())
        // League team routes
        .nest("/league-seasons", league_teams::season_routes())
        .nest("/league-teams", league_teams::team_routes())
        .nest("/league-team-seasons", league_teams::team_season_routes())
        .nest("/league-team-invitations", league_teams::invitation_routes())
        // Player league team routes (nested under /players)
        .nest("/players", league_teams::player_league_team_routes())
        // Tournament routes
        .nest("/tournaments", tournaments::routes())
}
