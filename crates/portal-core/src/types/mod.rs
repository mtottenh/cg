//! Common type definitions used across the portal.

mod demo;
pub mod evidence;
mod league_team;
mod pagination;
mod permission;
mod rating;
mod status;
mod tournament;
pub mod veto;

pub use demo::{DemoCategory, DemoLinkType, DemoStatus};
pub use evidence::{
    DemoFileMetadata, DiscoveredEvidenceData, EvidenceStorage, EvidenceType,
    EvidenceValidationResult, ExtractedMatchResult, GameMatchResult, MatchEvidenceContext,
    ParticipantEvidenceContext,
};
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
pub use veto::{SideSelectionMode, VetoActionType, VetoFormatActionConfig, VetoFormatConfig};
