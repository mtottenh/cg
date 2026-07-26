//! Result review repository traits.

use async_trait::async_trait;
use portal_core::errors::DomainError;
use portal_core::ids::{ResultClaimId, ResultReviewId, TournamentMatchId, UserId};

use crate::entities::result_review::{ResultReview, ResultReviewStatus};

/// Data for creating a new result review.
#[derive(Debug, Clone)]
pub struct CreateResultReview {
    pub result_claim_id: ResultClaimId,
    pub match_id: TournamentMatchId,
    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,
    pub demo_link_id: Option<portal_core::DemoMatchLinkId>,
    pub validation_result: Option<serde_json::Value>,
    pub unrecognized_players: Vec<serde_json::Value>,
    pub status: ResultReviewStatus,
    pub captain1_registration_id: portal_core::TournamentRegistrationId,
    pub captain2_registration_id: portal_core::TournamentRegistrationId,
}

/// Repository for result reviews.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ResultReviewRepository: Send + Sync + 'static {
    /// Insert a new review.
    async fn insert(&self, review: &ResultReview) -> Result<(), DomainError>;

    /// Find review by ID.
    async fn find_by_id(&self, id: ResultReviewId) -> Result<Option<ResultReview>, DomainError>;

    /// Find review by result claim ID.
    async fn find_by_claim_id(
        &self,
        claim_id: ResultClaimId,
    ) -> Result<Option<ResultReview>, DomainError>;

    /// Find review by match ID.
    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError>;

    /// Find all pending reviews for the admin queue.
    ///
    /// The order is part of the contract, not an implementation detail: the
    /// queue is paginated, so whichever end sorts last is unreachable in
    /// practice. It was `created_at ASC` unconditionally (P-55), which
    /// buried every fresh escalation on the final page. Newest-first is the
    /// default; `oldest_first` (P-129) lets an admin work the backlog from
    /// the other end WITHOUT paging to it — the server sorts, so the far
    /// end is page 1 again.
    async fn find_pending_admin_reviews(
        &self,
        limit: i64,
        offset: i64,
        oldest_first: bool,
    ) -> Result<Vec<ResultReview>, DomainError>;

    /// Count pending admin reviews.
    async fn count_pending_admin_reviews(&self) -> Result<i64, DomainError>;

    /// Update captain acknowledgment.
    ///
    /// `captain_side` must be 1 or 2.
    async fn update_captain_acknowledgment(
        &self,
        id: ResultReviewId,
        captain_side: i32,
        acknowledged_by_user_id: UserId,
    ) -> Result<(), DomainError>;

    /// Update status after both captains acknowledge.
    async fn update_status(
        &self,
        id: ResultReviewId,
        status: ResultReviewStatus,
    ) -> Result<(), DomainError>;

    /// Record admin resolution.
    async fn resolve(
        &self,
        id: ResultReviewId,
        status: ResultReviewStatus,
        reviewed_by_user_id: UserId,
        admin_notes: Option<String>,
    ) -> Result<(), DomainError>;
}
