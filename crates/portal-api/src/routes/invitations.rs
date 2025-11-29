//! Invitation routes.

use crate::handlers::invitations;
use crate::state::AppState;
use axum::routing::{delete, get, post};
use axum::Router;

/// Invitation routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Player's own invitations
        .route("/me", get(invitations::get_my_invitations))
        .route("/me/count", get(invitations::count_my_invitations))
        // Invitation actions
        .route("/{invitation_id}/accept", post(invitations::accept_invitation))
        .route("/{invitation_id}/decline", post(invitations::decline_invitation))
        .route("/{invitation_id}", delete(invitations::cancel_invitation))
}
