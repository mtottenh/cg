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
pub mod league_team;
mod player;
mod rbac;
pub mod tournament;
mod user;

pub use audit::{EntityChangeRow, NewEntityChange};
pub use game::{GameRow, NewGame, UpdateGame};
pub use league::{
    LeagueInvitationRow, LeagueMemberRow, LeagueMemberWithUserRow, LeagueRow, NewLeague,
    NewLeagueInvitation, NewLeagueMember, UpdateLeague, UpdateLeagueInvitation,
    UserLeagueMembershipRow,
};
pub use league_team::{
    LeagueSeasonParticipantRow, LeagueSeasonRow, LeagueTeamInvitationRow,
    LeagueTeamInvitationWithTeamRow, LeagueTeamMemberRow, LeagueTeamMemberWithPlayerRow,
    LeagueTeamRow, LeagueTeamSeasonRow, LeagueTeamSummaryRow, NewLeagueSeason,
    NewLeagueSeasonParticipant, NewLeagueTeam, NewLeagueTeamInvitation, NewLeagueTeamMember,
    NewLeagueTeamSeason, PlayerLeagueTeamMembershipRow, UpdateLeagueSeason,
    UpdateLeagueSeasonParticipant, UpdateLeagueTeam, UpdateLeagueTeamInvitation,
    UpdateLeagueTeamMember, UpdateLeagueTeamSeason,
};
pub use player::{
    NewPlayer, NewPlayerGameProfile, PlayerGameProfileRow, PlayerRow, UpdatePlayer,
    UpdatePlayerRating,
};
pub use rbac::{BanRow, NewBan, NewRole, NewUserRole, PermissionRow, RoleRow, UserRoleRow};
pub use tournament::{
    NewTournament, NewTournamentBracket, NewTournamentMatch, NewTournamentMatchGame,
    NewTournamentRegistration, NewTournamentStage, TournamentBracketRow, TournamentMapPoolRow,
    TournamentMatchGameRow, TournamentMatchRow, TournamentRegistrationRow, TournamentRow,
    TournamentStageRow, TournamentStandingRow,
};
pub use user::{NewUser, UpdateUser, UserRow, UserStatus};
