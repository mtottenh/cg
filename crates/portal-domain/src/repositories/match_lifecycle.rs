//! Match lifecycle repository traits.
//!
//! These repositories handle match status log persistence.

use crate::entities::MatchStatusLog;
use async_trait::async_trait;
use portal_core::types::TournamentMatchStatus;
use portal_core::{DomainError, MatchStatusLogId, TournamentMatchId, UserId};

// =============================================================================
// MATCH STATUS LOG REPOSITORY
// =============================================================================

/// Repository trait for match status log operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MatchStatusLogRepository: Send + Sync + 'static {
    /// Create a new status log entry.
    async fn create(&self, log: CreateMatchStatusLog) -> Result<MatchStatusLog, DomainError>;

    /// Find a log entry by ID.
    async fn find_by_id(&self, id: MatchStatusLogId)
    -> Result<Option<MatchStatusLog>, DomainError>;

    /// Find all log entries for a match, ordered by transition time (oldest first).
    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchStatusLog>, DomainError>;

    /// Find the most recent log entry for a match.
    async fn find_latest_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<MatchStatusLog>, DomainError>;

    /// Count transitions for a match.
    async fn count_by_match_id(&self, match_id: TournamentMatchId) -> Result<i64, DomainError>;
}

/// Data for creating a match status log entry.
#[derive(Debug, Clone)]
pub struct CreateMatchStatusLog {
    /// The match that transitioned.
    pub match_id: TournamentMatchId,
    /// Status before the transition.
    pub from_status: TournamentMatchStatus,
    /// Status after the transition.
    pub to_status: TournamentMatchStatus,
    /// Human-readable reason for the transition.
    pub transition_reason: Option<String>,
    /// User who triggered the transition (if not system).
    pub triggered_by_user_id: Option<UserId>,
    /// Whether the transition was triggered by a background job.
    pub triggered_by_system: bool,
    /// Additional context (job name, override reason, etc.).
    pub metadata: serde_json::Value,
}
