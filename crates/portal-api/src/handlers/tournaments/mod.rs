//! Tournament handlers — composite module.
//!
//! The tournament handler surface is large (40+ endpoints spanning
//! CRUD, registration, seeding, brackets, match lifecycle, scheduling,
//! and map pool). It lives in a `tournaments/` directory split by
//! concern:
//!
//! | sub-module        | responsibility                                          |
//! |-------------------|---------------------------------------------------------|
//! | `lifecycle`       | tournament CRUD + state machine transitions             |
//! | `stages`          | stage configuration                                     |
//! | `registration`    | register / check-in / withdraw / approve / disqualify   |
//! | `brackets`        | bracket + match read endpoints                          |
//! | `seeding`         | auto / manual / clear seeding                           |
//! | `match_lifecycle` | match status + check-in + forfeit + admin transitions   |
//! | `scheduling`      | proposal workflow, admin schedule, bracket standings    |
//! | `map_pool`        | tournament-level map pool override                      |
//!
//! This file carries only shared helpers (`get_request_id`,
//! `check_eligibility_for_players`, `auto_create_veto_session`) and the
//! `pub use` glob re-exports that keep the existing `tournaments::*`
//! paths — referenced by `openapi.rs` and `routes/*` — valid.

pub mod brackets;
pub mod lifecycle;
pub mod map_pool;
pub mod match_lifecycle;
pub mod registration;
pub mod scheduling;
pub mod seeding;
pub mod stages;

// Glob re-export so every handler is accessible as
// `handlers::tournaments::<name>` — the path `openapi.rs` and the
// `routes/*` modules already use — *and* so the `__path_<handler>`
// types that utoipa's `#[utoipa::path(...)]` macro generates sit at
// that same module path (utoipa's `paths(...)` resolves against it).
pub use brackets::*;
pub use lifecycle::*;
pub use map_pool::*;
pub use match_lifecycle::*;
pub use registration::*;
pub use scheduling::*;
pub use seeding::*;
pub use stages::*;

use crate::error::ApiError;
use crate::state::TournamentState;
use axum::http::HeaderMap;
use portal_core::types::MatchFormat;
use portal_core::{PlayerId, VetoFormatConfig};

/// Extract the request id from incoming headers, falling back to
/// `"unknown"` if absent or not ASCII.
///
/// `pub(super)` so every sub-module can reuse the same helper without
/// duplicating it.
pub(super) fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Check eligibility restrictions for a set of player IDs against a tournament.
///
/// Delegates to the `EligibilityService` which fetches each player's game
/// profile and rating stats for the tournament's game, then runs the checker.
///
/// `pub(super)` because it's called by the team-register and
/// player-register handlers in `registration.rs` and nowhere else —
/// keeping it out of the public surface avoids leaking an internal
/// enforcement path.
pub(super) async fn check_eligibility_for_players(
    state: &TournamentState,
    tournament: &portal_domain::entities::Tournament,
    player_ids: &[PlayerId],
) -> Result<(), ApiError> {
    let restrictions = tournament.eligibility_restrictions();
    let violations = state
        .eligibility_service
        .check_players(&restrictions, tournament.game_id, player_ids)
        .await?;

    if violations.is_empty() {
        return Ok(());
    }

    let messages: Vec<String> = violations
        .iter()
        .map(|v| {
            if v.player_id == PlayerId::from_uuid(uuid::Uuid::nil()) {
                format!("[{}] {}", v.restriction, v.message)
            } else {
                format!("[{}] Player {}: {}", v.restriction, v.player_id, v.message)
            }
        })
        .collect();
    Err(ApiError::bad_request(format!(
        "Eligibility check failed: {}",
        messages.join("; ")
    )))
}

/// Auto-create and start a veto session when a match transitions to PickBan.
///
/// Called after both participants check in for a veto-required match,
/// or when an admin force-transitions the match into PickBan. Derives
/// the veto format from the match format and loads the map pool from
/// tournament config (or falls back to the game's default pool).
///
/// `pub(super)` because only `match_lifecycle::match_check_in` and
/// `match_lifecycle::admin_match_transition` trigger this path — it's
/// an internal side effect of the status transition, not a standalone
/// API.
pub(super) async fn auto_create_veto_session(
    state: &TournamentState,
    match_: &portal_domain::entities::tournament::TournamentMatch,
) -> Result<(), ApiError> {
    use portal_domain::repositories::tournament::TournamentMapPoolRepository;

    // Derive veto format from match format
    let veto_format = match match_.match_format {
        MatchFormat::Bo1 => VetoFormatConfig::bo1(),
        MatchFormat::Bo3 => VetoFormatConfig::bo3(),
        MatchFormat::Bo5 | MatchFormat::Bo7 => VetoFormatConfig::bo5(),
    };

    // Resolve map pool and side selection mode
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;

    let map_pool = if let Ok(Some(pool)) = state
        .tournament_map_pool_repo
        .get_effective(match_.tournament_id, Some(match_.stage_id))
        .await
    {
        pool.maps
    } else {
        // Fall back to game's default pool
        if let Ok(Some(game)) = state
            .game_repo
            .find_by_id(tournament.game_id.as_uuid())
            .await
        {
            crate::handlers::games::extract_map_pool(&game)
        } else {
            vec![]
        }
    };

    // Resolve side selection mode: tournament settings → plugin default
    // No conversion needed — both plugin and domain use portal_core::SideSelectionMode
    let side_selection_mode = {
        use portal_core::SideSelectionMode;

        if let Some(mode) = tournament
            .settings
            .get("side_selection_mode")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<SideSelectionMode>().ok())
        {
            mode
        } else if let Some(plugin) = state.plugin_manager.get(&tournament.game_id.to_string()) {
            plugin
                .as_tournament_plugin()
                .map(|tp| tp.default_side_selection_mode())
                .unwrap_or(SideSelectionMode::Knife)
        } else {
            SideSelectionMode::Knife
        }
    };

    // Create the session
    let session = state
        .veto_service
        .create_session(match_.id, &veto_format, map_pool, None, side_selection_mode)
        .await?;

    // Auto-start the session (begins coin flip phase)
    state.veto_service.start_session(session.id).await?;

    tracing::info!(
        match_id = %match_.id,
        session_id = %session.id,
        format = %veto_format.id,
        "Auto-created and started veto session on pick_ban transition"
    );

    Ok(())
}
