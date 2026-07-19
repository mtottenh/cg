//! Request DTOs for API endpoints.

pub mod auth;
pub mod availability;
pub mod award;
pub mod ban;
pub mod demo;
pub mod dispute;
pub mod evidence;
pub mod forfeit;
pub mod game;
pub mod league;
pub mod league_team;
pub mod player;
pub mod player_game_profile;
pub mod progression;
pub mod result;
pub mod result_review;
pub mod role;
pub mod tournament;
pub mod veto;
pub mod veto_delegate;

pub use auth::{LoginRequest, RefreshTokenRequest, RegisterRequest};
pub use availability::{
    CreateAvailabilityOverrideRequest, CreateAvailabilityWindowRequest, GenerateSuggestionsRequest,
    GetAvailabilityQuery, UpdateAvailabilityWindowRequest,
};
pub use award::{
    CreateAwardRequest, LeaderboardQueryParams, StandingsQueryParams, UpdateAwardRequest,
};
pub use ban::{CreateBanRequest, LiftBanRequest, ListBansQuery};
pub use demo::{
    AssociateDemoRequest, BatchCatalogDemoEntry, BatchCatalogDemosRequest, CatalogDemoRequest,
    CategorizeDemoRequest, DemoPlayerInputDto, GetDemosForMatchQuery, LinkDemoToMatchRequest,
    ListDemosQuery, MarkDemoFailedRequest, PendingDemosQuery, ProcessUnlinkedDemosQuery,
    SetDemoNotesRequest, SetDemoVisibilityRequest, SubmitDemoStatsRequest,
    UnlinkDemoFromMatchRequest, UpdateAutoLinkSettingRequest,
};
pub use dispute::{
    AddDisputeMessageRequest, AdminDisputeMessageRequest, AssignDisputeRequest, ListDisputesQuery,
    RaiseDisputeRequest, ResolveAdjustedRequest, ResolveDoubleDqRequest, ResolveOverturnRequest,
    ResolveRematchRequest, ResolveUpholdRequest,
};
pub use evidence::{
    AddLinkEvidenceRequest, DiscoverEvidenceQuery, GetDemoStatsQuery, InitiateUploadRequest,
    LinkDemoRequest, LinkDiscoveredEvidenceRequest, ListEvidenceQuery, ValidateDemoRequest,
    ValidateEvidenceRequest,
};
pub use forfeit::{
    AdminDisqualifyRequest, AdminDoubleForfeitRequest, AdminForfeitMatchRequest,
    WithdrawFromTournamentRequest,
};
pub use game::{
    AddMapRequest, RankTierInput, SetMapPoolRequest, SetRankTiersRequest, UpdateGameRequest,
    UpdateMapRequest, UpdateTeamSizeRequest,
};
pub use league::{
    ApplyToLeagueRequest, CreateLeagueRequest, InviteToLeagueRequest,
    UpdateLeagueMemberRoleRequest, UpdateLeagueRequest,
};
pub use league_team::{
    AddLeagueTeamMemberRequest, ApplyToLeagueTeamRequest, CreateLeagueSeasonRequest,
    CreateLeagueTeamRequest, InviteToLeagueTeamRequest, RegisterParticipantRequest,
    RegisterTeamForSeasonRequest, RespondToInvitationRequest, TransferOwnershipRequest,
    UpdateLeagueSeasonRequest, UpdateLeagueTeamMemberRequest, UpdateLeagueTeamRequest,
    WithdrawParticipantRequest,
};
pub use player::{SocialLinksRequest, UpdatePlayerProfileRequest};
pub use player_game_profile::SubmitRatingRequest;
pub use progression::{ProcessProgressionRequest, ReapplyProgressionRequest};
pub use result::{
    AdminResolveResultRequest, CancelResultClaimRequest, ConfirmResultClaimRequest,
    DisputeResultClaimRequest, GameResultInput, ListResultClaimsQuery, SubmitResultClaimRequest,
};
pub use result_review::AdminReviewDecisionRequest;
pub use role::{
    AddPermissionToRoleRequest, AssignRoleRequest, CreateRoleRequest, RevokeRoleRequest,
    UpdateRoleRequest,
};
pub use tournament::{
    AcceptScheduleProposalRequest, AdminMatchTransitionRequest, AdminScheduleRequest,
    AutoSeedRequest, CheckInRequest, CounterProposeRequest, CreateTournamentRequest,
    CreateTournamentStageRequest, DisputeMatchRequest, DisqualifyRequest,
    EligibilityRestrictionsInput, ForfeitMatchRequest, ListTournamentsQuery, ManualSeedRequest,
    MatchCheckInRequest, ProposeScheduleRequest, RegisterPlayerRequest, RegisterTeamRequest,
    RejectRegistrationRequest, RejectScheduleProposalRequest, ResolveDisputeRequest,
    ScheduleMatchRequest, SeedAssignment, SetTournamentMapPoolRequest, SubmitMatchResultRequest,
    UpdateTournamentRequest, WithdrawRequest,
};
pub use veto::{
    CreateVetoSessionRequest, GetVetoStateQuery, PerformVetoActionRequest, RecordCoinFlipRequest,
    SelectSideRequest, StartVetoSessionRequest,
};
pub use veto_delegate::CreateVetoDelegateRequest;
