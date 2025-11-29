//! Team routes.

use crate::handlers::{invitations, teams, uploads};
use crate::state::AppState;
use axum::routing::{delete, get, patch, post};
use axum::Router;

/// Team routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Team CRUD
        .route("/", post(teams::create_team))
        .route("/", get(teams::list_teams))
        .route("/{team_id}", get(teams::get_team))
        .route("/{team_id}", patch(teams::update_team))
        // Team members
        .route("/{team_id}/members", get(teams::list_members))
        .route("/{team_id}/members/{player_id}", patch(teams::update_member_role))
        .route("/{team_id}/members/{player_id}", delete(teams::remove_member))
        .route("/{team_id}/leave", post(teams::leave_team))
        // Team invitations
        .route("/{team_id}/invitations", post(invitations::invite_player))
        .route("/{team_id}/invitations", get(invitations::get_team_invitations))
        // Team uploads
        .route("/{team_id}/logo", post(uploads::upload_team_logo))
        .route("/{team_id}/banner", post(uploads::upload_team_banner))
}
