//! Request DTOs for API endpoints.

pub mod auth;
pub mod ban;
pub mod game;
pub mod league;
pub mod league_team;
pub mod player;
pub mod tournament;

pub use auth::{LoginRequest, RegisterRequest};
pub use ban::{CreateBanRequest, LiftBanRequest, ListBansQuery};
pub use game::{SetMapPoolRequest, UpdateGameRequest};
pub use league::{
    ApplyToLeagueRequest, CreateLeagueRequest, InviteToLeagueRequest, UpdateLeagueMemberRoleRequest,
    UpdateLeagueRequest,
};
pub use league_team::{
    AddLeagueTeamMemberRequest, ApplyToLeagueTeamRequest, CreateLeagueSeasonRequest,
    CreateLeagueTeamRequest, InviteToLeagueTeamRequest, RegisterParticipantRequest,
    RegisterTeamForSeasonRequest, RespondToInvitationRequest, TransferOwnershipRequest,
    UpdateLeagueSeasonRequest, UpdateLeagueTeamMemberRequest, UpdateLeagueTeamRequest,
    WithdrawParticipantRequest,
};
pub use player::{SocialLinksRequest, UpdatePlayerProfileRequest};
pub use tournament::{
    CheckInRequest, CreateTournamentRequest, CreateTournamentStageRequest, DisputeMatchRequest,
    ListTournamentsQuery, RegisterPlayerRequest, RegisterTeamRequest, ResolveDisputeRequest,
    ScheduleMatchRequest, SubmitMatchResultRequest, UpdateTournamentRequest, WithdrawRequest,
};
