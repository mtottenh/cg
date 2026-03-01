//! Response DTOs for API endpoints.

pub mod admin;
pub mod auth;
pub mod availability;
pub mod ban;
pub mod demo;
pub mod dispute;
pub mod evidence;
pub mod forfeit;
pub mod game;
pub mod league;
pub mod league_team;
pub mod player;
pub mod progression;
pub mod result;
pub mod result_review;
pub mod role;
pub mod tournament;
pub mod user;
pub mod veto;
pub mod veto_delegate;

pub use admin::PlatformStatsResponse;
pub use availability::{
    AvailabilityOverrideResponse, AvailabilityWindowResponse, DateAvailabilityResponse,
    SuggestedTimeResponse, TimeSlotResponse,
};
pub use ban::{BanListResponse, BanResponse, PaginationMetaResponse};
pub use auth::{LoginResponse, RegisterResponse};
pub use demo::{
    BatchCatalogErrorResponse, BatchCatalogResultResponse, DemoIdListResponse, DemoListResponse,
    DemoMatchLinkResponse, DemoMatchLinkWithDemoResponse, DemoMetadataResponse, DemoPlayerResponse,
    DemoResponse, DemoStatusCountsResponse, DemoValidationResultResponse, DemoWithPlayersResponse,
};
pub use evidence::{
    AccessUrlResponse, DemoPlayerStatsResponse, DemoStatsResponse, DemoValidationResponse,
    DiscoveredEvidenceResponse, EvidenceResponse, EvidenceSummaryResponse, ExtractedResultResponse,
    UploadInfoResponse, ValidationResultResponse,
};
pub use game::{
    GameDetailResponse, GameSummaryResponse, MapInfoResponse, MapPickBanFormatResponse,
    RankTierResponse, TeamSizeConfig,
};
pub use league::{
    LeagueInvitationResponse, LeagueMemberBasicResponse, LeagueMemberResponse, LeagueResponse,
    UserLeagueMembershipResponse,
};
pub use league_team::{
    LeagueSeasonParticipantResponse, LeagueSeasonResponse, LeagueTeamInvitationResponse,
    LeagueTeamInvitationWithTeamResponse, LeagueTeamMemberResponse,
    LeagueTeamMemberWithPlayerResponse, LeagueTeamResponse, LeagueTeamSeasonResponse,
    LeagueTeamSummaryResponse, LeagueTeamWithSeasonResponse, PlayerLeagueTeamMembershipResponse,
};
pub use player::{PlayerResponse, PlayerSearchResponse, SocialLinksResponse};
pub use progression::{
    AdvancementResponse, LoserResultResponse, ProgressionLogResponse, ProgressionResponse,
};
pub use result::{
    GameResultResponse, ResultClaimResponse, ResultClaimSubmissionResponse,
    ResultConfirmationResponse, ResultDisputeResponse,
};
pub use role::{PermissionResponse, RoleResponse, RoleWithPermissionsResponse, UserRoleAssignmentResponse};
pub use tournament::{
    CheckInStatusResponse, MatchStatusDetailsResponse, MatchStatusLogResponse,
    ScheduleProposalResponse, SeededParticipantResponse, TournamentBracketResponse,
    TournamentMatchGameResponse, TournamentMatchResponse, TournamentRegistrationResponse,
    TournamentResponse, TournamentStageResponse, TournamentStandingResponse,
    TournamentSummaryResponse,
};
pub use user::UserResponse;
pub use veto::{
    MapStatusResponse, VetoActionResponse, VetoActionResultResponse, VetoFormatActionResponse,
    VetoFormatResponse, VetoSessionResponse, VetoSessionStateResponse,
};
pub use forfeit::{
    DisqualificationResponse, ForfeitRecordResponse, ForfeitResponse, WithdrawalResponse,
};
pub use dispute::{
    DisputeListResponse, DisputeMessageResponse, DisputeResolutionResponse,
    DisputeResolutionResultResponse, DisputeResponse, DisputeWithThreadResponse,
};
pub use result_review::{
    AcknowledgmentResponse, ResultReviewListResponse, ResultReviewResponse,
    ResultReviewSummaryResponse, UnrecognizedPlayerResponse,
};
pub use veto_delegate::{VetoDelegateListResponse, VetoDelegateResponse};
