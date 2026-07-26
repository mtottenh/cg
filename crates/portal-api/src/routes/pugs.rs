//! Routes for pick-up games (PUGs).
//!
//! The share-link endpoints (`/code/{code}` preview + join) are keyed by an
//! unauthenticated-reachable invite code, so they get the same per-IP
//! token-bucket the auth routes use (brute-force protection on top of the
//! ~49-bit code entropy).

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post, put};
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;

use crate::handlers::pugs;
use crate::state::AppState;

/// Build PUG routes (nested under `/v1/pugs`).
pub fn routes() -> Router<AppState> {
    let code_routes = Router::new()
        .route("/code/{code}", get(pugs::preview_by_code))
        .route("/code/{code}/join", post(pugs::join_by_code));

    // Same defaults as the auth limiter; generous for humans, hostile to
    // code scanners.
    let code_routes = match GovernorConfigBuilder::default()
        .per_second(5)
        .burst_size(20)
        .finish()
    {
        Some(config) => code_routes.layer(GovernorLayer {
            config: Arc::new(config),
        }),
        None => code_routes,
    };

    Router::new()
        .route("/", post(pugs::create_pug))
        .route("/mine", get(pugs::my_pugs))
        .route("/open", get(pugs::open_pugs))
        .route("/recent", get(pugs::recent_pugs))
        .merge(code_routes)
        .route("/{pug_id}", get(pugs::get_pug))
        .route("/{pug_id}/leave", post(pugs::leave_pug))
        .route("/{pug_id}/kick", post(pugs::kick_player))
        .route("/{pug_id}/team", put(pugs::set_team))
        .route("/{pug_id}/captain", put(pugs::set_captain))
        .route("/{pug_id}/shuffle", post(pugs::shuffle_teams))
        .route("/{pug_id}/swap-teams", post(pugs::swap_teams))
        .route("/{pug_id}/draft", post(pugs::draft_pick))
        .route("/{pug_id}/code/rotate", post(pugs::rotate_code))
        .route("/{pug_id}/wheel-entry", put(pugs::nominate_map))
        .route("/{pug_id}/lock", post(pugs::lock_pug))
        .route("/{pug_id}/spin", post(pugs::spin_wheel))
        .route("/{pug_id}/cancel", post(pugs::cancel_pug))
        .route("/{pug_id}/rematch", post(pugs::rematch_pug))
}
