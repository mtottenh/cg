//! Transactional match completion executor.
//!
//! Performs match completion operations atomically within a single database transaction.
//! This ensures that either all operations succeed or all are rolled back.

use portal_core::types::{BracketType, MatchParticipantSource, TournamentMatchStatus};
use portal_core::{DomainError, TournamentBracketId, TournamentMatchId, TournamentRegistrationId};
use tracing::{info, instrument};

use crate::adapters::tournament::{
    PgTournamentBracketRepository, PgTournamentMatchRepository, PgTournamentRegistrationRepository,
    PgTournamentStandingsRepository,
};
use crate::transaction::DbTransaction;
use portal_domain::entities::tournament::{TournamentBracket, TournamentMatch};
use portal_domain::repositories::tournament::{ParticipantSlot, UpdateTournamentStanding};

// =============================================================================
// INPUT/OUTPUT TYPES
// =============================================================================

/// Input for transactional match completion.
#[derive(Debug, Clone)]
pub struct MatchCompletionTxInput {
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
}

/// Output of transactional match completion.
#[derive(Debug, Clone)]
pub struct MatchCompletionTxOutput {
    /// The completed match.
    pub match_id: TournamentMatchId,
    /// Next match ID for the winner (if any).
    pub winner_next_match_id: Option<TournamentMatchId>,
    /// Next match ID for the loser (if any).
    pub loser_next_match_id: Option<TournamentMatchId>,
    /// Whether standings were updated.
    pub standings_updated: bool,
}

// =============================================================================
// TRANSACTIONAL MATCH COMPLETION
// =============================================================================

/// Executes match completion within a single database transaction.
///
/// This function performs all match completion operations atomically:
/// 1. Validates the match state
/// 2. Completes the match with result
/// 3. Advances winner to next match (if applicable)
/// 4. Routes loser (to losers bracket or eliminates)
/// 5. Updates standings (for round robin/swiss formats)
///
/// If any step fails, the entire transaction is rolled back.
#[instrument(skip(tx))]
pub async fn complete_match_in_transaction(
    tx: &mut DbTransaction<'_>,
    input: MatchCompletionTxInput,
) -> Result<MatchCompletionTxOutput, DomainError> {
    // Step 1: Validate and get match state
    let match_ = validate_match(tx, &input).await?;

    // Step 2: Get bracket info
    let bracket = get_bracket(tx, match_.bracket_id).await?;

    // Step 3: Complete the match with result
    complete_match(tx, &match_, &input).await?;

    // Step 4: Advance winner to next match
    let winner_next_match_id = advance_winner(tx, &match_, &input).await?;

    // Step 5: Route loser
    let loser_next_match_id = route_loser(tx, &match_, &input, &bracket).await?;

    // Step 6: Update standings (if applicable)
    let standings_updated = update_standings(tx, &match_, &bracket, &input).await?;

    info!(
        match_id = %input.match_id,
        winner = %input.winner_registration_id,
        "Match completed in transaction"
    );

    Ok(MatchCompletionTxOutput {
        match_id: input.match_id,
        winner_next_match_id,
        loser_next_match_id,
        standings_updated,
    })
}

// =============================================================================
// INTERNAL STEPS
// =============================================================================

/// Validate the match state for completion.
async fn validate_match(
    tx: &mut DbTransaction<'_>,
    input: &MatchCompletionTxInput,
) -> Result<TournamentMatch, DomainError> {
    let match_ = PgTournamentMatchRepository::find_by_id_in_tx(tx, input.match_id)
        .await?
        .ok_or_else(|| DomainError::TournamentMatchNotFound(input.match_id.to_string()))?;

    // Validate match is in valid state for completion
    // The match must be in an active state that allows result submission
    let can_complete = matches!(
        match_.status,
        TournamentMatchStatus::InProgress | TournamentMatchStatus::AwaitingResult
    );
    if !can_complete {
        return Err(DomainError::InvalidState(format!(
            "Match in {} status cannot be completed",
            match_.status
        )));
    }

    // Validate participants
    let p1 = match_.participant1_registration_id;
    let p2 = match_.participant2_registration_id;

    let valid_winner =
        p1 == Some(input.winner_registration_id) || p2 == Some(input.winner_registration_id);
    let valid_loser =
        p1 == Some(input.loser_registration_id) || p2 == Some(input.loser_registration_id);

    if !valid_winner || !valid_loser {
        return Err(DomainError::InvalidState(
            "Winner or loser is not a participant in this match".to_string(),
        ));
    }

    // Validate they are different
    if input.winner_registration_id == input.loser_registration_id {
        return Err(DomainError::InvalidState(
            "Winner and loser cannot be the same".to_string(),
        ));
    }

    Ok(match_)
}

/// Get bracket information.
async fn get_bracket(
    tx: &mut DbTransaction<'_>,
    bracket_id: TournamentBracketId,
) -> Result<TournamentBracket, DomainError> {
    PgTournamentBracketRepository::find_by_id_in_tx(tx, bracket_id)
        .await?
        .ok_or_else(|| DomainError::TournamentBracketNotFound(bracket_id.to_string()))
}

/// Complete the match with result.
async fn complete_match(
    tx: &mut DbTransaction<'_>,
    match_: &TournamentMatch,
    input: &MatchCompletionTxInput,
) -> Result<(), DomainError> {
    // Determine scores for participant1 and participant2
    let (p1_score, p2_score) =
        if match_.participant1_registration_id == Some(input.winner_registration_id) {
            (input.winner_score, input.loser_score)
        } else {
            (input.loser_score, input.winner_score)
        };

    // Submit the result and complete the match atomically
    PgTournamentMatchRepository::submit_result_in_tx(
        tx,
        input.match_id,
        p1_score,
        p2_score,
        input.winner_registration_id,
        input.loser_registration_id,
    )
    .await?;

    Ok(())
}

