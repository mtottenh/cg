//! Common type definitions used across the portal.

mod demo;
pub mod evidence;
mod game_server;
mod league_team;
mod lineup;
mod pagination;
mod permission;
mod status;
mod tournament;
pub mod veto;

#[cfg(test)]
mod wire_compat_tests;

pub use demo::{DemoCategory, DemoLinkType, DemoStatus};
pub use game_server::{
    AgentGamestate, GameServerStatus, ReservationStatus, SubstitutionStatus,
};
pub use evidence::{
    DemoFileMetadata, DiscoveredEvidenceData, EvidenceStorage, EvidenceType,
    EvidenceValidationResult, ExtractedMatchResult, GameMatchResult, MatchEvidenceContext,
    ParticipantEvidenceContext,
};
pub use league_team::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamSeasonStatus, LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
pub use lineup::{LineupSource, LineupStatus, ParticipationStatus, substitutes_are_minority};
pub use pagination::{Page, PageRequest, Pagination};
pub use permission::{ParseScopeTypeError, PermissionScope, ScopeType};
pub use status::{EntityStatus, MatchStatus, TournamentStatus};
pub use tournament::{
    AdvancementRule, BracketStatus, BracketType, ExceptionType, MatchFormat,
    MatchParticipantSource, ProposalStatus, RegistrationType, SchedulingMode, SeedingAlgorithm,
    StageFormat, StageStatus, TournamentFormat, TournamentInvitationStatus, TournamentMatchStatus,
    TournamentParticipantType, TournamentRegistrationStatus, WithdrawalPolicy,
};
pub use veto::{SideSelectionMode, VetoActionType, VetoFormatActionConfig, VetoFormatConfig};
