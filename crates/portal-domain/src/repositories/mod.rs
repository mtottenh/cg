//! Repository traits for data access.
//!
//! These traits define the interface between domain services and data storage.
//! Implementations are in the `portal-db` crate.

pub mod audit;
pub mod ban;
pub mod league;
pub mod league_team;
pub mod permission;
pub mod tournament;
pub mod user;

pub use audit::{CreateEntityChange, EntityChangeRepository};
pub use ban::{BanRepository, PaginatedBans, PaginationMeta};
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
pub use tournament::{
    CreateTournament, CreateTournamentBracket, CreateTournamentMatch, CreateTournamentMatchGame,
    CreateTournamentRegistration, CreateTournamentStage, CreateTournamentStanding,
    ParticipantSlot, TournamentBracketRepository, TournamentFilters, TournamentMapPoolRepository,
    TournamentMatchGameRepository, TournamentMatchRepository, TournamentRegistrationRepository,
    TournamentRepository, TournamentStageRepository, TournamentStandingsRepository,
    UpdateTournament, UpdateTournamentBracket, UpdateTournamentMatch, UpdateTournamentMatchGame,
    UpdateTournamentRegistration, UpdateTournamentStage, UpdateTournamentStanding,
    UpsertTournamentMapPool,
};
pub use user::{CreatePlayer, CreateUser, PlayerRepository, UpdatePlayer, UserRepository};
