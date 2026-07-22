//! Match completion saga.
//!
//! Orchestrates the multi-step process of completing a match:
//! 1. Validate the match result
//! 2. Validate demos (if any linked) and create review if needed
//! 3. Get bracket info
//! 4. Complete match status
//! 5. Advance winner to next match
//! 6. Route loser (to losers bracket or eliminate)
//! 7. Update standings (for round robin/swiss formats)

use std::sync::Arc;

use async_trait::async_trait;
use portal_core::types::{BracketType, TournamentMatchStatus};
use portal_core::{
    DomainError, ResultClaimId, ResultReviewId, SagaId, TournamentMatchId, TournamentRegistrationId,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument, warn};

use crate::entities::demo_validation::{DemoValidationResult, UnrecognizedPlayer};
use crate::entities::result_review::{ResultReview, ResultReviewStatus};
use crate::entities::saga::{SagaContext, SagaExecution};
use crate::entities::tournament::{TournamentBracket, TournamentMatch};
use crate::repositories::evidence::{
    CreateProgressionLog, ProgressionLogRepository, ProgressionType, SagaExecutionRepository,
};
use crate::repositories::tournament::{
    ParticipantSlot, TournamentBracketRepository, TournamentMatchRepository,
    TournamentRegistrationRepository, TournamentStandingsRepository,
};
use crate::services::tournament::{Saga, SagaCoordinator, SagaDefinition, SagaResult};
use portal_core::types::MatchParticipantSource;

// =============================================================================
// DEMO VALIDATION & REVIEW TRAITS
// =============================================================================

/// Outcome of validating a single demo-match link.
#[derive(Debug, Clone)]
pub struct DemoValidationOutcome {
    /// The demo match link ID that was validated.
    pub link_id: portal_core::DemoMatchLinkId,
    /// The validation result.
    pub validation: DemoValidationResult,
    /// Unrecognized players found in the demo.
    pub unrecognized_players: Vec<UnrecognizedPlayer>,
}

/// Trait for validating demos linked to a match.
///
/// Used by the saga to validate demo evidence without depending on the full DemoService.
#[async_trait]
pub trait MatchDemoValidator: Send + Sync + 'static {
    /// Validate all demos linked to a match against the confirmed result.
    ///
    /// Returns only outcomes with issues (non-empty warnings/errors).
    async fn validate_match_demos(
        &self,
        match_id: TournamentMatchId,
        claim_id: ResultClaimId,
    ) -> Result<Vec<DemoValidationOutcome>, DomainError>;
}

/// Trait for creating result reviews from validation outcomes.
///
/// Used by the saga to create reviews without depending on the full ResultReviewService.
#[async_trait]
pub trait ReviewCreator: Send + Sync + 'static {
    /// Create a result review if validation issues were found.
    ///
    /// Returns `None` if no review is needed.
    async fn create_if_needed(
        &self,
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        outcomes: &[DemoValidationOutcome],
        captain1_reg_id: TournamentRegistrationId,
        captain2_reg_id: TournamentRegistrationId,
    ) -> Result<Option<ResultReview>, DomainError>;

    /// Get the review for a match (if one exists).
    async fn get_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError>;
}

/// Trait for updating player stats after match completion.
///
/// Used by the saga to update player game profiles without depending on the full service stack.
#[async_trait]
pub trait MatchStatsUpdater: Send + Sync + 'static {
    /// Update player stats for all participants after a match completes.
    ///
    /// The implementation should look up the game_id from the match/tournament,
    /// resolve player IDs from registrations, and call the appropriate plugin
    /// and profile service methods.
    async fn update_player_stats(
        &self,
        match_id: TournamentMatchId,
        winner_registration_id: TournamentRegistrationId,
        loser_registration_id: TournamentRegistrationId,
        is_forfeit: bool,
    ) -> Result<(), DomainError>;
}

// =============================================================================
// INPUT/OUTPUT TYPES
// =============================================================================

/// Input for match completion saga.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchCompletionInput {
    /// The match being completed.
    pub match_id: TournamentMatchId,
    /// The winner registration ID.
    pub winner_registration_id: TournamentRegistrationId,
    /// The loser registration ID.
    pub loser_registration_id: TournamentRegistrationId,
    /// Winner's score.
    pub winner_score: i32,
    /// Loser's score.
    pub loser_score: i32,
    /// Whether this was a forfeit.
    pub is_forfeit: bool,
    /// ID of the saga execution for tracking.
    pub saga_id: Option<SagaId>,
    /// The result claim ID (used for demo validation).
    pub result_claim_id: Option<ResultClaimId>,
}

