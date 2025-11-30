//! Response DTOs for API endpoints.

pub mod admin;
pub mod auth;
pub mod ban;
pub mod game;
pub mod league;
pub mod league_team;
pub mod player;
pub mod tournament;
pub mod user;

pub use admin::PlatformStatsResponse;
pub use ban::{BanListResponse, BanResponse, PaginationMetaResponse};
pub use auth::{LoginResponse, RegisterResponse};
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
pub use tournament::{
    TournamentBracketResponse, TournamentMatchGameResponse, TournamentMatchResponse,
    TournamentRegistrationResponse, TournamentResponse, TournamentStageResponse,
    TournamentStandingResponse, TournamentSummaryResponse,
};
pub use user::UserResponse;
