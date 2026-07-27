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
pub mod lineup;
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
pub use lineup::*;
pub use map_pool::*;
pub use match_lifecycle::*;
pub use registration::*;
pub use scheduling::*;
pub use seeding::*;
pub use stages::*;

use crate::error::ApiError;
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::TournamentState;
use axum::http::HeaderMap;
use portal_core::{PlayerId, ScopeType, TournamentRegistrationId};

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

/// Require that the caller may act *on behalf of* `registration_id`.
///
/// P-24: check-in used to accept any authenticated caller and any
/// registration id, so a stranger could check a team in — and because
/// both-checked-in auto-advances a match to `pick_ban` / `in_progress`,
/// force someone else's match to start. The stored
/// `participantN_checked_in_by` was therefore meaningless as audit.
///
/// Rather than invent a second notion of "speaks for this participant"
/// we reuse the one veto already established
/// (`VetoAuthorizationService::can_act_for_registration`): registration
/// → team-season → captain / team owner / active veto delegate, and for
/// individual registrations (`team_season_id IS NULL`, `player_id` set)
/// the registered player themself.
///
/// Tournament staff are allowed through as an override, using the same
/// scoped `tournament.participants.manage` permission that gates
/// `admin_check_in` / `approve_registration` next door — with the usual
/// automatic fallback to the global `admin.tournaments.manage_any`.
/// Checking staff first keeps the participant model's more specific
/// error message for the (common) non-staff denial.
///
/// Failure is `403` — `DomainError::NotAuthorized` and
/// `ApiError::forbidden` both map there.
pub(super) async fn require_registration_actor(
    state: &TournamentState,
    auth: &AuthenticatedUser,
    perm_checker: &PermissionChecker,
    registration_id: TournamentRegistrationId,
) -> Result<(), ApiError> {
    // Resolve the tournament from the registration row rather than the
    // path segment — same reasoning as `require_registration_manage`:
    // otherwise staff of tournament A could act on tournament B's
    // registration by crafting the URL.
    let registration = state
        .registration_service
        .get_registration(registration_id)
        .await?;

    let tournament_uuid = registration.tournament_id.as_uuid();
    if perm_checker
        .has_scoped_permission(
            auth,
            portal_core::permissions::tournament::PARTICIPANTS_MANAGE,
            ScopeType::Tournament,
            tournament_uuid,
        )
        .await
        || perm_checker
            .has_admin_override(auth, ScopeType::Tournament)
            .await
    {
        return Ok(());
    }

    state
        .veto_authorization_service
        .can_act_for_registration(registration_id, auth.user_id, auth.player_id)
        .await?;

    Ok(())
}

/// Resolve the restrictions that actually bind a tournament: its own,
/// composed strictest-wins with its league's entry requirements when it
/// belongs to a league. A league tournament may tighten league rules but
/// never loosen them — previously a league's rating floor simply did not
/// apply to its tournaments at all.
pub(super) async fn effective_restrictions(
    state: &TournamentState,
    tournament: &portal_domain::entities::Tournament,
) -> Result<portal_domain::entities::eligibility::EligibilityRestrictions, ApiError> {
    let own = tournament.eligibility_restrictions();
    let Some(league_id) = tournament.league_id else {
        return Ok(own);
    };
    let league = state.league_service.get_league(league_id).await?;
    let league_restrictions =
        portal_domain::entities::eligibility::EligibilityRestrictions::from_settings(
            &league.settings,
        );
    Ok(own.intersect(&league_restrictions))
}

fn eligibility_error(
    violations: &[portal_domain::entities::eligibility::EligibilityViolation],
) -> ApiError {
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
    ApiError::bad_request(format!("Eligibility check failed: {}", messages.join("; ")))
}

/// Check per-player eligibility for a set of player IDs against a
/// tournament's effective restrictions.
///
/// `pub(super)` because it's called by the register handlers in
/// `registration.rs` and nowhere else — keeping it out of the public
/// surface avoids leaking an internal enforcement path.
pub(super) async fn check_eligibility_for_players(
    state: &TournamentState,
    tournament: &portal_domain::entities::Tournament,
    player_ids: &[PlayerId],
) -> Result<(), ApiError> {
    let restrictions = effective_restrictions(state, tournament).await?;
    let violations = state
        .eligibility_service
        .check_players(&restrictions, tournament.game_id, player_ids)
        .await?;

    if violations.is_empty() {
        Ok(())
    } else {
        Err(eligibility_error(&violations))
    }
}

/// Check a registering team's full roster: per-player restrictions on every
/// member plus the team-aggregate rating bounds (min and max, total and
/// average).
pub(super) async fn check_eligibility_for_team(
    state: &TournamentState,
    tournament: &portal_domain::entities::Tournament,
    player_ids: &[PlayerId],
) -> Result<(), ApiError> {
    let restrictions = effective_restrictions(state, tournament).await?;
    let violations = state
        .eligibility_service
        .check_team(&restrictions, tournament.game_id, player_ids)
        .await?;

    if violations.is_empty() {
        Ok(())
    } else {
        Err(eligibility_error(&violations))
    }
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

    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;

    // Veto format: stage override → tournament default → derived from the
    // match's own best-of. Standard boN ids re-key to the match format so a
    // mixed-format bracket vetos each match at its own series length.
    let stage_veto_override = state
        .tournament_service
        .get_stages(match_.tournament_id)
        .await?
        .into_iter()
        .find(|s| s.id == match_.stage_id)
        .and_then(|s| s.map_veto_format);
    let veto_format = crate::handlers::veto::resolve_match_veto_format(
        stage_veto_override.as_deref(),
        tournament.default_map_veto_format.as_deref(),
        match_.match_format,
        &state.plugin_manager,
    )?
    .unwrap_or_else(|| crate::handlers::veto::builtin_veto_for(match_.match_format));

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
            plugin.as_tournament_plugin().map_or(
                SideSelectionMode::Knife,
                portal_plugins::TournamentPlugin::default_side_selection_mode,
            )
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
