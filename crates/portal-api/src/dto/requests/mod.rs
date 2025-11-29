//! Request DTOs for API endpoints.

pub mod auth;
pub mod game;
pub mod invitation;
pub mod league;
pub mod player;
pub mod team;

pub use auth::{LoginRequest, RegisterRequest};
pub use game::{SetMapPoolRequest, UpdateGameRequest};
pub use invitation::{InvitePlayerRequest, RespondToInvitationRequest};
pub use league::{
    ApplyToLeagueRequest, CreateLeagueRequest, InviteToLeagueRequest, UpdateLeagueMemberRoleRequest,
    UpdateLeagueRequest,
};
pub use player::{SocialLinksRequest, UpdatePlayerProfileRequest};
pub use team::{CreateTeamRequest, UpdateMemberRoleRequest, UpdateTeamRequest};
