//! Domain entities with behavior.
//!
//! These are rich types that encapsulate business rules and invariants.

pub mod audit;
pub mod league;
pub mod league_team;
pub mod player;
pub mod team;
pub mod user;

pub use audit::{ChangeType, EntityChange, EntityChangeId, EntityHistory, FieldChangeSummary};
pub use league::{
    CreateLeagueCommand, League, LeagueAccessType, LeagueInvitation, LeagueInvitationStatus,
    LeagueInvitationType, LeagueMember, LeagueMemberWithUser, LeagueMembershipType, LeagueStatus,
    UpdateLeagueCommand, UserLeagueMembership,
};
pub use league_team::{
    AddLeagueTeamMemberCommand, CreateLeagueSeasonCommand, CreateLeagueTeamCommand,
    CreateLeagueTeamInvitationCommand, LeagueSeason, LeagueTeam, LeagueTeamInvitation,
    LeagueTeamInvitationWithTeam, LeagueTeamMember, LeagueTeamMemberWithUser, LeagueTeamSummary,
    UpdateLeagueSeasonCommand, UpdateLeagueTeamCommand, UserLeagueTeamMembership,
};
pub use player::{Player, SocialLinks};
pub use team::{PlayerTeamMembership, Team, TeamInvitation, TeamMember};
pub use user::{User, UserWithCredentials};
