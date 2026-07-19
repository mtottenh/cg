#![allow(missing_docs)]
//! Database access layer for the Gaming Portal.
//!
//! This crate provides:
//! - Database row types (entities) that map directly to SQL tables
//! - Repository implementations for data access
//! - Adapters that implement domain repository traits
//! - Transaction support for atomic operations
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
//!
//! - **Transaction** (`transaction` module): Transaction support for executing
//!   multiple operations atomically.

pub mod adapters;
pub mod entities;
pub mod error;
pub mod pool;
pub mod repositories;
pub mod transaction;

pub use adapters::{
    MatchCompletionTxInput, MatchCompletionTxOutput, PgApiKeyRepository,
    PgAvailabilityOverrideRepository, PgAvailabilityWindowRepository, PgBanRepository,
    PgDemoMatchLinkRepository, PgDemoPlayerRepository, PgDemoRepository,
    PgDiscoveredMatchRepository, PgDisputeMessageRepository, PgDisputeRepository,
    PgEntityChangeRepository, PgEvidenceRepository, PgForfeitRecordRepository,
    PgLeagueInvitationRepository, PgLeagueMemberRepository, PgLeagueRepository,
    PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository,
    PgLeagueTeamInvitationRepository, PgLeagueTeamMemberRepository, PgLeagueTeamRepository,
    PgLeagueTeamSeasonRepository, PgMatchStatusLogRepository, PgPermissionRepository,
    PgPlayerGameProfileRepository, PgPlayerMatchHistoryRepository, PgPlayerMmStatsRepository,
    PgPlayerRatingHistoryRepository, PgPlayerRepository, PgProgressionLogRepository,
    PgRefreshTokenRepository, PgResultClaimRepository, PgResultReviewRepository,
    PgSagaExecutionRepository, PgScheduleProposalRepository, PgSteamTrackingRepository,
    PgSuggestedTimeRepository, PgTournamentBracketRepository, PgTournamentMapPoolRepository,
    PgTournamentMatchGameRepository, PgTournamentMatchRepository,
    PgTournamentRegistrationRepository, PgTournamentRepository, PgTournamentStageRepository,
    PgTournamentStandingsRepository, PgUserRepository, PgVetoActionRepository,
    PgVetoDelegateRepository, PgVetoLobbyMessageRepository, PgVetoSessionRepository,
    complete_match_in_transaction,
};
pub use entities::NewUserRole;
pub use error::RepositoryError;
pub use pool::{DbPool, PoolConfig, create_pool};
pub use repositories::{
    ActionItem, ActionItemRepository, GameRepository, PermissionRepository, PlatformStats,
    RoleRepository, StatsRepository,
};
pub use transaction::{DbTransaction, Transactional, begin_transaction, with_transaction};

/// Re-export sqlx types for convenience.
pub use sqlx::{PgPool, Postgres};
