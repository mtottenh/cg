//! League team handlers.
//!
//! These handlers manage league-scoped teams, seasons, members, and invitations.
//! Teams have persistent identity at the league level, with seasonal participation
//! tracked via `team_seasons`. Players can only be a primary member of one team per season.
//!
//! This module is organized into:
//! - `season`: Season management (create, update, list)
//! - `team`: Persistent team management (create, update, disband, transfer)
//! - `team_season`: Seasonal roster management (members, roles)
//! - `invitation`: Team invitations and applications

pub mod invitation;
pub mod season;
pub mod team;
pub mod team_season;

pub use invitation::{
    accept_invitation, apply_to_team, cancel_invitation, decline_invitation, get_my_invitations,
    get_team_invitations, invite_to_team,
};
pub use season::{
    ListSeasonsParams, archive_season, create_season, get_season, list_seasons, restore_season,
    update_season,
};
pub use team::{
    ListLeagueTeamsParams, ListTeamSeasonsParams, archive_team, create_team, disband_team,
    get_team, list_teams_in_season, register_team_for_season, restore_team, transfer_ownership,
    update_team,
};
pub use team_season::{
    add_team_member, demote_from_captain, get_my_league_teams, get_player_league_teams,
    get_team_season, get_team_season_members, leave_team, promote_to_captain, remove_team_member,
};

use axum::http::HeaderMap;

use crate::error::ApiError;
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::LeagueTeamState;
use portal_core::{LeagueTeamSeasonId, PlayerId, ScopeType};
use portal_domain::entities::eligibility::EligibilityRestrictions;
use portal_domain::services::RosterLockOverride;

/// Enforce the league's entry requirements on a roster addition.
///
/// Two checks, both against the league that owns the team's season:
///
/// 1. The joining player must meet the per-player entry rules. League join
///    and application already check these, but team invitations reach
///    players a league admin invited directly (bypassing the join check), so
///    this is the backstop that keeps rating-gated leagues honest.
/// 2. The prospective roster (current members + the addition) must stay
///    under the league's team-total rating cap — the one aggregate a later
///    addition can never repair. Average caps and minimum bounds bind at
///    tournament registration instead, where the roster is final
///    (see `EligibilityRestrictions::team_total_cap_only`).
pub(crate) async fn check_roster_addition(
    state: &LeagueTeamState,
    team_season_id: LeagueTeamSeasonId,
    joining_player: PlayerId,
) -> Result<(), ApiError> {
    let team_season = state
        .league_team_service
        .get_team_season(team_season_id)
        .await?;
    let season = state
        .league_season_service
        .get_season(team_season.season_id)
        .await?;
    let league = state.league_service.get_league(season.league_id).await?;

    let restrictions = EligibilityRestrictions::from_settings(&league.settings);
    if !restrictions.has_restrictions() {
        return Ok(());
    }

    let mut violations = state
        .eligibility_service
        .check_players(&restrictions, league.game_id, &[joining_player])
        .await?;

    let total_cap = restrictions.team_total_cap_only();
    if total_cap.has_restrictions() {
        let mut roster: Vec<PlayerId> = state
            .league_team_service
            .get_members(team_season_id)
            .await?
            .iter()
            .map(|m| m.player_id)
            .collect();
        roster.push(joining_player);
        violations.extend(
            state
                .eligibility_service
                .check_team(&total_cap, league.game_id, &roster)
                .await?,
        );
    }

    if violations.is_empty() {
        return Ok(());
    }
    let messages: Vec<String> = violations.iter().map(|v| v.message.clone()).collect();
    Err(ApiError::bad_request(format!(
        "League entry requirements not met: {}",
        messages.join("; ")
    )))
}

/// Roster-lock override carried in the query string.
///
/// The three roster endpoints that take no request body (remove member,
/// promote, demote) accept the override here; `add_team_member` takes the same
/// two fields in its JSON body. See [`resolve_roster_lock_override`].
#[derive(Debug, Default, serde::Deserialize, utoipa::IntoParams)]
pub struct RosterLockOverrideParams {
    /// Bypass the season's roster lock (platform team admins only).
    #[serde(default)]
    pub override_roster_lock: bool,

    /// Why the lock was overridden. Required when `override_roster_lock` is
    /// set; at least 10 characters.
    #[serde(default)]
    pub override_reason: Option<String>,
}

/// Minimum length of an override justification, in characters.
///
/// Mirrors the `#[validate(length(min = 10))]` on
/// `AddLeagueTeamMemberRequest::override_reason` so the body and query-string
/// forms of the override cannot drift apart.
const MIN_OVERRIDE_REASON_LEN: usize = 10;

/// Turn a requested roster-lock override into a domain [`RosterLockOverride`],
/// or refuse it (P-18).
///
/// Two things make this an *audited* override rather than a hole in the lock:
///
/// 1. It is gated on the **platform** team-admin override, not on captaincy —
///    a captain cannot unlock their own roster, which is the entire point of
///    the lock.
/// 2. A justification is mandatory. It is carried into the audit row written by
///    the domain enforcement point before the mutation is allowed to proceed,
///    so "who, when, why" is always recorded.
pub(crate) async fn resolve_roster_lock_override(
    perm: &PermissionChecker,
    auth: &AuthenticatedUser,
    requested: bool,
    reason: Option<String>,
    request_id: &str,
) -> Result<Option<RosterLockOverride>, ApiError> {
    if !requested {
        return Ok(None);
    }

    if !perm.has_admin_override(auth, ScopeType::Team).await {
        return Err(ApiError::forbidden(
            "Only platform team admins can override a roster lock",
        ));
    }

    let reason = reason.unwrap_or_default().trim().to_string();
    if reason.chars().count() < MIN_OVERRIDE_REASON_LEN {
        return Err(ApiError::bad_request(format!(
            "override_reason is required when overriding the roster lock and must be at least {MIN_OVERRIDE_REASON_LEN} characters"
        )));
    }

    Ok(Some(RosterLockOverride {
        overridden_by: auth.player_id,
        reason,
        request_id: (request_id != "unknown").then(|| request_id.to_string()),
    }))
}

/// Extract request ID from headers.
pub(crate) fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

pub(crate) const fn default_page() -> i64 {
    1
}

pub(crate) const fn default_per_page() -> i64 {
    20
}

/// Allow the call if the caller is a captain on `team_season_id` **or** holds
/// the platform's team-admin override (`admin.teams.manage_any`). Returns
/// 403 with a descriptive message otherwise.
///
/// Why both: captain is the per-team role that naturally gates roster
/// management, and `admin.teams.manage_any` exists exactly so platform
/// moderators can intervene without being added to every team. Previously
/// handlers checked only `is_captain`, so admins were locked out — a bug
/// flagged as I1 in the audit.
pub(crate) async fn require_captain_or_admin(
    state: &LeagueTeamState,
    perm: &PermissionChecker,
    auth: &AuthenticatedUser,
    team_season_id: LeagueTeamSeasonId,
    action: &str,
) -> Result<(), ApiError> {
    let is_captain = state
        .league_team_service
        .is_captain(team_season_id, auth.player_id)
        .await?;
    if is_captain {
        return Ok(());
    }
    if perm.has_admin_override(auth, ScopeType::Team).await {
        return Ok(());
    }
    Err(ApiError::forbidden(format!(
        "Only captains or platform admins can {action}"
    )))
}
