//! Adapters that implement domain repository traits.
//!
//! These adapters bridge the gap between the database layer and the domain layer
//! by implementing domain traits and converting between db row types and domain entities.

mod audit;
mod league;
mod permission;
mod team;
mod user;

pub use audit::PgEntityChangeRepository;
pub use league::{PgLeagueInvitationRepository, PgLeagueMemberRepository, PgLeagueRepository};
pub use permission::PgPermissionRepository;
pub use team::{PgTeamInvitationRepository, PgTeamMemberRepository, PgTeamRepository};
pub use user::{PgPlayerRepository, PgUserRepository};
