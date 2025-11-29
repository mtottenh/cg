//! Domain services containing business logic.

pub mod audit;
pub mod league;
pub mod league_team;
pub mod permission;
pub mod player;
pub mod team;
pub mod team_invitation;
pub mod user;

pub use audit::{AuditService, ChangeContext, ChangeDetector, FieldChange};
pub use league::LeagueService;
pub use league_team::{LeagueSeasonService, LeagueTeamInvitationService, LeagueTeamService};
pub use permission::PermissionService;
pub use player::{PlayerSearchResult, PlayerService};
pub use team::TeamService;
pub use team_invitation::TeamInvitationService;
pub use user::{AuthResult, LoginCommand, RegisterUserCommand, UserService};
