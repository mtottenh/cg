//! Match completion saga.
//!
//! Orchestrates the multi-step process of completing a match:
//! 1. Validate the match result
//! 2. Update match status to completed
//! 3. Advance winner to next match
//! 4. Route loser (to losers bracket or eliminate)
//! 5. Update standings (for round robin/swiss formats)

use std::sync::Arc;

use async_trait::async_trait;
use portal_core::types::{BracketType, TournamentMatchStatus};
use portal_core::{DomainError, SagaId, TournamentMatchId, TournamentRegistrationId};
use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument, warn};

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
/// - Match completion
/// - Winner advancement to next match
/// - Loser routing (elimination or losers bracket)
/// - Standings updates for round robin/swiss
pub struct MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR> {
    match_repo: Arc<TMR>,
    bracket_repo: Arc<TBR>,
    registration_repo: Arc<TRR>,
    standings_repo: Arc<TSTR>,
    saga_repo: Arc<SR>,
    progression_log_repo: Arc<PLR>,
}

impl<TMR, TBR, TRR, TSTR, SR, PLR> MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingsRepository,
    SR: SagaExecutionRepository,
    PLR: ProgressionLogRepository,
{
    /// Create a new match completion saga.
    pub fn new(
        match_repo: Arc<TMR>,
        bracket_repo: Arc<TBR>,
        registration_repo: Arc<TRR>,
        standings_repo: Arc<TSTR>,
        saga_repo: Arc<SR>,
        progression_log_repo: Arc<PLR>,
    ) -> Self {
        Self {
            match_repo,
            bracket_repo,
            registration_repo,
            standings_repo,
            saga_repo,
            progression_log_repo,
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
        let result = self.run_steps(&saga_coordinator, &mut execution, input).await;

        match result {
            Ok(output) => {
                saga_coordinator.complete_saga(&mut execution).await?;
                Ok(SagaResult::success(execution, output))
            }
            Err(e) => {
                // Attempt compensation if needed
                if self.should_compensate(&execution) {
                    if let Err(comp_err) = self.compensate(&saga_coordinator, &mut execution).await {
                        error!(
                            saga_id = %execution.id,
                            error = %comp_err,
                            "Compensation failed"
                        );
                    }
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
        let match_ = self.step_validate_match(saga_coordinator, execution, &input).await?;

        // Step 2: Get bracket info
        let bracket = self.step_get_bracket(saga_coordinator, execution, &match_).await?;

        // Step 3: Complete the match
        self.step_complete_match(saga_coordinator, execution, &input).await?;

        // Step 4: Advance winner
        let winner_next_match_id = self
            .step_advance_winner(saga_coordinator, execution, &match_, &input)
            .await?;

        // Step 5: Route loser
        let loser_next_match_id = self
            .step_route_loser(saga_coordinator, execution, &match_, &input, &bracket)
            .await?;

        // Step 6: Update standings (if applicable)
        let standings_updated = self
            .step_update_standings(saga_coordinator, execution, &match_, &bracket, &input)
            .await?;

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
            .ok_or_else(|| {
                DomainError::TournamentMatchNotFound(input.match_id.to_string())
            })?;

        // Validate match is in valid state for completion
        if !match_.status.can_transition_to(TournamentMatchStatus::Completed) {
            let err = format!(
                "Match in {} status cannot be completed",
                match_.status
            );
            saga_coordinator
                .fail_step(execution, STEP_NAME, &err)
                .await?;
            return Err(DomainError::InvalidState(err));
        }

        // Validate participants
        let p1 = match_.participant1_registration_id;
        let p2 = match_.participant2_registration_id;

        let valid_winner = p1 == Some(input.winner_registration_id)
            || p2 == Some(input.winner_registration_id);
        let valid_loser = p1 == Some(input.loser_registration_id)
            || p2 == Some(input.loser_registration_id);

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

    /// Step 2: Get bracket information.
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
            .ok_or_else(|| {
                DomainError::TournamentBracketNotFound(match_.bracket_id.to_string())
            })?;

        saga_coordinator
            .complete_step(execution, STEP_NAME, None)
            .await?;

        Ok(bracket)
    }

    /// Step 3: Complete the match.
    async fn step_complete_match(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        input: &MatchCompletionInput,
    ) -> Result<(), DomainError> {
        const STEP_NAME: &str = "complete_match";

        // Determine scores for participant1 and participant2
        let match_ = self
            .match_repo
            .find_by_id(input.match_id)
            .await?
            .ok_or_else(|| {
                DomainError::TournamentMatchNotFound(input.match_id.to_string())
            })?;

        let (p1_score, p2_score) =
            if match_.participant1_registration_id == Some(input.winner_registration_id) {
                (input.winner_score, input.loser_score)
            } else {
                (input.loser_score, input.winner_score)
            };

        // Submit the result
        self.match_repo
            .submit_result(
                input.match_id,
                p1_score,
                p2_score,
                input.winner_registration_id,
                input.loser_registration_id,
            )
            .await?;

        // Complete the match
        self.match_repo.complete(input.match_id).await?;

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

    /// Step 4: Advance winner to next match.
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
            .ok_or_else(|| {
                DomainError::TournamentRegistrationNotFound(
                    input.winner_registration_id.to_string(),
                )
            })?;

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
                target_position: Some(if target_slot == ParticipantSlot::One { 1 } else { 2 }),
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

    /// Step 5: Route loser.
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
            .ok_or_else(|| {
                DomainError::TournamentRegistrationNotFound(
                    input.loser_registration_id.to_string(),
                )
            })?;

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
                target_position: Some(if target_slot == ParticipantSlot::One { 1 } else { 2 }),
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

    /// Step 6: Update standings (for round robin/swiss).
    async fn step_update_standings(
        &self,
        saga_coordinator: &SagaCoordinator<SR>,
        execution: &mut SagaExecution,
        match_: &TournamentMatch,
        bracket: &TournamentBracket,
        input: &MatchCompletionInput,
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

        // Update standings using delta-based update
        use crate::repositories::tournament::UpdateTournamentStanding;

        // Update winner
        let winner_update = UpdateTournamentStanding {
            bracket_id: match_.bracket_id,
            registration_id: input.winner_registration_id,
            matches_won_delta: 1,
            matches_lost_delta: 0,
            matches_drawn_delta: 0,
            game_wins_delta: input.winner_score,
            game_losses_delta: input.loser_score,
            points_delta: 3, // 3 points for a win
        };
        self.standings_repo.update_after_match(winner_update).await?;

        // Update loser
        let loser_update = UpdateTournamentStanding {
            bracket_id: match_.bracket_id,
            registration_id: input.loser_registration_id,
            matches_won_delta: 0,
            matches_lost_delta: 1,
            matches_drawn_delta: 0,
            game_wins_delta: input.loser_score,
            game_losses_delta: input.winner_score,
            points_delta: 0,
        };
        self.standings_repo.update_after_match(loser_update).await?;

        // Recalculate positions
        self.standings_repo
            .recalculate_positions(match_.bracket_id)
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
        if let Ok(logs) = self
            .progression_log_repo
            .find_by_saga(execution.id)
            .await
        {
            info!(
                saga_id = %execution.id,
                log_count = logs.len(),
                "Found progression logs for compensation"
            );
            // Note: actual deletion would depend on business requirements
        }

        saga_coordinator
            .complete_compensation(execution)
            .await?;

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
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id.to_string()))?;
        Ok(match_.tournament_id)
    }

    fn definition(&self) -> SagaDefinition {
        SagaDefinition::new("match_completion", 1).with_max_retries(3)
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
            .ok_or_else(|| DomainError::TournamentMatchNotFound(target_match_id.to_string()))?;

        // Check which slot expects input from this match
        use portal_core::types::MatchParticipantSource;

        // Check participant 1 source
        if let Some(source) = &target_match.participant1_source {
            match source {
                MatchParticipantSource::WinnerOf(pos) | MatchParticipantSource::LoserOf(pos) => {
                    if pos == &source_match.bracket_position {
                        return Ok(ParticipantSlot::One);
                    }
                }
                _ => {}
            }
        }

        // Check participant 2 source
        if let Some(source) = &target_match.participant2_source {
            match source {
                MatchParticipantSource::WinnerOf(pos) | MatchParticipantSource::LoserOf(pos) => {
                    if pos == &source_match.bracket_position {
                        return Ok(ParticipantSlot::Two);
                    }
                }
                _ => {}
            }
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
impl<TMR, TBR, TRR, TSTR, SR, PLR> Saga for MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingsRepository,
    SR: SagaExecutionRepository,
    PLR: ProgressionLogRepository,
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

// Manual Clone implementation
impl<TMR, TBR, TRR, TSTR, SR, PLR> Clone for MatchCompletionSaga<TMR, TBR, TRR, TSTR, SR, PLR>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingsRepository,
    SR: SagaExecutionRepository,
    PLR: ProgressionLogRepository,
{
    fn clone(&self) -> Self {
        Self {
            match_repo: Arc::clone(&self.match_repo),
            bracket_repo: Arc::clone(&self.bracket_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            standings_repo: Arc::clone(&self.standings_repo),
            saga_repo: Arc::clone(&self.saga_repo),
            progression_log_repo: Arc::clone(&self.progression_log_repo),
        }
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
            summary: "All done".to_string(),
        };

        let json = serde_json::to_string(&output).expect("Should serialize");
        assert!(json.contains("standings_updated"));
        assert!(json.contains("All done"));
    }
}