/// Output of match completion saga.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchCompletionOutput {
    /// The completed match.
    pub match_id: TournamentMatchId,
    /// Next match ID for the winner (if any).
    pub winner_next_match_id: Option<TournamentMatchId>,
    /// Next match ID for the loser (if any - double elim only).
    pub loser_next_match_id: Option<TournamentMatchId>,
    /// Whether standings were updated.
    pub standings_updated: bool,
    /// Whether a review is pending.
    pub review_pending: bool,
    /// The review ID if one was created.
    pub review_id: Option<ResultReviewId>,
    /// Summary message.
    pub summary: String,
}

// =============================================================================
// MATCH COMPLETION SAGA
// =============================================================================

/// Saga for completing a match and processing bracket progression.
///
/// This saga handles:
/// - Match result validation
/// - Demo validation and review creation
/// - Match completion
/// - Winner advancement to next match
/// - Loser routing (elimination or losers bracket)
/// - Standings updates for round robin/swiss
pub struct MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR, MDV, RC, MSU> {
    match_repo: Arc<TMR>,
    bracket_repo: Arc<TBR>,
    registration_repo: Arc<TRR>,
    standings_repo: Arc<TSTR>,
    saga_repo: Arc<SR>,
    progression_log_repo: Arc<PLR>,
    demo_validator: Arc<MDV>,
    review_creator: Arc<RC>,
    stats_updater: Arc<MSU>,
}

