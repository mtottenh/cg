//! Result review service.
//!
//! Handles validation discrepancies between claimed results and demo evidence,
//! routing issues through appropriate approval workflows (captain acknowledgment or admin review).

use std::sync::Arc;

use portal_core::{
    DemoMatchLinkId, DomainError, ResultClaimId, ResultReviewId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use tracing::{info, instrument};

use crate::entities::demo_validation::{DemoValidationResult, UnrecognizedPlayer};
use crate::entities::result_review::{ResultReview, ResultReviewStatus};
use crate::repositories::result_review::ResultReviewRepository;
use crate::repositories::tournament::TournamentMatchRepository;

/// Service for handling result reviews.
#[derive(Clone)]
pub struct ResultReviewService<RRR, MR> {
    review_repo: Arc<RRR>,
    /// Match repository for future use (e.g., match status validation).
    #[allow(dead_code)]
    match_repo: Arc<MR>,
}

impl<RRR, MR> ResultReviewService<RRR, MR>
where
    RRR: ResultReviewRepository,
    MR: TournamentMatchRepository,
{
    /// Create a new result review service.
    pub fn new(review_repo: Arc<RRR>, match_repo: Arc<MR>) -> Self {
        Self {
            review_repo,
            match_repo,
        }
    }

    /// Create a review from a validation result.
    ///
    /// Returns `None` if validation passed with no issues.
    /// Returns `Some(review)` if there are mismatches to review.
    #[instrument(skip(self, validation_result, unrecognized_players))]
    #[allow(clippy::too_many_arguments)]
    pub async fn create_from_validation(
        &self,
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        demo_link_id: Option<DemoMatchLinkId>,
        validation_result: DemoValidationResult,
        score_mismatch: bool,
        winner_mismatch: bool,
        unrecognized_players: Vec<UnrecognizedPlayer>,
        captain1_registration_id: TournamentRegistrationId,
        captain2_registration_id: TournamentRegistrationId,
    ) -> Result<Option<ResultReview>, DomainError> {
        // If there are no issues, no review is needed
        let has_roster_mismatch = !unrecognized_players.is_empty();
        if !has_roster_mismatch && !score_mismatch && !winner_mismatch {
            return Ok(None);
        }

        // Determine review type based on mismatches
        let review = if score_mismatch || winner_mismatch {
            // Score/winner mismatches require admin review
            ResultReview::for_score_mismatch(
                result_claim_id,
                match_id,
                demo_link_id,
                validation_result,
                score_mismatch,
                winner_mismatch,
                unrecognized_players,
                captain1_registration_id,
                captain2_registration_id,
            )
        } else {
            // Roster mismatch only requires captain acknowledgment
            ResultReview::for_roster_mismatch(
                result_claim_id,
                match_id,
                demo_link_id,
                validation_result,
                unrecognized_players,
                captain1_registration_id,
                captain2_registration_id,
            )
        };

        // Persist the review
        self.review_repo.insert(&review).await?;

        info!(
            review_id = %review.id,
            match_id = %match_id,
            roster_mismatch = review.roster_mismatch,
            score_mismatch = review.score_mismatch,
            winner_mismatch = review.winner_mismatch,
            status = %review.status,
            "Created result review"
        );

        Ok(Some(review))
    }

    /// Captain acknowledges the roster mismatch.
    ///
    /// When both captains acknowledge, status transitions to `Acknowledged`.
    #[instrument(skip(self))]
    pub async fn acknowledge(
        &self,
        review_id: ResultReviewId,
        registration_id: TournamentRegistrationId,
        user_id: UserId,
    ) -> Result<ResultReview, DomainError> {
        let review = self.get_review(review_id).await?;

        // Validate the review status
        if review.status != ResultReviewStatus::PendingAcknowledgment {
            return Err(DomainError::InvalidReviewState(
                review.status.to_string(),
                "review is not pending acknowledgment".to_string(),
            ));
        }

        // Determine which captain is acknowledging
        let captain_side = review.get_captain_side(registration_id).ok_or_else(|| {
            DomainError::NotAuthorized(format!(
                "Registration {registration_id} is not a captain for this review"
            ))
        })?;

        // Check if already acknowledged
        if review.is_captain_acknowledged(captain_side) {
            return Err(DomainError::ReviewAlreadyAcknowledged(captain_side));
        }

        // Update acknowledgment
        self.review_repo
            .update_captain_acknowledgment(review_id, captain_side, user_id)
            .await?;

        // Check if both captains have now acknowledged
        let other_acknowledged = match captain_side {
            1 => review.captain2_acknowledged,
            2 => review.captain1_acknowledged,
            _ => false,
        };

        if other_acknowledged {
            // Both captains have acknowledged, update status
            self.review_repo
                .update_status(review_id, ResultReviewStatus::Acknowledged)
                .await?;

            info!(
                review_id = %review_id,
                "Both captains acknowledged roster mismatch"
            );
        } else {
            info!(
                review_id = %review_id,
                captain_side = captain_side,
                "Captain acknowledged roster mismatch (waiting for other captain)"
            );
        }

        // Fetch and return updated review
        self.get_review(review_id).await
    }

    /// Admin approves the result despite mismatches.
    #[instrument(skip(self, admin_notes))]
    pub async fn approve(
        &self,
        review_id: ResultReviewId,
        admin_user_id: UserId,
        admin_notes: Option<String>,
    ) -> Result<ResultReview, DomainError> {
        let review = self.get_review(review_id).await?;

        // Validate the review can be resolved
        if review.status.is_terminal() {
            return Err(DomainError::InvalidReviewState(
                review.status.to_string(),
                "review has already been resolved".to_string(),
            ));
        }

        // Approve the review
        self.review_repo
            .resolve(
                review_id,
                ResultReviewStatus::Approved,
                admin_user_id,
                admin_notes,
            )
            .await?;

        info!(
            review_id = %review_id,
            admin_user_id = %admin_user_id,
            "Admin approved result review"
        );

        // Fetch and return updated review
        self.get_review(review_id).await
    }

    /// Admin rejects the result.
    #[instrument(skip(self, admin_notes))]
    pub async fn reject(
        &self,
        review_id: ResultReviewId,
        admin_user_id: UserId,
        admin_notes: Option<String>,
    ) -> Result<ResultReview, DomainError> {
        let review = self.get_review(review_id).await?;

        // Validate the review can be resolved
        if review.status.is_terminal() {
            return Err(DomainError::InvalidReviewState(
                review.status.to_string(),
                "review has already been resolved".to_string(),
            ));
        }

        // Reject the review
        self.review_repo
            .resolve(
                review_id,
                ResultReviewStatus::Rejected,
                admin_user_id,
                admin_notes,
            )
            .await?;

        info!(
            review_id = %review_id,
            admin_user_id = %admin_user_id,
            "Admin rejected result review"
        );

        // Fetch and return updated review
        self.get_review(review_id).await
    }

    /// Get review for a match.
    #[instrument(skip(self))]
    pub async fn get_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError> {
        self.review_repo.find_by_match_id(match_id).await
    }

    /// Get review by ID.
    #[instrument(skip(self))]
    pub async fn get_by_id(
        &self,
        review_id: ResultReviewId,
    ) -> Result<Option<ResultReview>, DomainError> {
        self.review_repo.find_by_id(review_id).await
    }

    /// List pending admin reviews.
    #[instrument(skip(self))]
    pub async fn list_pending_reviews(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ResultReview>, i64), DomainError> {
        let reviews = self
            .review_repo
            .find_pending_admin_reviews(limit, offset)
            .await?;
        let total = self.review_repo.count_pending_admin_reviews().await?;

        Ok((reviews, total))
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    async fn get_review(&self, review_id: ResultReviewId) -> Result<ResultReview, DomainError> {
        self.review_repo
            .find_by_id(review_id)
            .await?
            .ok_or(DomainError::ResultReviewNotFound(review_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_status_transitions() {
        // Test that terminal states are correctly identified
        assert!(ResultReviewStatus::Approved.is_terminal());
        assert!(ResultReviewStatus::Rejected.is_terminal());
        assert!(!ResultReviewStatus::PendingAcknowledgment.is_terminal());
        assert!(!ResultReviewStatus::PendingAdminReview.is_terminal());
        assert!(!ResultReviewStatus::Acknowledged.is_terminal());

        // Test pending states
        assert!(ResultReviewStatus::PendingAcknowledgment.is_pending());
        assert!(ResultReviewStatus::PendingAdminReview.is_pending());
        assert!(!ResultReviewStatus::Acknowledged.is_pending());
        assert!(!ResultReviewStatus::Approved.is_pending());
        assert!(!ResultReviewStatus::Rejected.is_pending());
    }
}
