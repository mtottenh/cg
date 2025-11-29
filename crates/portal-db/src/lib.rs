//! Database access layer for the Gaming Portal.
//!
//! This crate provides:
//! - Database row types (entities) that map directly to SQL tables
//! - Repository implementations for data access
//! - Adapters that implement domain repository traits
//! - Migrations
//!
//! ## Architecture
//!
//! This crate follows a strict separation between database types and domain types:
//!
//! - **DB Entities** (`entities` module): Flat structs that derive `sqlx::FromRow`,
//!   with nullable fields matching the database schema exactly.
//!
//! - **Adapters** (`adapters` module): Implement domain repository traits,
//!   converting DB entities to domain types via `From` implementations.
//!
//! - **Repositories** (`repositories` module): Lower-level data access that
//!   returns raw DB rows. Used by adapters internally.

pub mod adapters;
pub mod entities;
pub mod error;
pub mod pool;
pub mod repositories;

pub use adapters::{
    PgLeagueInvitationRepository, PgLeagueMemberRepository, PgLeagueRepository,
    PgPermissionRepository, PgPlayerRepository, PgTeamInvitationRepository,
    PgTeamMemberRepository, PgTeamRepository, PgUserRepository,
};
pub use entities::NewUserRole;
pub use error::RepositoryError;
pub use pool::{create_pool, DbPool, PoolConfig};
pub use repositories::{GameRepository, PermissionRepository, PlatformStats, RoleRepository, StatsRepository};

/// Re-export sqlx types for convenience.
pub use sqlx::{PgPool, Postgres};
