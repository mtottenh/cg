//! Routes for veto delegate management.

use axum::routing::{delete, get, post};
use axum::Router;

use crate::handlers::veto_delegates;
use crate::state::AppState;

/// Build veto delegate routes.
///
/// These routes are nested under `/v1/leagues/{league_id}/teams/{team_id}/seasons/{season_id}/veto-delegates`
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", post(veto_delegates::create_delegation))
        .route("/", get(veto_delegates::list_delegations))
        .route("/{delegate_id}", delete(veto_delegates::revoke_delegation))
}
