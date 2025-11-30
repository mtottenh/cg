//! Core types, IDs, and errors for the Gaming Portal.
//!
//! This crate provides the foundational types used across all other crates:
//! - Strongly-typed IDs using the newtype pattern
//! - Domain and validation errors
//! - Common type definitions (status enums, team roles, etc.)
//! - Permission constants for RBAC
//!
//! This crate has no async dependencies and can be used anywhere.

pub mod errors;
pub mod ids;
pub mod permissions;
pub mod types;
pub mod validation;

// Re-export commonly used types at crate root
pub use errors::{DomainError, FieldError, ValidationError};
pub use ids::{
    BanId, GameId, GameSlug, LeagueId, LeagueInvitationId, LeagueMemberId, LeagueSeasonId,
    LeagueTeamId, LeagueTeamInvitationId, LeagueTeamMemberId, LeagueTeamSeasonId, LobbyId, MatchId,
    PlayerId, TournamentBracketId, TournamentId, TournamentMapPoolId, TournamentMatchGameId,
    TournamentMatchId, TournamentRegistrationId, TournamentStageId, UserId,
};
pub use types::{ParseScopeTypeError, PermissionScope, ScopeType};
