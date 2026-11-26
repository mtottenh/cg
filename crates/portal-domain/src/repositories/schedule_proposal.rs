//! Schedule proposal repository trait.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{ScheduleProposalId, TournamentMatchId, UserId};
use portal_core::types::TournamentMatchStatus;

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

    /// Answer a pending proposal: write the given state over it.
    ///
    /// Only a row that is still `pending` is written. If someone else
    /// answered it first (accepted, rejected, withdrew, or it expired) this
    /// returns `Conflict` instead of overwriting their answer.
    async fn update(&self, proposal: &ScheduleProposal) -> Result<ScheduleProposal, DomainError>;

    /// Accept a pending proposal and give its match the chosen time, as one
    /// transaction: the proposal becomes `accepted`, any other pending
    /// proposal on the match is cancelled, and the match gets
    /// `scheduled_at` and status `scheduled` (with a status-log row when the
    /// status changed).
    ///
    /// `Conflict` if the proposal is no longer pending or the match is not
    /// in one of `allowed_from`; nothing is written in either case.
    async fn accept_and_schedule(
        &self,
        proposal_id: ScheduleProposalId,
        selected_time: DateTime<Utc>,
        accepted_by: UserId,
        allowed_from: &[TournamentMatchStatus],
    ) -> Result<ScheduleProposal, DomainError>;

    /// Replace a pending proposal with a counter-proposal, as one
    /// transaction: the original becomes `counter_proposed` (only if still
    /// pending, else `Conflict`), the counter is inserted, and the two are
    /// linked.
    async fn create_counter(
        &self,
        original_id: ScheduleProposalId,
        command: CreateScheduleProposalCommand,
    ) -> Result<ScheduleProposal, DomainError>;

    /// Find all proposals that have expired but are still pending.
    async fn find_expired(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ScheduleProposal>, DomainError>;

    /// Mark a proposal as expired.
    async fn mark_expired(&self, id: ScheduleProposalId) -> Result<ScheduleProposal, DomainError>;
}
