//! Domain entities with behavior.
//!
//! These are rich types that encapsulate business rules and invariants.

pub mod audit;
pub mod ban;
pub mod league;
pub mod league_team;
pub mod player;
pub mod tournament;
pub mod user;

pub use audit::{ChangeType, EntityChange, EntityChangeId, EntityHistory, FieldChangeSummary};
pub use ban::{Ban, BanFilters, BanType, CreateBanCommand, LiftBanCommand};
pub use league::{
    CreateLeagueCommand, League, LeagueAccessType, LeagueInvitation, LeagueInvitationStatus,
    LeagueInvitationType, LeagueMember, LeagueMemberWithUser, LeagueMembershipType, LeagueStatus,
    UpdateLeagueCommand, UserLeagueMembership,
};
pub use league_team::{
    AddLeagueTeamMemberCommand, CreateLeagueSeasonCommand, CreateLeagueTeamCommand,
    CreateLeagueTeamInvitationCommand, LeagueSeason, LeagueTeam, LeagueTeamInvitation,
    LeagueTeamInvitationWithTeam, LeagueTeamMember, LeagueTeamMemberWithPlayer, LeagueTeamSummary,
    PlayerLeagueTeamMembership, UpdateLeagueSeasonCommand, UpdateLeagueTeamCommand,
};
pub use player::{Player, SocialLinks};
pub use tournament::{
    ByeAssignment, CreateTournamentBracketCommand, CreateTournamentCommand,
    CreateTournamentStageCommand, GameStatus, GeneratedMatch, RegisterPlayerCommand,
    RegisterTeamCommand, ScheduleMatchCommand, SeededParticipant, SubmitGameResultCommand,
    SubmitMatchResultCommand, Tournament, TournamentBracket, TournamentMapPool, TournamentMatch,
    TournamentMatchGame, TournamentRegistration, TournamentStage, TournamentStanding,
    UpdateTournamentCommand,
};
pub use user::{User, UserWithCredentials};
