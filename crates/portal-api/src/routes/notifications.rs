//! The caller's own notifications (`/v1/me/notifications`).

use crate::handlers::notifications;
use crate::state::AppState;
use axum::Router;
use axum::routing::{get, post};

/// Create the `/me` notification routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(notifications::list_my_notifications))
        .route(
            "/notifications/unread-count",
            get(notifications::my_unread_count),
        )
        .route(
            "/notifications/read",
            post(notifications::mark_my_notifications_read),
        )
}