/// Advance winner to next match.
async fn advance_winner(
    tx: &mut DbTransaction<'_>,
    match_: &TournamentMatch,
    input: &MatchCompletionTxInput,
) -> Result<Option<TournamentMatchId>, DomainError> {
    let Some(next_match_id) = match_.winner_progresses_to else {
        // No next match - this was the final
        return Ok(None);
    };

    // Get the registration info for populating the next match
    let registration =
        PgTournamentRegistrationRepository::find_by_id_in_tx(tx, input.winner_registration_id)
            .await?
            .ok_or_else(|| {
                DomainError::TournamentRegistrationNotFound(
                    input.winner_registration_id.to_string(),
                )
            })?;

    // Determine which slot the winner goes to
    let target_slot = determine_target_slot(tx, match_, next_match_id, true).await?;

    // Assign the winner to the next match
    PgTournamentMatchRepository::assign_participant_in_tx(
        tx,
        next_match_id,
        target_slot,
        input.winner_registration_id,
        registration.participant_name.clone(),
        registration.participant_logo_url.clone(),
        registration.seed,
    )
    .await?;

    info!(
        match_id = %input.match_id,
        next_match_id = %next_match_id,
        winner = %input.winner_registration_id,
        "Winner advanced in transaction"
    );

    Ok(Some(next_match_id))
}

/// Route loser (to losers bracket or eliminate).
async fn route_loser(
    tx: &mut DbTransaction<'_>,
    match_: &TournamentMatch,
    input: &MatchCompletionTxInput,
    _bracket: &TournamentBracket,
) -> Result<Option<TournamentMatchId>, DomainError> {
    let Some(loser_match_id) = match_.loser_progresses_to else {
        // No loser bracket - loser is eliminated (or not applicable for round robin)
        return Ok(None);
    };

    // Get the registration info
    let registration =
        PgTournamentRegistrationRepository::find_by_id_in_tx(tx, input.loser_registration_id)
            .await?
            .ok_or_else(|| {
                DomainError::TournamentRegistrationNotFound(input.loser_registration_id.to_string())
            })?;

    // Determine which slot the loser goes to
    let target_slot = determine_target_slot(tx, match_, loser_match_id, false).await?;

    // Assign the loser to the losers bracket match
    PgTournamentMatchRepository::assign_participant_in_tx(
        tx,
        loser_match_id,
        target_slot,
        input.loser_registration_id,
        registration.participant_name.clone(),
        registration.participant_logo_url.clone(),
        registration.seed,
    )
    .await?;

    info!(
        match_id = %input.match_id,
        loser_match_id = %loser_match_id,
        loser = %input.loser_registration_id,
        "Loser dropped to losers bracket in transaction"
    );

    Ok(Some(loser_match_id))
}

/// Update standings for round robin/swiss formats.
async fn update_standings(
    tx: &mut DbTransaction<'_>,
    match_: &TournamentMatch,
    bracket: &TournamentBracket,
    input: &MatchCompletionTxInput,
) -> Result<bool, DomainError> {
    // Only update standings for round robin and swiss formats
    let needs_standings = matches!(
        bracket.bracket_type,
        BracketType::RoundRobin | BracketType::Swiss
    );

    if !needs_standings {
        return Ok(false);
    }

    // Update winner standings
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
    PgTournamentStandingsRepository::update_after_match_in_tx(tx, winner_update).await?;

    // Update loser standings
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
    PgTournamentStandingsRepository::update_after_match_in_tx(tx, loser_update).await?;

    // Recalculate positions
    PgTournamentStandingsRepository::recalculate_positions_in_tx(tx, match_.bracket_id).await?;

    info!(bracket_id = %match_.bracket_id, "Standings updated in transaction");

    Ok(true)
}

/// Determine which slot (1 or 2) a participant should go to in the target match.
async fn determine_target_slot(
    tx: &mut DbTransaction<'_>,
    source_match: &TournamentMatch,
    target_match_id: TournamentMatchId,
    _is_winner: bool,
) -> Result<ParticipantSlot, DomainError> {
    let target_match = PgTournamentMatchRepository::find_by_id_in_tx(tx, target_match_id)
        .await?
        .ok_or_else(|| DomainError::TournamentMatchNotFound(target_match_id.to_string()))?;

    // Check which slot expects input from this match
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

    // Default to slot 1 if neither slot is assigned
    if target_match.participant1_registration_id.is_none() {
        Ok(ParticipantSlot::One)
    } else {
        Ok(ParticipantSlot::Two)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_completion_tx_input_creation() {
        let input = MatchCompletionTxInput {
            match_id: TournamentMatchId::new(),
            winner_registration_id: TournamentRegistrationId::new(),
            loser_registration_id: TournamentRegistrationId::new(),
            winner_score: 2,
            loser_score: 1,
        };

        assert_eq!(input.winner_score, 2);
        assert_eq!(input.loser_score, 1);
    }

    #[test]
    fn test_match_completion_tx_output_creation() {
        let output = MatchCompletionTxOutput {
            match_id: TournamentMatchId::new(),
            winner_next_match_id: Some(TournamentMatchId::new()),
            loser_next_match_id: None,
            standings_updated: false,
        };

        assert!(output.winner_next_match_id.is_some());
        assert!(output.loser_next_match_id.is_none());
        assert!(!output.standings_updated);
    }
}
