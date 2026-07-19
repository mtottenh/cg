//! Repository implementations for data access.
//!
//! Repositories provide async data access methods and return domain types.

mod action_item;
mod game;
mod league;
mod rbac;
mod stats;
mod user;

pub use action_item::{ActionItem, ActionItemRepository};
pub use game::GameRepository;
pub use league::{
    LeagueInvitationRepository, LeagueMemberRepository, LeagueRepository,
    PgLeagueInvitationRepository, PgLeagueMemberRepository, PgLeagueRepository,
};
pub use rbac::{BanRepository, PermissionRepository, RoleRepository};
pub use stats::{PlatformStats, StatsRepository};
pub use user::{PlayerGameProfileRepository, PlayerRepository, UserRepository};
