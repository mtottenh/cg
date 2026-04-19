//! Dispute repository traits.

use async_trait::async_trait;
use portal_core::errors::DomainError;
use portal_core::ids::{
    DisputeId, DisputeMessageId, EvidenceId, ResultClaimId, TournamentId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use portal_core::types::TournamentMatchStatus;

use crate::entities::dispute::{
    AuthorType, Dispute, DisputeMessage, DisputePriority, DisputeReason, DisputeResolution,
    DisputeStatus,
};

/// Data for creating a dispute.
#[derive(Debug, Clone)]
pub struct CreateDispute {
    pub match_id: TournamentMatchId,
    pub result_claim_id: Option<ResultClaimId>,
    pub disputed_by_registration_id: TournamentRegistrationId,
    pub disputed_by_user_id: UserId,
    pub reason: DisputeReason,
    pub description: String,
    pub evidence_ids: Vec<EvidenceId>,
    pub original_winner_registration_id: Option<TournamentRegistrationId>,
    pub original_participant1_score: Option<i32>,
    pub original_participant2_score: Option<i32>,
    pub priority: DisputePriority,
}

/// Data for updating a dispute.
#[derive(Debug, Clone, Default)]
pub struct UpdateDispute {
    pub status: Option<DisputeStatus>,
    pub priority: Option<DisputePriority>,
    pub resolution: Option<DisputeResolution>,
    pub resolved_by_user_id: Option<UserId>,
}

/// Repository for disputes.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DisputeRepository: Send + Sync + 'static {
    /// Create a new dispute.
    async fn create(&self, data: CreateDispute) -> Result<Dispute, DomainError>;

    /// Find a dispute by ID.
    async fn find_by_id(&self, id: DisputeId) -> Result<Option<Dispute>, DomainError>;

    /// Find all disputes for a match.
    async fn find_by_match(&self, match_id: TournamentMatchId) -> Result<Vec<Dispute>, DomainError>;

    /// Find pending disputes for a match.
    async fn find_pending_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<Dispute>, DomainError>;

    /// Find pending disputes, optionally filtered by tournament and/or priority.
    async fn find_pending(
        &self,
        tournament_id: Option<TournamentId>,
        priority: Option<DisputePriority>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Dispute>, i64), DomainError>;

    /// Update a dispute.
    async fn update(&self, id: DisputeId, data: UpdateDispute) -> Result<Dispute, DomainError>;

    /// Check if there's a pending dispute for a match.
    async fn exists_pending_for_match(&self, match_id: TournamentMatchId)
        -> Result<bool, DomainError>;

    /// Resolve a dispute.
    async fn resolve(
        &self,
        id: DisputeId,
        resolved_by: UserId,
        resolution: DisputeResolution,
    ) -> Result<Dispute, DomainError>;

    /// Cancel a dispute.
    async fn cancel(&self, id: DisputeId) -> Result<Dispute, DomainError>;

    /// Raise a dispute in a single transaction: insert the `disputes`
    /// row, flip the match status to `Disputed`, and append the initial
    /// system message to the thread.
    ///
    /// Replaces the previous three-call chain in
    /// `DisputeService::raise_dispute`. A partial failure there could
    /// leave any of: a dispute with no `Disputed` match (so a concurrent
    /// result submission could still land), a `Disputed` match with no
    /// dispute row (so the admin queue wouldn't see it), or a dispute +
    /// status without the initial message (confusing for the
    /// participant). See audit I5.
    async fn raise_atomic(
        &self,
        create: CreateDispute,
        initial_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError>;

    /// Apply a dispute resolution that only changes the match **status**
    /// (no score overwrite). Does three writes atomically: flip the
    /// dispute to Resolved, set the match status (Completed for
    /// `Upheld`, Ready for `Rematch`, Cancelled for `DoubleDq`), and
    /// append the resolution message to the thread.
    ///
    /// Use [`Self::resolve_with_overturn`] instead when the resolution
    /// also overwrites the match scores (Overturned / Adjusted).
    async fn resolve_with_status_change(
        &self,
        dispute_id: DisputeId,
        resolved_by: UserId,
        resolution: DisputeResolution,
        match_id: TournamentMatchId,
        new_match_status: TournamentMatchStatus,
        resolution_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError>;

    /// Cancel a dispute, restore the match to Completed, and append a
    /// cancellation message — all atomically. Counterpart to
    /// [`Self::cancel`] that used to run the three writes sequentially.
    async fn cancel_with_match_restore(
        &self,
        dispute_id: DisputeId,
        match_id: TournamentMatchId,
        cancellation_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError>;

    /// Apply the resolution of a dispute in a single transaction:
    /// mark the dispute Resolved, submit the overturned scores on the
    /// match row, append the resolution message to the thread.
    ///
    /// Replaces the `dispute_repo.resolve + match_repo.submit_result +
    /// match_repo.update_status + message_repo.create` chain in
    /// `resolve_overturn`. Partial failure there left the dispute
    /// marked Resolved but the match still showing the old (disputed)
    /// result — bracket progression would then advance the wrong
    /// winner. See audit I5.
    #[allow(clippy::too_many_arguments)]
    async fn resolve_with_overturn(
        &self,
        dispute_id: DisputeId,
        resolved_by: UserId,
        resolution: DisputeResolution,
        match_id: TournamentMatchId,
        new_winner_registration_id: TournamentRegistrationId,
        new_loser_registration_id: TournamentRegistrationId,
        new_participant1_score: i32,
        new_participant2_score: i32,
        resolution_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError>;
}

/// Data for creating a dispute message.
#[derive(Debug, Clone)]
pub struct CreateDisputeMessage {
    pub dispute_id: DisputeId,
    pub author_user_id: UserId,
    pub author_type: AuthorType,
    pub message: String,
    pub evidence_ids: Vec<EvidenceId>,
    pub is_internal: bool,
}

/// Repository for dispute messages.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DisputeMessageRepository: Send + Sync + 'static {
    /// Create a new dispute message.
    async fn create(&self, data: CreateDisputeMessage) -> Result<DisputeMessage, DomainError>;

    /// Find a dispute message by ID.
    async fn find_by_id(&self, id: DisputeMessageId)
        -> Result<Option<DisputeMessage>, DomainError>;

    /// Find all messages for a dispute, optionally including internal messages.
    async fn find_by_dispute(
        &self,
        dispute_id: DisputeId,
        include_internal: bool,
    ) -> Result<Vec<DisputeMessage>, DomainError>;

    /// Count messages in a dispute.
    async fn count_by_dispute(&self, dispute_id: DisputeId) -> Result<i64, DomainError>;
}
