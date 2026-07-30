//! Internal API routes for bot/service endpoints.
//!
//! All routes require `X-API-Key` authentication (no JWT).

use crate::handlers::internal;
use crate::state::AppState;
use axum::Router;
use axum::routing::{get, patch, post};

/// Create internal API routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Steam tracking
        .route("/steam-tracking/active", get(internal::get_active_tracking))
        .route(
            "/steam-tracking/{id}/poll-result",
            patch(internal::update_poll_result),
        )
        // Discovered matches
        .route(
            "/discovered-matches",
            post(internal::submit_discovered_matches),
        )
        .route(
            "/discovered-matches/pending",
            get(internal::get_pending_matches),
        )
        .route(
            "/discovered-matches/recent-demos",
            get(internal::get_recent_demo_matches),
        )
        .route(
            "/discovered-matches/{id}/claim",
            post(internal::claim_match),
        )
        .route(
            "/discovered-matches/{id}/enriched",
            post(internal::submit_enriched),
        )
        .route(
            "/discovered-matches/{id}/failed",
            post(internal::mark_failed),
        )
        // Demo extraction — a retried stage in its own right, not a side
        // effect of enrichment. Leasing mutates (it banks an attempt against
        // the row before any work starts), hence POST rather than GET.
        .route(
            "/discovered-matches/demo-jobs",
            post(internal::lease_demo_jobs),
        )
        .route(
            "/discovered-matches/{id}/demo-result",
            post(internal::submit_demo_result),
        )
        // Demos (scanner)
        .route("/demos/batch", post(internal::internal_batch_catalog_demos))
        .route("/demos/pending", get(internal::internal_get_pending_demos))
        .route(
            "/demos/{demo_id}/stats",
            post(internal::internal_submit_demo_stats),
        )
        .route(
            "/demos/{demo_id}/stats-failed",
            post(internal::internal_mark_demo_stats_failed),
        )
}
