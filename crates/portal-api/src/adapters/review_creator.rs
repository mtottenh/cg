//! Review creator adapter for the match completion saga.
//!
//! Wraps the ResultReviewService to implement the ReviewCreator trait.

use async_trait::async_trait;
use portal_core::{DomainError, ResultClaimId, TournamentMatchId, TournamentRegistrationId};
use portal_domain::entities::demo_validation::DemoValidationResult;
use portal_domain::entities::result_review::ResultReview;
use portal_domain::services::tournament::{DemoValidationOutcome, ReviewCreator};

use crate::state::AppResultReviewService;

/// Adapter that wraps ResultReviewService to implement ReviewCreator.
#[derive(Clone)]
pub struct ReviewCreatorAdapter {
    review_service: AppResultReviewService,
}

impl ReviewCreatorAdapter {
    /// Create a new adapter.
    pub fn new(review_service: AppResultReviewService) -> Self {
        Self { review_service }
    }
}

#[async_trait]
impl ReviewCreator for ReviewCreatorAdapter {
    async fn create_if_needed(
        &self,
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        outcomes: &[DemoValidationOutcome],
        captain1_reg_id: TournamentRegistrationId,
        captain2_reg_id: TournamentRegistrationId,
    ) -> Result<Option<ResultReview>, DomainError> {
        if outcomes.is_empty() {
            return Ok(None);
        }

        // Aggregate validation results from all outcomes
        let mut has_score_mismatch = false;
        let mut has_winner_mismatch = false;
        let mut all_unrecognized = Vec::new();

        // Use the first outcome's validation as the primary validation result,
        // but aggregate issues from all outcomes
        let mut primary_validation = DemoValidationResult::default();
        let mut first_link_id = None;

        for outcome in outcomes {
            if first_link_id.is_none() {
                first_link_id = Some(outcome.link_id);
                primary_validation = outcome.validation.clone();
            }

            if outcome.validation.has_score_mismatch() {
                has_score_mismatch = true;
            }
            if outcome.validation.has_winner_mismatch() {
                has_winner_mismatch = true;
            }
            all_unrecognized.extend(outcome.unrecognized_players.clone());
        }

        // Delegate to the existing service
        self.review_service
            .create_from_validation(
                result_claim_id,
                match_id,
                first_link_id,
                primary_validation,
                has_score_mismatch,
                has_winner_mismatch,
                all_unrecognized,
                captain1_reg_id,
                captain2_reg_id,
            )
            .await
    }

    async fn get_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError> {
        self.review_service.get_for_match(match_id).await
    }
}
