//! Common type definitions used across the portal.

mod league_team;
mod pagination;
mod permission;
mod rating;
mod status;
mod tournament;

pub use league_team::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamSeasonStatus, LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
pub use pagination::{Page, PageRequest, Pagination};
pub use permission::{ParseScopeTypeError, PermissionScope, ScopeType};
pub use rating::{Glicko2Rating, RatingChange};
pub use status::{EntityStatus, MatchStatus, TournamentStatus};
pub use tournament::{
    AdvancementRule, BracketStatus, BracketType, MatchFormat, MatchParticipantSource,
    RegistrationType, SchedulingMode, SeedingAlgorithm, StageFormat, StageStatus,
    TournamentFormat, TournamentMatchStatus, TournamentParticipantType, TournamentRegistrationStatus,
    WithdrawalPolicy,
};
