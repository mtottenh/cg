//! API routes.

pub mod admin;
pub mod auth;
pub mod games;
pub mod invitations;
pub mod leagues;
pub mod players;
pub mod teams;
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
        .nest("/teams", teams::routes())
        .nest("/invitations", invitations::routes())
        .nest("/games", games::routes())
        .nest("/leagues", leagues::routes())
        .nest("/league-invitations", leagues::invitation_routes())
}
