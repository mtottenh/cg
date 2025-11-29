//! Database entity types (row structs).
//!
//! These types map directly to database tables via `sqlx::FromRow`.
//! They are intentionally flat and use nullable types where the schema allows.
//!
//! Domain types are in `portal-domain`; mappings from DB types to domain types
//! are implemented alongside each entity.

mod audit;
mod game;
mod league;
mod league_team;
mod player;
mod rbac;
mod team;
mod user;

pub use audit::{EntityChangeRow, NewEntityChange};
pub use game::{GameRow, NewGame, UpdateGame};
pub use league::{
    LeagueInvitationRow, LeagueMemberRow, LeagueMemberWithUserRow, LeagueRow, NewLeague,
    NewLeagueInvitation, NewLeagueMember, UpdateLeague, UpdateLeagueInvitation,
    UserLeagueMembershipRow,
};
pub use league_team::{
    LeagueSeasonRow, LeagueTeamInvitationRow, LeagueTeamInvitationWithTeamRow, LeagueTeamMemberRow,
    LeagueTeamMemberWithUserRow, LeagueTeamRow, LeagueTeamSummaryRow, NewLeagueSeason,
    NewLeagueTeam, NewLeagueTeamInvitation, NewLeagueTeamMember, UpdateLeagueSeason,
    UpdateLeagueTeam, UpdateLeagueTeamInvitation, UpdateLeagueTeamMember,
    UserLeagueTeamMembershipRow,
};
pub use player::{
    NewPlayer, NewPlayerGameProfile, PlayerGameProfileRow, PlayerRow, UpdatePlayer,
    UpdatePlayerRating,
};
pub use rbac::{BanRow, NewBan, NewRole, NewUserRole, PermissionRow, RoleRow, UserRoleRow};
pub use team::{
    NewTeam, NewTeamInvitation, NewTeamMember, PlayerTeamMembershipRow, TeamInvitationRow,
    TeamMemberRow, TeamRow, UpdateTeam, UpdateTeamInvitation, UpdateTeamMember,
};
pub use user::{NewUser, UpdateUser, UserRow, UserStatus};
