//! Dispute repository traits.

use async_trait::async_trait;
use portal_core::errors::DomainError;
use portal_core::ids::{
    DisputeId, DisputeMessageId, EvidenceId, ResultClaimId, TournamentId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};

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
