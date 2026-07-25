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
