//! Response DTOs for API endpoints.

pub mod admin;
pub mod auth;
pub mod game;
pub mod invitation;
pub mod league;
pub mod player;
pub mod team;
pub mod user;

pub use admin::PlatformStatsResponse;
pub use auth::{LoginResponse, RegisterResponse};
pub use game::{
    GameDetailResponse, GameSummaryResponse, MapInfoResponse, MapPickBanFormatResponse,
    RankTierResponse, TeamSizeConfig,
};
pub use invitation::{
    InvitationCountResponse, TeamInvitationResponse, TeamInvitationWithTeamResponse,
};
pub use league::{
    LeagueInvitationResponse, LeagueMemberBasicResponse, LeagueMemberResponse, LeagueResponse,
    UserLeagueMembershipResponse,
};
pub use player::{PlayerResponse, PlayerSearchResponse, SocialLinksResponse};
pub use team::{
    PlayerTeamMembershipResponse, TeamMemberResponse, TeamResponse, TeamWithMembersResponse,
};
pub use user::UserResponse;