impl<TMR, TBR, TRR, TSTR, SR, PLR, MDV, RC, MSU> Clone
    for MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR, MDV, RC, MSU>
{
    fn clone(&self) -> Self {
        Self {
            match_repo: Arc::clone(&self.match_repo),
            bracket_repo: Arc::clone(&self.bracket_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            standings_repo: Arc::clone(&self.standings_repo),
            saga_repo: Arc::clone(&self.saga_repo),
            progression_log_repo: Arc::clone(&self.progression_log_repo),
            demo_validator: Arc::clone(&self.demo_validator),
            review_creator: Arc::clone(&self.review_creator),
            stats_updater: Arc::clone(&self.stats_updater),
        }
    }
}

impl<TMR, TBR, TRR, TSTR, SR, PLR, MDV, RC, MSU>
    MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR, MDV, RC, MSU>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingsRepository,
    SR: SagaExecutionRepository,
    PLR: ProgressionLogRepository,
    MDV: MatchDemoValidator,
    RC: ReviewCreator,
    MSU: MatchStatsUpdater,
{
    /// Create a new match completion saga.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        match_repo: Arc<TMR>,
        bracket_repo: Arc<TBR>,
        registration_repo: Arc<TRR>,
        standings_repo: Arc<TSTR>,
        saga_repo: Arc<SR>,
        progression_log_repo: Arc<PLR>,
        demo_validator: Arc<MDV>,
        review_creator: Arc<RC>,
        stats_updater: Arc<MSU>,
    ) -> Self {
        Self {
            match_repo,
            bracket_repo,
            registration_repo,
            standings_repo,
            saga_repo,
            progression_log_repo,
            demo_validator,
            review_creator,
            stats_updater,
        }
    }

    /// Execute the match completion saga.
    #[instrument(skip(self))]
    pub async fn execute_completion(
        &self,
        input: MatchCompletionInput,
    ) -> Result<SagaResult<MatchCompletionOutput>, DomainError> {
        let saga_coordinator = SagaCoordinator::new(Arc::clone(&self.saga_repo));

        // Create saga execution record
        let definition = self.definition();
        let context = SagaContext::with_match(
            input.match_id,
            self.get_tournament_id(input.match_id).await?,
        );
        let input_json = serde_json::to_value(&input)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize input: {e}")))?;

        let mut execution = saga_coordinator
            .start_saga(&definition, context, input_json)
            .await?;

        // Execute the saga steps
        let result = self
            .run_steps(&saga_coordinator, &mut execution, input)
            .await;

        match result {
            Ok(output) => {
                saga_coordinator.complete_saga(&mut execution).await?;
                Ok(SagaResult::success(execution, output))
            }
            Err(DomainError::SagaPaused(ref msg)) => {
                saga_coordinator.pause_saga(&mut execution, msg).await?;
                Ok(SagaResult::paused(execution))
            }
            Err(e) => {
                // Attempt compensation if needed
                if self.should_compensate(&execution)
                    && let Err(comp_err) = self.compensate(&saga_coordinator, &mut execution).await
                {
                    error!(
                        saga_id = %execution.id,
                        error = %comp_err,
                        "Compensation failed"
                    );
                }
                saga_coordinator
                    .fail_saga(&mut execution, &e.to_string())
                    .await?;
                Err(e)
            }
        }
    }

    /// Continue the saga after a review has been resolved.
    ///
    /// This resumes from step 3 (get bracket) onward, skipping validation steps.
    #[instrument(skip(self))]
    pub async fn continue_after_review(
        &self,
        match_id: TournamentMatchId,
        input: MatchCompletionInput,
    ) -> Result<SagaResult<MatchCompletionOutput>, DomainError> {
        // Verify review is resolved
        let review = self.review_creator.get_for_match(match_id).await?;
        if let Some(ref review) = review {
            if review.status.is_pending() {
                return Err(DomainError::InvalidState(
                    "Review is still pending".to_string(),
                ));
            }
            if review.status == ResultReviewStatus::Rejected {
                return Err(DomainError::ResultRejectedByReview(review.id.to_string()));
            }
        }

        // Idempotency guard for the double-resume.
        //
        // `Acknowledged` is not a terminal review status, so both captains
        // acknowledging a roster-only review resumes this saga (running the
        // progression steps to completion) AND a subsequent admin `approve`
        // of that same non-terminal review resumes it a *second* time. The
        // standings step is now idempotent (it recomputes rather than adds),
        // but the best-effort player-stats step is not — a second resume
        // would double-count every participant's match/win/streak.
        //
        // Once a `match_completion` saga for this match has run to
        // completion, the progression effects are already applied, so any
        // further resume is a no-op. This does not affect the background
        // re-drive of a *failed* saga (that path goes through
        // `redrive_stuck_completion_sagas`, never here), which the derived
        // standings recompute makes safe on its own.
        let prior_sagas = self.saga_repo.find_by_match(match_id).await?;
        if let Some(done) = prior_sagas
            .into_iter()
            .find(|s| s.saga_type == "match_completion" && s.is_completed())
        {
            info!(
                match_id = %match_id,
                saga_id = %done.id,
                "Match completion already applied; skipping duplicate review resume"
            );
            let output = MatchCompletionOutput {
                match_id,
                winner_next_match_id: None,
                loser_next_match_id: None,
                standings_updated: false,
                review_pending: false,
                review_id: review.as_ref().map(|r| r.id),
                summary: "Match completion already applied; resume skipped".to_string(),
            };
            return Ok(SagaResult::success(done, output));
        }

        let saga_coordinator = SagaCoordinator::new(Arc::clone(&self.saga_repo));

        // Create a new saga execution for the continuation
        let definition = self.definition();
        let context = SagaContext::with_match(
            input.match_id,
            self.get_tournament_id(input.match_id).await?,
        );
        let input_json = serde_json::to_value(&input)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize input: {e}")))?;

        let mut execution = saga_coordinator
            .start_saga(&definition, context, input_json)
            .await?;

        // Run from step 3 onward (skip validation and demo validation)
        let result = self
            .run_progression_steps(&saga_coordinator, &mut execution, input)
            .await;

        match result {
            Ok(output) => {
                saga_coordinator.complete_saga(&mut execution).await?;
                Ok(SagaResult::success(execution, output))
            }
            Err(e) => {
                if self.should_compensate(&execution)
                    && let Err(comp_err) = self.compensate(&saga_coordinator, &mut execution).await
                {
                    error!(
                        saga_id = %execution.id,
                        error = %comp_err,
                        "Compensation failed"
                    );
                }
                saga_coordinator
                    .fail_saga(&mut execution, &e.to_string())
                    .await?;
                Err(e)
            }
        }
    }

    /// Run the saga steps in sequence.
    async fn run_steps(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: MatchCompletionInput,
    ) -> Result<MatchCompletionOutput, DomainError> {
        // Step 1: Validate and get match state
        let match_ = self
            .step_validate_match(saga_coordinator, execution, &input)
            .await?;

        // Step 2: Validate demos (if any linked)
        let review = self
            .step_validate_demos(saga_coordinator, execution, &input, &match_)
            .await?;

        // If review is pending, pause the saga
        if let Some(ref review) = review {
            if review.status.is_pending() {
                return Err(DomainError::SagaPaused(format!(
                    "Awaiting review {} resolution",
                    review.id
                )));
            }
            if review.status == ResultReviewStatus::Rejected {
                return Err(DomainError::ResultRejectedByReview(review.id.to_string()));
            }
        }

        // Steps 3-7: Bracket progression
        self.run_progression_steps_inner(saga_coordinator, execution, input, &match_)
            .await
    }

    /// Run only the progression steps (used for both initial and continuation).
    async fn run_progression_steps(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: MatchCompletionInput,
    ) -> Result<MatchCompletionOutput, DomainError> {
        // Re-fetch the match state
        let match_ = self
            .match_repo
            .find_by_id(input.match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(input.match_id))?;

        self.run_progression_steps_inner(saga_coordinator, execution, input, &match_)
            .await
    }

    /// Inner progression steps (steps 3-7).
    async fn run_progression_steps_inner(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: MatchCompletionInput,
        match_: &TournamentMatch,
    ) -> Result<MatchCompletionOutput, DomainError> {
        // Step 3: Get bracket info
        let bracket = self
            .step_get_bracket(saga_coordinator, execution, match_)
            .await?;

        // Step 4: Complete the match
        self.step_complete_match(saga_coordinator, execution, &input)
            .await?;

        // Step 5: Advance winner
        let winner_next_match_id = self
            .step_advance_winner(saga_coordinator, execution, match_, &input)
            .await?;

        // Step 6: Route loser
        let loser_next_match_id = self
            .step_route_loser(saga_coordinator, execution, match_, &input, &bracket)
            .await?;

        // Step 6.5: Mark newly ready matches
        self.step_mark_ready_matches(
            saga_coordinator,
            execution,
            winner_next_match_id,
            loser_next_match_id,
        )
        .await?;

        // Step 7: Update standings (if applicable)
        let standings_updated = self
            .step_update_standings(saga_coordinator, execution, match_, &bracket)
            .await?;

        // Step 8: Update player stats (best-effort, non-blocking)
        self.step_update_player_stats(saga_coordinator, execution, &input)
            .await;

        // Build summary
        let summary = self.build_summary(
            winner_next_match_id.is_some(),
            loser_next_match_id.is_some(),
            standings_updated,
            &bracket,
        );

        Ok(MatchCompletionOutput {
            match_id: input.match_id,
            winner_next_match_id,
            loser_next_match_id,
            standings_updated,
            review_pending: false,
            review_id: None,
            summary,
        })
    }

    // =========================================================================
    // SAGA STEPS
    // =========================================================================

    /// Step 1: Validate the match state.
    async fn step_validate_match(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: &MatchCompletionInput,
    ) -> Result<TournamentMatch, DomainError> {
        const STEP_NAME: &str = "validate_match";

        let match_ = self
            .match_repo
            .find_by_id(input.match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(input.match_id))?;

        // Validate match is in valid state for completion.
        // The match may already be completed (by confirm_claim) — that's OK,
        // the saga handles progression from that point.
        if match_.status != TournamentMatchStatus::Completed
            && !match_
                .status
                .can_transition_to(TournamentMatchStatus::Completed)
        {
            let err = format!("Match in {} status cannot be completed", match_.status);
            saga_coordinator
                .fail_step(execution, STEP_NAME, &err)
                .await?;
            return Err(DomainError::InvalidState(err));
        }

        // Validate participants
        let p1 = match_.participant1_registration_id;
        let p2 = match_.participant2_registration_id;

        let valid_winner =
            p1 == Some(input.winner_registration_id) || p2 == Some(input.winner_registration_id);
        let valid_loser =
            p1 == Some(input.loser_registration_id) || p2 == Some(input.loser_registration_id);

        if !valid_winner || !valid_loser {
            let err = "Winner or loser is not a participant in this match".to_string();
            saga_coordinator
                .fail_step(execution, STEP_NAME, &err)
                .await?;
            return Err(DomainError::InvalidState(err));
        }

        // Validate they are different
        if input.winner_registration_id == input.loser_registration_id {
            let err = "Winner and loser cannot be the same".to_string();
            saga_coordinator
                .fail_step(execution, STEP_NAME, &err)
                .await?;
            return Err(DomainError::InvalidState(err));
        }

        saga_coordinator
            .complete_step(execution, STEP_NAME, None)
            .await?;

        info!(
            match_id = %input.match_id,
            "Match validated for completion"
        );

        Ok(match_)
    }

    /// Step 2: Validate demos linked to the match.
    async fn step_validate_demos(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: &MatchCompletionInput,
        match_: &TournamentMatch,
    ) -> Result<Option<ResultReview>, DomainError> {
        const STEP_NAME: &str = "validate_demos";

        // Skip for forfeits (no demos to validate)
        if input.is_forfeit {
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({"action": "skipped_forfeit"})),
                )
                .await?;
            return Ok(None);
        }

        // Skip if no result claim ID
        let Some(claim_id) = input.result_claim_id else {
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({"action": "no_claim_id"})),
                )
                .await?;
            return Ok(None);
        };

        // Validate demos via the trait
        let outcomes = self
            .demo_validator
            .validate_match_demos(input.match_id, claim_id)
            .await?;

        // If no issues found, skip review
        if outcomes.is_empty() {
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({"action": "all_valid"})),
                )
                .await?;
            return Ok(None);
        }

        // Determine captain registration IDs
        let captain1 = match_
            .participant1_registration_id
            .ok_or_else(|| DomainError::InvalidState("Match participant 1 not set".to_string()))?;
        let captain2 = match_
            .participant2_registration_id
            .ok_or_else(|| DomainError::InvalidState("Match participant 2 not set".to_string()))?;

        // Create review if issues found
        let review = self
            .review_creator
            .create_if_needed(claim_id, input.match_id, &outcomes, captain1, captain2)
            .await?;

        if let Some(ref review) = review {
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({
                        "action": "review_created",
                        "review_id": review.id.to_string(),
                        "status": review.status.to_string()
                    })),
                )
                .await?;

            info!(
                match_id = %input.match_id,
                review_id = %review.id,
                "Demo validation review created"
            );
        } else {
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({"action": "no_review_needed"})),
                )
                .await?;
        }

        Ok(review)
    }

    /// Step 3: Get bracket information.
    async fn step_get_bracket(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        match_: &TournamentMatch,
    ) -> Result<TournamentBracket, DomainError> {
        const STEP_NAME: &str = "get_bracket";

        let bracket = self
            .bracket_repo
            .find_by_id(match_.bracket_id)
            .await?
            .ok_or(DomainError::TournamentBracketNotFound(match_.bracket_id))?;

        saga_coordinator
            .complete_step(execution, STEP_NAME, None)
            .await?;

        Ok(bracket)
    }

    /// Step 4: Complete the match (idempotent — skips if already completed).
    async fn step_complete_match(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: &MatchCompletionInput,
    ) -> Result<(), DomainError> {
        const STEP_NAME: &str = "complete_match";

        let match_ = self
            .match_repo
            .find_by_id(input.match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(input.match_id))?;

        // If the match is already completed (e.g., by confirm_claim), skip
        if match_.status == TournamentMatchStatus::Completed {
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({
                        "action": "already_completed",
                        "winner_id": input.winner_registration_id.to_string()
                    })),
                )
                .await?;

            info!(
                match_id = %input.match_id,
                "Match already completed, skipping"
            );
            return Ok(());
        }

        let (p1_score, p2_score) =
            if match_.participant1_registration_id == Some(input.winner_registration_id) {
                (input.winner_score, input.loser_score)
            } else {
                (input.loser_score, input.winner_score)
            };

        // Submit the result. `submit_result` already sets
        // `status = 'completed'` on the match row (see the adapter's
        // UPDATE), so the prior separate `match_repo.complete(...)`
        // call that used to follow this was redundant *and*
        // non-atomic — if `complete()` failed after `submit_result()`,
        // the saga step logged a partial success but the db reflected
        // the correct end state anyway. Dropping it tightens the
        // invariant: one write, one atomic transition. See audit I5.
        self.match_repo
            .submit_result(
                input.match_id,
                p1_score,
                p2_score,
                input.winner_registration_id,
                input.loser_registration_id,
            )
            .await?;

        saga_coordinator
            .complete_step(
                execution,
                STEP_NAME,
                Some(serde_json::json!({
                    "winner_id": input.winner_registration_id.to_string(),
                    "score": format!("{}-{}", p1_score, p2_score)
                })),
            )
            .await?;

        info!(
            match_id = %input.match_id,
            winner = %input.winner_registration_id,
            "Match completed"
        );

        Ok(())
    }

    /// Step 5: Advance winner to next match.
    async fn step_advance_winner(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        match_: &TournamentMatch,
        input: &MatchCompletionInput,
    ) -> Result<Option<TournamentMatchId>, DomainError> {
        const STEP_NAME: &str = "advance_winner";

        // Check if there's a next match for the winner
        let Some(next_match_id) = match_.winner_progresses_to else {
            // No next match - this was the final
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({ "action": "no_progression_needed" })),
                )
                .await?;
            return Ok(None);
        };

        // Get the registration info for populating the next match
        let registration = self
            .registration_repo
            .find_by_id(input.winner_registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(
                input.winner_registration_id,
            ))?;

        // Determine which slot the winner goes to
        let target_slot = self
            .determine_target_slot(match_, next_match_id, true)
            .await?;

        // Assign the winner to the next match
        self.match_repo
            .assign_participant(
                next_match_id,
                target_slot,
                input.winner_registration_id,
                registration.participant_name.clone(),
                registration.participant_logo_url.clone(),
                registration.seed,
            )
            .await?;

        // Log the progression
        self.progression_log_repo
            .log(CreateProgressionLog {
                source_match_id: input.match_id,
                target_match_id: Some(next_match_id),
                registration_id: input.winner_registration_id,
                progression_type: ProgressionType::WinnerAdvance,
                target_position: Some(if target_slot == ParticipantSlot::One {
                    1
                } else {
                    2
                }),
                saga_id: Some(execution.id),
            })
            .await?;

        saga_coordinator
            .complete_step(
                execution,
                STEP_NAME,
                Some(serde_json::json!({
                    "next_match_id": next_match_id.to_string(),
                    "slot": format!("{:?}", target_slot)
                })),
            )
            .await?;

        info!(
            match_id = %input.match_id,
            next_match_id = %next_match_id,
            winner = %input.winner_registration_id,
            "Winner advanced"
        );

        Ok(Some(next_match_id))
    }

    /// Step 6: Route loser.
    async fn step_route_loser(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        match_: &TournamentMatch,
        input: &MatchCompletionInput,
        bracket: &TournamentBracket,
    ) -> Result<Option<TournamentMatchId>, DomainError> {
        const STEP_NAME: &str = "route_loser";

        // Check if this bracket has loser progression (double elimination)
        let Some(loser_match_id) = match_.loser_progresses_to else {
            // No loser bracket - loser is eliminated
            let progression_type = match bracket.bracket_type {
                BracketType::RoundRobin | BracketType::Swiss => {
                    // Not really eliminated in these formats
                    saga_coordinator
                        .complete_step(
                            execution,
                            STEP_NAME,
                            Some(serde_json::json!({ "action": "not_applicable" })),
                        )
                        .await?;
                    return Ok(None);
                }
                _ => ProgressionType::LoserEliminate,
            };

            // Log the elimination
            self.progression_log_repo
                .log(CreateProgressionLog {
                    source_match_id: input.match_id,
                    target_match_id: None,
                    registration_id: input.loser_registration_id,
                    progression_type,
                    target_position: None,
                    saga_id: Some(execution.id),
                })
                .await?;

            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({ "action": "eliminated" })),
                )
                .await?;

            info!(
                match_id = %input.match_id,
                loser = %input.loser_registration_id,
                "Loser eliminated"
            );

            return Ok(None);
        };

        // Get the registration info
        let registration = self
            .registration_repo
            .find_by_id(input.loser_registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(
                input.loser_registration_id,
            ))?;

        // Determine which slot the loser goes to
        let target_slot = self
            .determine_target_slot(match_, loser_match_id, false)
            .await?;

        // Assign the loser to the losers bracket match
        self.match_repo
            .assign_participant(
                loser_match_id,
                target_slot,
                input.loser_registration_id,
                registration.participant_name.clone(),
                registration.participant_logo_url.clone(),
                registration.seed,
            )
            .await?;

        // Log the progression
        self.progression_log_repo
            .log(CreateProgressionLog {
                source_match_id: input.match_id,
                target_match_id: Some(loser_match_id),
                registration_id: input.loser_registration_id,
                progression_type: ProgressionType::LoserDrop,
                target_position: Some(if target_slot == ParticipantSlot::One {
                    1
                } else {
                    2
                }),
                saga_id: Some(execution.id),
            })
            .await?;

        saga_coordinator
            .complete_step(
                execution,
                STEP_NAME,
                Some(serde_json::json!({
                    "target_match_id": loser_match_id.to_string(),
                    "slot": format!("{:?}", target_slot)
                })),
            )
            .await?;

        info!(
            match_id = %input.match_id,
            loser_match_id = %loser_match_id,
            loser = %input.loser_registration_id,
            "Loser dropped to losers bracket"
        );

        Ok(Some(loser_match_id))
    }

    /// Step 6.5: Mark target matches as Ready if both participants are now assigned.
    async fn step_mark_ready_matches(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        winner_next_match_id: Option<TournamentMatchId>,
        loser_next_match_id: Option<TournamentMatchId>,
    ) -> Result<(), DomainError> {
        const STEP_NAME: &str = "mark_ready_matches";
        let mut newly_ready = Vec::new();

        for target_id in [winner_next_match_id, loser_next_match_id]
            .into_iter()
            .flatten()
        {
            if let Some(target) = self.match_repo.find_by_id(target_id).await?
                && target.status == TournamentMatchStatus::Pending
                && target.has_both_participants()
            {
                self.match_repo
                    .update_status(target_id, TournamentMatchStatus::Ready)
                    .await?;
                newly_ready.push(target_id.to_string());
            }
        }

        saga_coordinator
            .complete_step(
                execution,
                STEP_NAME,
                Some(serde_json::json!({ "newly_ready": newly_ready })),
            )
            .await?;

        if !newly_ready.is_empty() {
            info!(matches = ?newly_ready, "Marked matches as ready");
        }

        Ok(())
    }

    /// Step 7: Update standings (for round robin/swiss).
    async fn step_update_standings(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        match_: &TournamentMatch,
        bracket: &TournamentBracket,
    ) -> Result<bool, DomainError> {
        const STEP_NAME: &str = "update_standings";

        // Only update standings for round robin and swiss formats
        let needs_standings = matches!(
            bracket.bracket_type,
            BracketType::RoundRobin | BracketType::Swiss
        );

        if !needs_standings {
            saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({ "action": "not_applicable" })),
                )
                .await?;
            return Ok(false);
        }

        // Recompute the bracket from its completed match rows instead of
        // adding this match's deltas. `step_complete_match` has already
        // persisted the winner/loser on the row, so this result is
        // included — and because the write is a pure function of those
        // rows, a saga re-drive (or an admin approving a review that was
        // already resumed by captain acknowledgment) converges on the same
        // numbers instead of counting the match twice.
        self.standings_repo
            .recompute_bracket(match_.bracket_id)
            .await?;

        saga_coordinator
            .complete_step(
                execution,
                STEP_NAME,
                Some(serde_json::json!({
                    "bracket_type": format!("{:?}", bracket.bracket_type),
                    "standings_updated": true
                })),
            )
            .await?;

        info!(
            bracket_id = %match_.bracket_id,
            "Standings updated"
        );

        Ok(true)
    }

    /// Step 8: Update player stats (best-effort).
    ///
    /// This step is non-critical — if it fails, we log and continue.
    /// Stats can be recalculated later from match history.
    async fn step_update_player_stats(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: &MatchCompletionInput,
    ) {
        const STEP_NAME: &str = "update_player_stats";

        // Skip for forfeits (no meaningful stats to update)
        if input.is_forfeit {
            let _ = saga_coordinator
                .complete_step(
                    execution,
                    STEP_NAME,
                    Some(serde_json::json!({"action": "skipped_forfeit"})),
                )
                .await;
            return;
        }

        match self
            .stats_updater
            .update_player_stats(
                input.match_id,
                input.winner_registration_id,
                input.loser_registration_id,
                input.is_forfeit,
            )
            .await
        {
            Ok(()) => {
                let _ = saga_coordinator
                    .complete_step(
                        execution,
                        STEP_NAME,
                        Some(serde_json::json!({"action": "stats_updated"})),
                    )
                    .await;
                info!(
                    match_id = %input.match_id,
                    "Player stats updated"
                );
            }
            Err(e) => {
                // Log but don't fail the saga — stats are non-critical
                warn!(
                    match_id = %input.match_id,
                    error = %e,
                    "Failed to update player stats (non-critical)"
                );
                let _ = saga_coordinator
                    .complete_step(
                        execution,
                        STEP_NAME,
                        Some(serde_json::json!({
                            "action": "failed_non_critical",
                            "error": e.to_string()
                        })),
                    )
                    .await;
            }
        }
    }

    // =========================================================================
    // COMPENSATION
    // =========================================================================

    /// Check if compensation should be attempted.
    fn should_compensate(&self, execution: &SagaExecution) -> bool {
        // Only compensate if we got past the validation step
        execution.current_step > 0
    }

    /// Compensate for failed saga.
    async fn compensate(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
    ) -> Result<(), DomainError> {
        saga_coordinator.start_compensation(execution).await?;

        // Compensation is complex for match completion - we'd need to:
        // 1. Revert match status
        // 2. Remove participant from next match
        // 3. Revert standings updates
        //
        // For now, we just mark compensation as needed for manual review
        warn!(
            saga_id = %execution.id,
            "Match completion saga requires manual compensation review"
        );

        // Delete progression logs for this saga
        if let Ok(logs) = self.progression_log_repo.find_by_saga(execution.id).await {
            info!(
                saga_id = %execution.id,
                log_count = logs.len(),
                "Found progression logs for compensation"
            );
            // Note: actual deletion would depend on business requirements
        }

        saga_coordinator.complete_compensation(execution).await?;

        Ok(())
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    /// Get tournament ID from match.
    async fn get_tournament_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<portal_core::TournamentId, DomainError> {
        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(match_id))?;
        Ok(match_.tournament_id)
    }

    fn definition(&self) -> SagaDefinition {
        SagaDefinition::new("match_completion", 2).with_max_retries(3)
    }

    /// Determine which slot (1 or 2) a participant should go to in the target match.
    async fn determine_target_slot(
        &self,
        source_match: &TournamentMatch,
        target_match_id: TournamentMatchId,
        _is_winner: bool,
    ) -> Result<ParticipantSlot, DomainError> {
        // Get the target match to check sources
        let target_match = self
            .match_repo
            .find_by_id(target_match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(target_match_id))?;

        // Check which slot expects input from this match
        // Check participant 1 source
        if let Some(MatchParticipantSource::WinnerOf(pos) | MatchParticipantSource::LoserOf(pos)) =
            &target_match.participant1_source
            && pos == &source_match.bracket_position
        {
            return Ok(ParticipantSlot::One);
        }

        // Check participant 2 source
        if let Some(MatchParticipantSource::WinnerOf(pos) | MatchParticipantSource::LoserOf(pos)) =
            &target_match.participant2_source
            && pos == &source_match.bracket_position
        {
            return Ok(ParticipantSlot::Two);
        }

        // Default to slot 1 if neither slot is assigned (shouldn't happen in well-formed brackets)
        if target_match.participant1_registration_id.is_none() {
            Ok(ParticipantSlot::One)
        } else {
            Ok(ParticipantSlot::Two)
        }
    }

    fn build_summary(
        &self,
        winner_advanced: bool,
        loser_dropped: bool,
        standings_updated: bool,
        bracket: &TournamentBracket,
    ) -> String {
        let mut parts = Vec::new();

        parts.push("Match completed".to_string());

        if winner_advanced {
            parts.push("winner advanced to next match".to_string());
        } else {
            parts.push("winner crowned as champion".to_string());
        }

        if loser_dropped {
            parts.push("loser dropped to losers bracket".to_string());
        } else if matches!(
            bracket.bracket_type,
            BracketType::SingleElim | BracketType::Winners | BracketType::Losers
        ) {
            parts.push("loser eliminated".to_string());
        }

        if standings_updated {
            parts.push("standings updated".to_string());
        }

        parts.join(", ")
    }
}

