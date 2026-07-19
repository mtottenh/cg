//! Match routes for veto, result submission, and evidence.

use crate::handlers::{demos, evidence, progression, result_reviews, results, veto};
use crate::state::AppState;
use axum::Router;
use axum::routing::{delete, get, post};

/// Match routes including veto, result submission, and evidence.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Result submission endpoints
        .route("/{match_id}/result", post(results::submit_result))
        .route("/{match_id}/result", get(results::get_result_claim))
        .route(
            "/{match_id}/result/history",
            get(results::list_result_claims),
        )
        .route(
            "/{match_id}/result/{claim_id}/confirm",
            post(results::confirm_result),
        )
        .route(
            "/{match_id}/result/{claim_id}/dispute",
            post(results::dispute_result),
        )
        // Veto session endpoints
        .route("/{match_id}/veto", post(veto::create_veto_session))
        .route("/{match_id}/veto", get(veto::get_veto_session))
        .route("/{match_id}/veto/start", post(veto::start_veto_session))
        .route("/{match_id}/veto/coin-flip", post(veto::record_coin_flip))
        .route("/{match_id}/veto/action", post(veto::perform_veto_action))
        .route("/{match_id}/veto/side", post(veto::select_side))
        // Evidence endpoints
        .route("/{match_id}/evidence", get(evidence::list_evidence))
        .route(
            "/{match_id}/evidence/upload",
            post(evidence::initiate_upload),
        )
        .route(
            "/{match_id}/evidence/link",
            post(evidence::add_link_evidence),
        )
        .route(
            "/{match_id}/evidence/discover",
            get(evidence::discover_evidence),
        )
        .route(
            "/{match_id}/evidence/link-discovered",
            post(evidence::link_discovered_evidence),
        )
        .route(
            "/{match_id}/evidence/validate",
            post(evidence::validate_evidence),
        )
        // CS2 Demo validation endpoints
        .route(
            "/{match_id}/evidence/validate-demo",
            post(evidence::validate_demo),
        )
        .route("/{match_id}/evidence/link-demo", post(evidence::link_demo))
        .route(
            "/{match_id}/evidence/demo-stats/{demo_name}",
            get(evidence::get_demo_stats),
        )
        // Evidence instance endpoints
        .route(
            "/{match_id}/evidence/{evidence_id}",
            get(evidence::get_evidence),
        )
        .route(
            "/{match_id}/evidence/{evidence_id}",
            delete(evidence::delete_evidence),
        )
        .route(
            "/{match_id}/evidence/{evidence_id}/complete",
            post(evidence::complete_upload),
        )
        .route(
            "/{match_id}/evidence/{evidence_id}/access",
            get(evidence::get_access_url),
        )
        // Progression endpoints
        .route("/{match_id}/progression", get(progression::get_progression))
        // Demo endpoints
        .route("/{match_id}/demos", get(demos::get_demos_for_match))
        // Result review endpoints
        .route(
            "/{match_id}/result-review",
            get(result_reviews::get_result_review),
        )
        .route(
            "/{match_id}/result-review/acknowledge",
            post(result_reviews::acknowledge_result_review),
        )
}
