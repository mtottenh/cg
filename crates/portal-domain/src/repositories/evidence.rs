//! Evidence repository traits.
//!
//! Repository for evidence storage and access logging.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{DomainError, EvidenceId, TournamentMatchId, TournamentRegistrationId, UserId};
use std::net::IpAddr;

use crate::entities::evidence::{
    Evidence, EvidenceAccessLog, EvidenceAccessType, EvidenceSource, EvidenceStatus,
    EvidenceStorage, EvidenceType,
};

// =============================================================================
// EVIDENCE REPOSITORY
// =============================================================================

/// Repository trait for evidence operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait EvidenceRepository: Send + Sync {
    /// Find evidence by ID.
    async fn find_by_id(&self, id: EvidenceId) -> Result<Option<Evidence>, DomainError>;

    /// Find all evidence for a match.
    async fn find_by_match(&self, match_id: TournamentMatchId) -> Result<Vec<Evidence>, DomainError>;

    /// Find evidence for a specific game in a match.
    async fn find_by_match_and_game(
        &self,
        match_id: TournamentMatchId,
        game_number: i32,
    ) -> Result<Vec<Evidence>, DomainError>;

    /// Create new evidence record.
    async fn create(&self, evidence: CreateEvidence) -> Result<Evidence, DomainError>;

    /// Update evidence record.
    async fn update(&self, id: EvidenceId, update: UpdateEvidence) -> Result<Evidence, DomainError>;

    /// Update evidence status.
    async fn update_status(
        &self,
        id: EvidenceId,
        status: EvidenceStatus,
    ) -> Result<Evidence, DomainError>;

    /// Mark evidence as validated.
    async fn mark_validated(
        &self,
        id: EvidenceId,
        validation_result: serde_json::Value,
    ) -> Result<Evidence, DomainError>;

    /// Soft delete evidence.
    async fn delete(&self, id: EvidenceId) -> Result<(), DomainError>;

    /// Find expired evidence.
    async fn find_expired(&self, before: DateTime<Utc>) -> Result<Vec<Evidence>, DomainError>;

    /// Log evidence access.
    async fn log_access(&self, log: CreateEvidenceAccessLog) -> Result<EvidenceAccessLog, DomainError>;

    /// Get access log for evidence.
    async fn get_access_log(&self, evidence_id: EvidenceId) -> Result<Vec<EvidenceAccessLog>, DomainError>;

    /// Find stale pending evidence (created before the given timestamp).
    /// Used by background cleanup to remove abandoned uploads.
    async fn find_stale_pending(&self, created_before: DateTime<Utc>) -> Result<Vec<Evidence>, DomainError>;
}

/// Data for creating evidence.
#[derive(Debug, Clone)]
pub struct CreateEvidence {
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,
    pub evidence_type: EvidenceType,
    pub evidence_source: EvidenceSource,
    pub name: String,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,
    pub storage: EvidenceStorage,
    pub plugin_metadata: serde_json::Value,
    pub uploaded_by_registration_id: Option<TournamentRegistrationId>,
    pub uploaded_by_user_id: Option<UserId>,
    pub discovered_by_plugin: Option<String>,
    pub discovered_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Initial status (defaults to Active if not set).
    pub status: Option<EvidenceStatus>,
}

/// Data for updating evidence.
#[derive(Debug, Clone, Default)]
pub struct UpdateEvidence {
    pub name: Option<String>,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,
    pub storage: Option<EvidenceStorage>,
    pub plugin_metadata: Option<serde_json::Value>,
    pub status: Option<EvidenceStatus>,
}

/// Data for creating an access log entry.
#[derive(Debug, Clone)]
pub struct CreateEvidenceAccessLog {
    pub evidence_id: EvidenceId,
    pub accessed_by_user_id: Option<UserId>,
    pub access_type: EvidenceAccessType,
    pub ip_address: Option<IpAddr>,
    pub user_agent: Option<String>,
}

// =============================================================================
// SAGA EXECUTION REPOSITORY
// =============================================================================

use crate::entities::saga::{SagaExecution, SagaStatus};
use portal_core::SagaId;

