//! Schedule proposal repository trait.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{ScheduleProposalId, TournamentMatchId};

use crate::entities::{CreateScheduleProposalCommand, ScheduleProposal};

/// Repository for schedule proposals.
#[async_trait]
pub trait ScheduleProposalRepository: Send + Sync + 'static {
    /// Create a new schedule proposal.
    async fn create(
        &self,
        command: CreateScheduleProposalCommand,
    ) -> Result<ScheduleProposal, DomainError>;

    /// Find a schedule proposal by ID.
    async fn find_by_id(
        &self,
        id: ScheduleProposalId,
    ) -> Result<Option<ScheduleProposal>, DomainError>;

    /// Find all proposals for a match.
    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ScheduleProposal>, DomainError>;

    /// Find the current pending proposal for a match (if any).
    async fn find_pending_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ScheduleProposal>, DomainError>;

    /// Update a schedule proposal.
    async fn update(&self, proposal: &ScheduleProposal) -> Result<ScheduleProposal, DomainError>;

    /// Find all proposals that have expired but are still pending.
    async fn find_expired(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ScheduleProposal>, DomainError>;

    /// Mark a proposal as expired.
    async fn mark_expired(
        &self,
        id: ScheduleProposalId,
    ) -> Result<ScheduleProposal, DomainError>;
}
