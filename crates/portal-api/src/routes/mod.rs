//! API routes.

pub mod admin;
pub mod auth;
pub mod availability;
pub mod demos;
pub mod disputes;
pub mod games;
pub mod league_teams;
pub mod leagues;
pub mod matches;
pub mod players;
pub mod tournaments;
pub mod users;
pub mod veto_delegates;
pub mod websocket;

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
        // Veto delegate routes (nested under leagues/teams/seasons)
        .nest(
            "/leagues/{league_id}/teams/{team_id}/seasons/{season_id}/veto-delegates",
            veto_delegates::routes(),
        )
        // Player league team routes (nested under /players)
        .nest("/players", league_teams::player_league_team_routes())
        // Player availability routes (nested under /players)
        .nest("/players/me/availability", availability::player_availability_routes())
        .nest(
            "/players/{player_id}/availability",
            availability::player_public_availability_routes(),
        )
        // Tournament routes
        .nest("/tournaments", tournaments::routes())
        // Match routes (veto, results)
        .nest("/matches", matches::routes())
        // Dispute routes
        .nest("/disputes", disputes::routes())
        // Demo routes
        .nest("/demos", demos::routes())
        // WebSocket routes
        .nest("/ws", websocket::routes())
}
