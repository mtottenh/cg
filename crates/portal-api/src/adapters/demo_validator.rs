//! Demo validator adapter for the match completion saga.
//!
//! Wraps the DemoService to implement the MatchDemoValidator trait.

use async_trait::async_trait;
use portal_core::{DomainError, ResultClaimId, TournamentMatchId};
use portal_domain::entities::demo_validation::DemoValidationResult;
use portal_domain::services::tournament::{DemoValidationOutcome, MatchDemoValidator};
use tracing::debug;

use crate::state::AppDemoService;
use crate::state::AppResultService;

/// Adapter that wraps DemoService + ResultService to implement MatchDemoValidator.
#[derive(Clone)]
pub struct DemoValidatorAdapter {
    demo_service: AppDemoService,
    result_service: AppResultService,
}

impl DemoValidatorAdapter {
    /// Create a new adapter.
    pub fn new(demo_service: AppDemoService, result_service: AppResultService) -> Self {
        Self {
            demo_service,
            result_service,
        }
    }
}

#[async_trait]
impl MatchDemoValidator for DemoValidatorAdapter {
    async fn validate_match_demos(
        &self,
        match_id: TournamentMatchId,
        claim_id: ResultClaimId,
    ) -> Result<Vec<DemoValidationOutcome>, DomainError> {
        // Get the confirmed claim for game results
        let claim = self.result_service.get_claim_by_id(claim_id).await?;

        debug!(
            claim_id = %claim_id,
            demo_link_ids = ?claim.demo_link_ids,
            game_results_count = claim.game_results.len(),
            "Validating demos for claim"
        );

        // Skip if no demos linked
        if claim.demo_link_ids.is_empty() {
            debug!("No demo_link_ids in claim, skipping validation");
            return Ok(Vec::new());
        }

        // Get demos linked to the match with full data
        let demo_links = self
            .demo_service
            .get_match_demos_with_data(match_id, true, None)
            .await?;

        debug!(
            match_id = %match_id,
            demo_links_count = demo_links.len(),
            "Found demo links for match"
        );

        if demo_links.is_empty() {
            debug!("No demo links found for match, skipping validation");
            return Ok(Vec::new());
        }

        let mut outcomes = Vec::new();

        for link_data in &demo_links {
            let link = &link_data.link;

            // Only validate links that are in the claim's demo_link_ids
            if !claim.demo_link_ids.contains(&link.id) {
                continue;
            }

            // Find the corresponding game result for this link's game_number
            let game_result = link
                .game_number
                .and_then(|gn| claim.game_results.iter().find(|gr| gr.game_number == gn));

            // Build validation result
            let mut validation = DemoValidationResult::default();
            let unrecognized = Vec::new();

            let demo = &link_data.demo;
            if let Some(ref metadata) = demo.metadata {
                // Extract claimed score for this game
                let claimed_score = game_result.map_or(
                    (
                        claim.claimed_participant1_score,
                        claim.claimed_participant2_score,
                    ),
                    |gr| (gr.participant1_score, gr.participant2_score),
                );
                validation.claimed_score = claimed_score;

                // Check scores
                let extracted_score = (metadata.team1_score, metadata.team2_score);
                validation.extracted_score = Some(extracted_score);

                if extracted_score == claimed_score {
                    validation.is_valid = true;
                    validation.confidence = 0.9;
                } else {
                    validation.add_error(format!(
                        "Score mismatch: demo shows {}-{} but claim says {}-{}",
                        extracted_score.0, extracted_score.1, claimed_score.0, claimed_score.1,
                    ));
                }

                // Check map name (if game result has map info)
                if let Some(ref gr) = game_result {
                    if metadata.map_name.to_lowercase() != gr.map_id.to_lowercase() {
                        validation.map_match = false;
                        validation.add_warning(
                            format!(
                                "Map mismatch: demo map '{}' vs claimed '{}'",
                                metadata.map_name, gr.map_id
                            ),
                            0.3,
                        );
                    } else {
                        validation.map_match = true;
                    }
                }
            } else {
                // No metadata — can't validate
                validation.add_warning("Demo has no parsed metadata".to_string(), 0.5);
                validation.claimed_score = (
                    claim.claimed_participant1_score,
                    claim.claimed_participant2_score,
                );
            }

            // Only include outcomes with issues
            if !validation.errors.is_empty() || !validation.warnings.is_empty() {
                outcomes.push(DemoValidationOutcome {
                    link_id: link.id,
                    validation,
                    unrecognized_players: unrecognized,
                });
            }
        }

        Ok(outcomes)
    }
}