#[async_trait]
impl<TMR, TBR, TRR, TSTR, SR, PLR, MDV, RC, MSU> Saga
    for MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR, MDV, RC, MSU>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingsRepository,
    SR: SagaExecutionRepository,
    PLR: ProgressionLogRepository,
    MDV: MatchDemoValidator,
    RC: ReviewCreator,
    MSU: MatchStatsUpdater,
{
    type Input = MatchCompletionInput;
    type Output = MatchCompletionOutput;

    fn definition(&self) -> SagaDefinition {
        self.definition()
    }

    async fn execute(&self, input: Self::Input) -> Result<SagaResult<Self::Output>, DomainError> {
        self.execute_completion(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_completion_input_creation() {
        let input = MatchCompletionInput {
            match_id: TournamentMatchId::new(),
            winner_registration_id: TournamentRegistrationId::new(),
            loser_registration_id: TournamentRegistrationId::new(),
            winner_score: 2,
            loser_score: 1,
            is_forfeit: false,
            saga_id: None,
            result_claim_id: None,
        };

        assert_eq!(input.winner_score, 2);
        assert_eq!(input.loser_score, 1);
        assert!(!input.is_forfeit);
        assert!(input.saga_id.is_none());
    }

    #[test]
    fn test_match_completion_input_forfeit() {
        let input = MatchCompletionInput {
            match_id: TournamentMatchId::new(),
            winner_registration_id: TournamentRegistrationId::new(),
            loser_registration_id: TournamentRegistrationId::new(),
            winner_score: 0,
            loser_score: 0,
            is_forfeit: true,
            saga_id: Some(SagaId::new()),
            result_claim_id: None,
        };

        assert!(input.is_forfeit);
        assert!(input.saga_id.is_some());
    }

    #[test]
    fn test_match_completion_output_creation() {
        let output = MatchCompletionOutput {
            match_id: TournamentMatchId::new(),
            winner_next_match_id: Some(TournamentMatchId::new()),
            loser_next_match_id: None,
            standings_updated: false,
            review_pending: false,
            review_id: None,
            summary: "Match completed".to_string(),
        };

        assert!(output.winner_next_match_id.is_some());
        assert!(output.loser_next_match_id.is_none());
        assert!(!output.standings_updated);
    }

    #[test]
    fn test_match_completion_output_with_standings() {
        let output = MatchCompletionOutput {
            match_id: TournamentMatchId::new(),
            winner_next_match_id: None,
            loser_next_match_id: None,
            standings_updated: true,
            review_pending: false,
            review_id: None,
            summary: "Match completed, standings updated".to_string(),
        };

        assert!(output.standings_updated);
        assert!(output.summary.contains("standings"));
    }

    #[test]
    fn test_match_completion_input_serialization() {
        let input = MatchCompletionInput {
            match_id: TournamentMatchId::new(),
            winner_registration_id: TournamentRegistrationId::new(),
            loser_registration_id: TournamentRegistrationId::new(),
            winner_score: 3,
            loser_score: 2,
            is_forfeit: false,
            saga_id: None,
            result_claim_id: None,
        };

        // Test that serialization works
        let json = serde_json::to_string(&input).expect("Should serialize");
        assert!(json.contains("winner_score"));
        assert!(json.contains("loser_score"));

        // Test deserialization
        let deserialized: MatchCompletionInput =
            serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(deserialized.winner_score, 3);
        assert_eq!(deserialized.loser_score, 2);
    }

    #[test]
    fn test_match_completion_output_serialization() {
        let output = MatchCompletionOutput {
            match_id: TournamentMatchId::new(),
            winner_next_match_id: Some(TournamentMatchId::new()),
            loser_next_match_id: Some(TournamentMatchId::new()),
            standings_updated: true,
            review_pending: false,
            review_id: None,
            summary: "All done".to_string(),
        };

        let json = serde_json::to_string(&output).expect("Should serialize");
        assert!(json.contains("standings_updated"));
        assert!(json.contains("All done"));
    }
}
