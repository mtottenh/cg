//! Domain entities with behavior.
//!
//! These are rich types that encapsulate business rules and invariants.

pub mod api_key;
pub mod audit;
pub mod availability;
pub mod ban;
pub mod demo;
pub mod eligibility;
pub mod demo_validation;
pub mod discovered_match;
pub mod dispute;
pub mod evidence;
pub mod forfeit;
pub mod league;
pub mod league_team;
pub mod match_lifecycle;
pub mod player;
pub mod player_game_profile;
pub mod refresh_token;
pub mod player_rating_history;
pub mod result_claim;
pub mod result_review;
pub mod schedule_proposal;
pub mod saga;
pub mod steam_tracking;
pub mod tournament;
pub mod user;
pub mod veto;
pub mod veto_delegate;
pub mod veto_lobby_message;

pub use api_key::ApiKey;
pub use audit::{ChangeType, EntityChange, EntityChangeId, EntityHistory, FieldChangeSummary};
pub use availability::{
    AvailabilityOverride, AvailabilityWindow, CreateAvailabilityOverride, CreateAvailabilityWindow,
    CreateSuggestedTime, DateAvailability, OverrideType, SuggestedTime, SuggestionStatus, TimeSlot,
    UpdateAvailabilityWindow,
};
pub use ban::{Ban, BanFilters, BanType, CreateBanCommand, LiftBanCommand};
pub use demo::{
    AssociateDemoCommand, CategorizeDemoCommand, CreateDemoCommand, CreateDemoPlayerCommand,
    Demo, DemoFilter, DemoListResult, DemoMatchLink, DemoPlayer, DemoPlayerStats,
    LinkDemoToMatchCommand, ParsedDemoMetadata, SetDemoVisibilityCommand,
    UnlinkDemoFromMatchCommand, UpdateDemoStatsCommand,
};
pub use discovered_match::DiscoveredMatch;
pub use eligibility::{EligibilityRestrictions, EligibilityViolation};
pub use demo_validation::{
    DemoValidationEntry, DemoValidationResult, MatchDemoValidation, TeamSide, UnrecognizedPlayer,
};
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
pub use match_lifecycle::{
    CreateMatchStatusLogCommand, MatchStatusLog, TransitionTrigger,
};
pub use player::{Player, SocialLinks};
pub use player_game_profile::PlayerGameProfile;
pub use player_rating_history::PlayerRatingHistory;
pub use schedule_proposal::{
    AcceptProposalCommand, CounterProposeCommand, CreateScheduleProposalCommand,
    RejectProposalCommand, ScheduleProposal,
};
pub use tournament::{
    ByeAssignment, CreateTournamentBracketCommand, CreateTournamentCommand,
    CreateTournamentStageCommand, GameStatus, GeneratedMatch, HeadToHead, HeadToHeadRecord,
    RegisterPlayerCommand, RegisterTeamCommand, ScheduleMatchCommand, SeededParticipant,
    SubmitGameResultCommand, SubmitMatchResultCommand, Tournament, TournamentBracket,
    TournamentMapPool, TournamentMatch, TournamentMatchGame, TournamentRegistration,
    TournamentStage, TournamentStanding, UpdateTournamentCommand,
};
pub use user::{User, UserWithCredentials};
pub use veto::{
    CreateVetoSessionCommand, MapStatus, MapVetoStatus, PerformVetoActionCommand,
    RecordCoinFlipCommand, SelectSideCommand, VetoAction, VetoActionResult, VetoActionType,
    VetoFormat, VetoFormatAction, VetoSession, VetoSessionState, VetoStatus,
};
pub use result_claim::{
    CancelResultClaimCommand, ClaimStatus, ConfirmResultClaimCommand, CreateResultClaimCommand,
    DisputeResultClaimCommand, GameResult, GameResultInput, ResultClaim, ResultValidationError,
};
pub use evidence::{
    AddLinkEvidenceCommand, DemoMetadata, DiscoveredEvidence, Evidence, EvidenceAccessLog,
    EvidenceAccessType, EvidenceAccessUrl, EvidenceSource, EvidenceStatus, EvidenceStorage,
    EvidenceType, EvidenceUploadInfo, EvidenceValidation, ExtractedResult,
    InitiateEvidenceUploadCommand, LinkDiscoveredEvidenceCommand, MatchEvidenceContext,
    ParticipantContext,
};
pub use saga::{
    SagaContext, SagaExecution, SagaStatus, StepRecord, StepStatus,
};
pub use forfeit::{
    DisqualifyCommand, ForfeitRecord, ForfeitResult, ForfeitTrigger, ForfeitType,
    ProcessForfeitCommand, WithdrawFromTournamentCommand,
};
pub use dispute::{
    AddDisputeMessageCommand, AssignDisputeCommand, AuthorType, Dispute, DisputeMessage,
    DisputePriority, DisputeReason, DisputeResolution, DisputeResolutionResult,
    DisputeStatus, DisputeWithThread, ProgressionChanges, RaiseDisputeCommand,
    ResolveDisputeCommand, ResolutionType,
};
pub use result_review::{ResultReview, ResultReviewStatus};
pub use steam_tracking::{CreateSteamTrackingCommand, SteamTracking, UpdatePollResultCommand};
pub use veto_delegate::{
    CreateVetoDelegateCommand, DelegatedByRole, RevokeVetoDelegateCommand, VetoDelegate,
};
pub use veto_lobby_message::{VetoLobbyMessage, VetoMessageType};
pub use refresh_token::RefreshToken;
