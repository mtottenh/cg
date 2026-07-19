//! Response DTOs for API endpoints.

pub mod action_item;
pub mod admin;
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
pub mod user;
pub mod veto;
pub mod veto_delegate;

pub use action_item::ActionItemResponse;
pub use admin::PlatformStatsResponse;
pub use auth::{LoginResponse, RegisterResponse};
pub use availability::{
    AvailabilityOverrideResponse, AvailabilityWindowResponse, DateAvailabilityResponse,
    SuggestedTimeResponse, TimeSlotResponse,
};
pub use award::{
    AwardResponse, AwardResultResponse, AwardStandingsResponse, AwardTemplateResponse,
    FinalizedAwardResponse, LeaderboardEntryResponse, PlayerTrophyResponse,
    StatCatalogEntryResponse,
};
pub use ban::{BanListResponse, BanResponse, PaginationMetaResponse};
pub use demo::{
    AutoLinkSettingResponse, BatchCatalogErrorResponse, BatchCatalogResultResponse,
    DemoDownloadResponse, DemoIdListResponse, DemoListResponse, DemoMatchLinkResponse,
    DemoMatchLinkWithDemoResponse, DemoMetadataResponse, DemoPlayerResponse, DemoResponse,
    DemoStatusCountsResponse, DemoValidationResultResponse, DemoWithPlayersResponse,
    ProcessUnlinkedDemosResponse,
};
pub use dispute::{
    DisputeListResponse, DisputeMessageResponse, DisputeResolutionResponse,
    DisputeResolutionResultResponse, DisputeResponse, DisputeWithThreadResponse,
};
pub use evidence::{
    AccessUrlResponse, DemoPlayerStatsResponse, DemoStatsResponse, DemoValidationResponse,
    DiscoveredEvidenceResponse, EvidenceResponse, EvidenceSummaryResponse, ExtractedResultResponse,
    UploadInfoResponse, ValidationResultResponse,
};
pub use forfeit::{
    DisqualificationResponse, ForfeitRecordResponse, ForfeitResponse, WithdrawalResponse,
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
pub use player_game_profile::{
    DisplayStatResponse, MatchHistoryEntryResponse, PlayerGameProfileResponse,
    PlayerRatingHistoryResponse, PublicMmStatsResponse,
};
pub use progression::{
    AdvancementResponse, LoserResultResponse, ProgressionLogResponse, ProgressionResponse,
};
pub use result::{
    GameResultResponse, ResultClaimResponse, ResultClaimSubmissionResponse,
    ResultConfirmationResponse, ResultDisputeResponse,
};
pub use result_review::{
    AcknowledgmentResponse, ResultReviewListResponse, ResultReviewResponse,
    ResultReviewSummaryResponse, UnrecognizedPlayerResponse,
};
pub use role::{
    PermissionResponse, RoleResponse, RoleWithPermissionsResponse, UserRoleAssignmentResponse,
};
pub use tournament::{
    CheckInStatusResponse, MatchStatusDetailsResponse, MatchStatusLogResponse,
    ScheduleProposalResponse, SeededParticipantResponse, TournamentBracketResponse,
    TournamentMapPoolResponse, TournamentMatchGameResponse, TournamentMatchResponse,
    TournamentRegistrationResponse, TournamentResponse, TournamentStageResponse,
    TournamentStandingResponse, TournamentSummaryResponse,
};
pub use user::UserResponse;
pub use veto::{
    MapStatusResponse, VetoActionResponse, VetoActionResultResponse, VetoFormatActionResponse,
    VetoFormatResponse, VetoSessionResponse, VetoSessionStateResponse,
};
pub use veto_delegate::{VetoDelegateListResponse, VetoDelegateResponse};
