//! Repository implementations for data access.
//!
//! Repositories provide async data access methods and return domain types.

mod game;
mod league;
mod rbac;
mod stats;
mod team;
mod user;

pub use game::GameRepository;
pub use league::{
    LeagueInvitationRepository, LeagueMemberRepository, LeagueRepository, PgLeagueInvitationRepository,
    PgLeagueMemberRepository, PgLeagueRepository,
};
pub use rbac::{BanRepository, PermissionRepository, RoleRepository};
pub use stats::{PlatformStats, StatsRepository};
pub use team::{TeamInvitationRepository, TeamMemberRepository, TeamRepository};
pub use user::{PlayerGameProfileRepository, PlayerRepository, UserRepository};
