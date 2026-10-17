//! League team services with business logic.
//!
//! This module contains services for managing league-scoped teams, seasons,
//! and team memberships.
//!
//! # Architecture Changes (League-Scoped Teams)
//!
//! Teams now belong to leagues (not seasons) with persistent identity.
//! - `LeagueTeam`: Persistent team entity with `league_id` and `owner_player_id`
//! - `LeagueTeamSeason`: Seasonal participation for a team
//! - `LeagueTeamMember`: Members belong to a `team_season_id` (seasonal roster)
//!
//! Multiple captains are allowed per team (captain is a role, not a field).
//!
//! Note on `UserId` vs `PlayerId`:
//! - `PlayerId` is used for player-related operations (team membership, invitations)
//! - `UserId` is used for admin/audit fields (`created_by`, `added_by`, `invited_by`, `locked_by`)

//! # Roster lock
//!
//! Every path that mutates a seasonal roster enforces the season's roster lock
//! through the **single** entry point in [`roster_lock`] — see that module for
//! why (P-15) and for the P-16/P-18 decisions it encodes.

mod invitation;
mod participant;
mod roster_lock;
mod season;
mod team;

#[cfg(test)]
mod tests;

// Re-export all services for backward compatibility
pub use invitation::LeagueTeamInvitationService;
pub use participant::LeagueSeasonParticipantService;
pub use roster_lock::{RosterChange, RosterLockOverride};
pub use season::LeagueSeasonService;
pub use team::LeagueTeamService;

use crate::entities::league_team::{LeagueSeason, LeagueTeam, LeagueTeamSeason};
use crate::repositories::league::LeagueMemberRepository;
use crate::repositories::league_team::LeagueSeasonRepository;
use portal_core::{DomainError, LeagueId, PlayerId};
use std::sync::Arc;

/// THE "league member before team membership" rule (Discord-design §9.3).
///
/// Stated in the project docs since the Phase-2 join-flow work and enforced
/// nowhere until now: the web UI merely FUNNELLED users through league join
/// before team creation, and every API path — apply, invite-accept, direct
/// add, founding, season re-registration — would happily seat a player who
/// had never joined (or had since left) the league. One rule, one helper;
/// the call sites are every point that seats a player on a roster, plus
/// join-request creation so the applicant hears "join the league first"
/// before applying rather than at acceptance.
pub(crate) async fn ensure_league_member(
    members: &Arc<dyn LeagueMemberRepository>,
    league_id: LeagueId,
    player_id: PlayerId,
) -> Result<(), DomainError> {
    if members.is_member_by_player(league_id, player_id).await? {
        Ok(())
    } else {
        Err(DomainError::NotLeagueMember)
    }
}

/// Refuse to act in a season that has been archived, directly or through its
/// league. The listings hide an archived season by joining its parents; the
/// write paths have to ask the same question or a direct link still works.
pub(crate) async fn ensure_season_live<SR: LeagueSeasonRepository>(
    seasons: &SR,
    season: &LeagueSeason,
) -> Result<(), DomainError> {
    if season.archived_at.is_some() || seasons.is_archived(season.id).await? {
        return Err(DomainError::InvalidState(
            "this season has been archived".to_string(),
        ));
    }
    Ok(())
}

/// THE "can this roster still take a player" rule, for every path that
/// invites, applies or seats someone on it.
///
/// Disbanding, withdrawing and archiving release a roster's seats; without
/// this, an invite or application made before (or after) that could still
/// seat a player on a team that is gone — spending their one primary seat
/// for the season on it.
pub(crate) async fn ensure_roster_live<SR: LeagueSeasonRepository>(
    seasons: &SR,
    team: &LeagueTeam,
    team_season: &LeagueTeamSeason,
    season: &LeagueSeason,
) -> Result<(), DomainError> {
    if team.status.is_terminal() {
        return Err(DomainError::InvalidState(
            "this team has been disbanded".to_string(),
        ));
    }
    if team.archived_at.is_some() {
        return Err(DomainError::InvalidState(
            "this team has been archived".to_string(),
        ));
    }
    if team_season.is_terminal() {
        return Err(DomainError::InvalidState(
            "this team is no longer taking part in the season".to_string(),
        ));
    }
    ensure_season_live(seasons, season).await
}
