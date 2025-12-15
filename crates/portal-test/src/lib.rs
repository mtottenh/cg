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
pub mod fixtures;
pub mod helpers;

/// Common test imports.
pub mod prelude {
    pub use crate::builders::{
        DemoBuilder, DemoMatchLinkBuilder, GameBuilder, LeagueBuilder, LeagueSeasonBuilder,
        LeagueSeasonParticipantBuilder, LeagueTeamBuilder, LeagueTeamInvitationBuilder,
        LeagueTeamMemberBuilder, LeagueTeamSeasonBuilder, PlayerBuilder, TournamentBracketBuilder,
        TournamentBuilder, TournamentMatchBuilder, TournamentRegistrationBuilder,
        TournamentStageBuilder, UserBuilder, VetoDelegateBuilder, VetoSessionBuilder,
        DEFAULT_CS2_MAP_POOL,
    };
    pub use crate::database::TestDb;
    pub use crate::fixtures::{FixtureTokens, TeamFixture, TwoTeamMatchFixture, UserFixture};
    pub use crate::helpers::{
        assign_role_to_user, assign_scoped_role_to_user, create_admin_token, create_test_token,
        get_cs2_game_id, get_dev_player_id, get_dev_user_id, get_game_id, TEST_JWT_SECRET,
    };
    pub use portal_core::*;
}
