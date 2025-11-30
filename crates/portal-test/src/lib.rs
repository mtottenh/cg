#![allow(missing_docs)]
//! Test utilities for the Gaming Portal.
//!
//! This crate provides:
//! - Test database management with isolated databases (via testcontainers)
//! - Builder patterns for creating test entities
//! - Common test fixtures and utilities
//!
//! ## Usage
//!
//! ```ignore
//! use portal_test::prelude::*;
//!
//! #[tokio::test]
//! async fn test_player_creation() {
//!     // TestDb automatically starts a PostgreSQL container
//!     let db = TestDb::new().await;
//!
//!     let player = PlayerBuilder::new()
//!         .display_name("TestPlayer")
//!         .build_persisted(&db.pool).await;
//!
//!     assert!(!player.display_name.is_empty());
//! }
//! ```
//!
//! ## Requirements
//!
//! - Docker must be installed and running
//! - First test run pulls the `postgres:16-alpine` image

pub mod builders;
mod container; // Internal: shared PostgreSQL container management
pub mod database;

/// Common test imports.
pub mod prelude {
    pub use crate::builders::{
        LeagueBuilder, LeagueSeasonBuilder, LeagueSeasonParticipantBuilder, LeagueTeamBuilder,
        LeagueTeamInvitationBuilder, LeagueTeamMemberBuilder, LeagueTeamSeasonBuilder,
        PlayerBuilder, UserBuilder,
    };
    pub use crate::database::TestDb;
    pub use portal_core::*;
}
