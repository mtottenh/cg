//! Domain services containing business logic.

pub mod audit;
pub mod ban;
pub mod demo;
pub mod eligibility;
pub mod eligibility_service;
pub mod discovered_match;
pub mod league;
pub mod league_team;
pub mod permission;
pub mod player;
pub mod player_game_profile;
pub mod steam_tracking;
pub mod tournament;
pub mod user;

pub use audit::{AuditService, ChangeContext, ChangeDetector, FieldChange};
pub use ban::BanService;
pub use demo::{CatalogResult, DemoPlayerInput, DemoService};
pub use discovered_match::DiscoveredMatchService;
pub use league::LeagueService;
pub use league_team::{
    LeagueSeasonParticipantService, LeagueSeasonService, LeagueTeamInvitationService,
    LeagueTeamService,
};
pub use permission::PermissionService;
pub use player::{PlayerSearchResult, PlayerService};
pub use steam_tracking::SteamTrackingService;
pub use player_game_profile::PlayerGameProfileService;
pub use eligibility_service::EligibilityService;
pub use tournament::{BracketGenerator, GeneratedBracket, TournamentService};
pub use user::{AuthResult, LoginCommand, RegisterUserCommand, UserService};
