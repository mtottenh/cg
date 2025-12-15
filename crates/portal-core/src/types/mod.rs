//! Common type definitions used across the portal.

mod demo;
mod league_team;
mod pagination;
mod permission;
mod rating;
mod status;
mod tournament;

pub use demo::{DemoCategory, DemoLinkType, DemoStatus};
pub use league_team::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamSeasonStatus, LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
pub use pagination::{Page, PageRequest, Pagination};
pub use permission::{ParseScopeTypeError, PermissionScope, ScopeType};
pub use rating::{Glicko2Rating, RatingChange};
pub use status::{EntityStatus, MatchStatus, TournamentStatus};
pub use tournament::{
    AdvancementRule, BracketStatus, BracketType, ExceptionType, MatchFormat, MatchParticipantSource,
    ProposalStatus, RegistrationType, SchedulingMode, SeedingAlgorithm, StageFormat, StageStatus,
    TournamentFormat, TournamentMatchStatus, TournamentParticipantType, TournamentRegistrationStatus,
    WithdrawalPolicy,
};
