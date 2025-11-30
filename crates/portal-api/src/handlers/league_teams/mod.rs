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