/// Repository trait for saga execution state.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SagaExecutionRepository: Send + Sync {
    /// Find saga by ID.
    async fn find_by_id(&self, id: SagaId) -> Result<Option<SagaExecution>, DomainError>;

    /// Create a new saga execution.
    async fn create(&self, saga: CreateSagaExecution) -> Result<SagaExecution, DomainError>;

    /// Update saga execution.
    async fn update(&self, saga: &SagaExecution) -> Result<SagaExecution, DomainError>;

    /// Update saga status.
    async fn update_status(
        &self,
        id: SagaId,
        status: SagaStatus,
    ) -> Result<SagaExecution, DomainError>;

    /// Find stuck sagas (running for too long).
    async fn find_stuck(&self, running_since_before: DateTime<Utc>) -> Result<Vec<SagaExecution>, DomainError>;

    /// Find pending sagas.
    async fn find_pending(&self) -> Result<Vec<SagaExecution>, DomainError>;

    /// Find sagas by status.
    async fn find_by_status(&self, status: SagaStatus) -> Result<Vec<SagaExecution>, DomainError>;

    /// Find sagas for a match.
    async fn find_by_match(&self, match_id: TournamentMatchId) -> Result<Vec<SagaExecution>, DomainError>;

    /// Find sagas for a tournament.
    async fn find_by_tournament(&self, tournament_id: portal_core::TournamentId) -> Result<Vec<SagaExecution>, DomainError>;
}

/// Data for creating a saga execution.
#[derive(Debug, Clone)]
pub struct CreateSagaExecution {
    pub saga_type: String,
    pub saga_version: i32,
    pub tournament_id: Option<portal_core::TournamentId>,
    pub match_id: Option<TournamentMatchId>,
    pub correlation_id: Option<String>,
    pub input_data: serde_json::Value,
    pub max_retries: i32,
}

// =============================================================================
// PROGRESSION LOG REPOSITORY
// =============================================================================

/// Repository trait for progression log.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ProgressionLogRepository: Send + Sync {
    /// Log a progression event.
    async fn log(&self, log: CreateProgressionLog) -> Result<ProgressionLog, DomainError>;

    /// Find progression logs for a source match.
    async fn find_by_source_match(
        &self,
        source_match_id: TournamentMatchId,
    ) -> Result<Vec<ProgressionLog>, DomainError>;

    /// Find progression logs for a target match.
    async fn find_by_target_match(
        &self,
        target_match_id: TournamentMatchId,
    ) -> Result<Vec<ProgressionLog>, DomainError>;

    /// Find progression logs for a saga.
    async fn find_by_saga(&self, saga_id: SagaId) -> Result<Vec<ProgressionLog>, DomainError>;

    /// Delete logs for a match (used when reverting).
    async fn delete_by_source_match(&self, source_match_id: TournamentMatchId) -> Result<(), DomainError>;
}

/// A progression log entry.
#[derive(Debug, Clone)]
pub struct ProgressionLog {
    pub id: uuid::Uuid,
    pub source_match_id: TournamentMatchId,
    pub target_match_id: Option<TournamentMatchId>,
    pub registration_id: TournamentRegistrationId,
    pub progression_type: ProgressionType,
    pub target_position: Option<i32>,
    pub saga_id: Option<SagaId>,
    pub progressed_at: DateTime<Utc>,
}

/// Type of progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressionType {
    /// Winner advances to next match
    WinnerAdvance,
    /// Loser drops to losers bracket
    LoserDrop,
    /// Loser eliminated
    LoserEliminate,
    /// Bye advancement
    ByeAdvance,
}

impl std::fmt::Display for ProgressionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WinnerAdvance => write!(f, "winner_advance"),
            Self::LoserDrop => write!(f, "loser_drop"),
            Self::LoserEliminate => write!(f, "loser_eliminate"),
            Self::ByeAdvance => write!(f, "bye_advance"),
        }
    }
}

impl std::str::FromStr for ProgressionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "winner_advance" => Ok(Self::WinnerAdvance),
            "loser_drop" => Ok(Self::LoserDrop),
            "loser_eliminate" => Ok(Self::LoserEliminate),
            "bye_advance" => Ok(Self::ByeAdvance),
            _ => Err(format!("invalid progression type: {s}")),
        }
    }
}

/// Data for creating a progression log entry.
#[derive(Debug, Clone)]
pub struct CreateProgressionLog {
    pub source_match_id: TournamentMatchId,
    pub target_match_id: Option<TournamentMatchId>,
    pub registration_id: TournamentRegistrationId,
    pub progression_type: ProgressionType,
    pub target_position: Option<i32>,
    pub saga_id: Option<SagaId>,
}
