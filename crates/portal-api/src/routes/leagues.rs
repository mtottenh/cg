//! League routes.

use crate::handlers::leagues;
use crate::state::AppState;
use axum::routing::{delete, get, patch, post};
use axum::Router;

/// League routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        // League CRUD
        .route("/", post(leagues::create_league))
        .route("/", get(leagues::list_leagues))
        .route("/{league_id}", get(leagues::get_league))
        .route("/{league_id}", patch(leagues::update_league))
        .route("/by-slug/{slug}", get(leagues::get_league_by_slug))
        // League membership
        .route("/{league_id}/members", get(leagues::list_members))
        .route("/{league_id}/members/{user_id}", patch(leagues::update_member_role))
        .route("/{league_id}/members/{user_id}", delete(leagues::remove_member))
        .route("/{league_id}/join", post(leagues::join_league))
        .route("/{league_id}/leave", post(leagues::leave_league))
        // Applications & Invitations
        .route("/{league_id}/apply", post(leagues::apply_to_league))
        .route("/{league_id}/invitations", post(leagues::invite_user))
        .route("/{league_id}/invitations", get(leagues::list_invitations))
        .route("/{league_id}/applications", get(leagues::list_applications))
        .route("/{league_id}/applications/{application_id}/approve", post(leagues::approve_application))
        .route("/{league_id}/applications/{application_id}/reject", post(leagues::reject_application))
}

/// League invitation routes (user-centric, mounted separately).
pub fn invitation_routes() -> Router<AppState> {
    Router::new()
        .route("/{invitation_id}/accept", post(leagues::accept_invitation))
        .route("/{invitation_id}/decline", post(leagues::decline_invitation))
}
