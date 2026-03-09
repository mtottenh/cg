//! Admin routes.

use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::handlers::{admin, bans, demos, dispute, forfeit, progression, result_reviews, roles, tournaments};
use crate::state::AppState;

/// Create routes for admin endpoints.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats", get(admin::get_stats))
        // Ban routes
        .route("/bans", get(bans::list_bans).post(bans::create_ban))
        .route("/bans/{id}", get(bans::get_ban))
        .route("/bans/{id}/lift", post(bans::lift_ban))
        .route("/users/{user_id}/bans", get(bans::get_user_bans))
        // Role routes
        .route("/roles", get(roles::list_roles).post(roles::create_role))
        .route(
            "/roles/{role_id}",
            get(roles::get_role)
                .patch(roles::update_role)
                .delete(roles::delete_role),
        )
        .route(
            "/roles/{role_id}/permissions",
            post(roles::add_permission_to_role),
        )
        .route(
            "/roles/{role_id}/permissions/{permission_id}",
            delete(roles::remove_permission_from_role),
        )
        // Permission routes
        .route("/permissions", get(roles::list_permissions))
        // User role assignment routes
        .route(
            "/users/{user_id}/roles",
            get(roles::get_user_roles).post(roles::assign_role_to_user),
        )
        .route(
            "/users/{user_id}/roles/{role_id}",
            delete(roles::revoke_role_from_user),
        )
        // Tournament admin routes
        .route(
            "/tournaments/{tournament_id}/matches/{match_id}/transition",
            post(tournaments::admin_match_transition),
        )
        .route(
            "/tournaments/{tournament_id}/matches/{match_id}/schedule",
            post(tournaments::admin_schedule_match),
        )
        // Swiss next round generation
        .route(
            "/tournaments/{tournament_id}/generate-next-round",
            post(tournaments::admin_generate_next_swiss_round),
        )
        // Progression admin routes
        .route(
            "/matches/{match_id}/progression/revert",
            post(progression::revert_progression),
        )
        .route(
            "/matches/{match_id}/progression/reapply",
            post(progression::reapply_progression),
        )
        .route(
            "/matches/{match_id}/progression/process",
            post(progression::process_progression),
        )
        // Forfeit admin routes
        .route(
            "/tournaments/{tournament_id}/matches/{match_id}/forfeit",
            post(forfeit::admin_forfeit_match),
        )
        .route(
            "/tournaments/{tournament_id}/matches/{match_id}/double-forfeit",
            post(forfeit::admin_double_forfeit),
        )
        .route(
            "/tournaments/{tournament_id}/registrations/{registration_id}/disqualify",
            post(forfeit::admin_disqualify),
        )
        // Dispute admin routes
        .route("/disputes", get(dispute::admin_list_disputes))
        .route(
            "/disputes/{dispute_id}/messages",
            post(dispute::admin_add_message),
        )
        .route(
            "/disputes/{dispute_id}/assign",
            post(dispute::admin_assign_dispute),
        )
        .route(
            "/disputes/{dispute_id}/resolve/uphold",
            post(dispute::admin_resolve_uphold),
        )
        .route(
            "/disputes/{dispute_id}/resolve/overturn",
            post(dispute::admin_resolve_overturn),
        )
        .route(
            "/disputes/{dispute_id}/resolve/rematch",
            post(dispute::admin_resolve_rematch),
        )
        .route(
            "/disputes/{dispute_id}/resolve/adjusted",
            post(dispute::admin_resolve_adjusted),
        )
        .route(
            "/disputes/{dispute_id}/resolve/double-dq",
            post(dispute::admin_resolve_double_dq),
        )
        // Demo admin routes
        .route(
            "/demos/{id}",
            delete(demos::delete_demo),
        )
        .route("/demos/{id}/notes", patch(demos::set_demo_notes))
        .route("/demos", post(demos::catalog_demo))
        .route("/demos/batch", post(demos::batch_catalog_demos))
        .route("/demos/stats", get(demos::get_demo_status_counts))
        .route("/demos/pending", get(demos::get_pending_demos))
        .route("/demos/{id}/stats", post(demos::submit_demo_stats))
        .route("/demos/{id}/stats-failed", post(demos::mark_demo_stats_failed))
        .route("/demos/{id}/categorize", post(demos::categorize_demo))
        .route("/demos/{id}/visibility", post(demos::set_demo_visibility))
        .route("/demos/{id}/associate", post(demos::associate_demo))
        .route("/demos/{id}/link", post(demos::link_demo_to_match))
        .route(
            "/demos/{demo_id}/link/{match_id}",
            delete(demos::unlink_demo_from_match),
        )
        // Result review admin routes
        .route("/result-reviews", get(result_reviews::list_pending_reviews))
        .route("/result-reviews/{id}", get(result_reviews::get_result_review_by_id))
        .route("/result-reviews/{id}/approve", post(result_reviews::approve_result_review))
        .route("/result-reviews/{id}/reject", post(result_reviews::reject_result_review))
}
