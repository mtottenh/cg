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
//! async fn test_team_creation() {
//!     // TestDb automatically starts a PostgreSQL container
//!     let db = TestDb::new().await;
//!
//!     let player = PlayerBuilder::new()
//!         .display_name("TestPlayer")
//!         .build_persisted(&db.pool).await;
//!
//!     let team = TeamBuilder::new("Test Team")
//!         .with_founder(player.id)
//!         .build_persisted(&db.pool).await;
//!
//!     assert_eq!(team.created_by, player.id);
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
    pub use crate::builders::{PlayerBuilder, TeamBuilder, UserBuilder};
    pub use crate::database::TestDb;
    pub use portal_core::*;
}
