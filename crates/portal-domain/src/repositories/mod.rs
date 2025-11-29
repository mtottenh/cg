//! Repository traits for data access.
//!
//! These traits define the interface between domain services and data storage.
//! Implementations are in the `portal-db` crate.

pub mod audit;
pub mod league;
pub mod league_team;
pub mod permission;
pub mod team;
pub mod user;

pub use audit::{CreateEntityChange, EntityChangeRepository};
pub use league::{
    AddLeagueMember, CreateLeague, CreateLeagueInvitation, LeagueInvitationRepository,
    LeagueMemberRepository, LeagueRepository, UpdateLeague,
};
pub use league_team::{
    AddLeagueTeamMember, CreateLeagueSeason, CreateLeagueTeam, CreateLeagueTeamInvitation,
    LeagueSeasonRepository, LeagueTeamInvitationRepository, LeagueTeamMemberRepository,
    LeagueTeamRepository, UpdateLeagueSeason, UpdateLeagueTeam,
};
pub use permission::PermissionRepository;
pub use team::{
    AddMember, CreateInvitation, CreateTeam, TeamInvitationRepository, TeamMemberRepository,
    TeamRepository, UpdateTeam,
};
pub use user::{CreatePlayer, CreateUser, PlayerRepository, UpdatePlayer, UserRepository};
