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
pub use season::{create_season, get_season, list_seasons, update_season, ListSeasonsParams};
pub use team::{
    create_team, disband_team, get_team, list_teams_in_season, register_team_for_season,
    transfer_ownership, update_team, ListLeagueTeamsParams, ListTeamSeasonsParams,
};
pub use team_season::{
    add_team_member, demote_from_captain, get_my_league_teams, get_player_league_teams,
    get_team_season, get_team_season_members, leave_team, promote_to_captain, remove_team_member,
};

use axum::http::HeaderMap;

use crate::error::ApiError;
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::LeagueTeamState;
use portal_core::{LeagueTeamSeasonId, ScopeType};

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
