#![allow(missing_docs)]
//! Business logic and domain services for the Gaming Portal.
//!
//! This crate contains:
//! - Domain entities (rich types with behavior)
//! - Domain services (business logic)
//! - Repository traits (interfaces for data access)
//!
//! ## Architecture
//!
//! Domain types are separate from:
//! - DB types (in `portal-db`) - flat structs matching database rows
//! - API types (in `portal-api`) - DTOs for request/response
//!
//! Mappings between layers use `From` / `TryFrom` implementations.

pub mod auth;
pub mod entities;
pub mod jwt;
pub mod refresh_token;
pub mod repositories;
pub mod services;

// Re-export commonly used types
pub use auth::{hash_password, verify_password};
pub use jwt::{generate_access_token, generate_access_token_with_admin, generate_access_token_with_admin_and_expiry, generate_access_token_with_expiry, validate_token, Claims};
pub use refresh_token::{generate_refresh_token, hash_refresh_token};
pub use entities::league_team::{LeagueSeason, LeagueTeam, LeagueTeamMember};
pub use services::league::LeagueService;
pub use services::league_team::{LeagueSeasonService, LeagueTeamInvitationService, LeagueTeamService};
