//! Shared helper functions for tournament bracket setup.
//!
//! These are extracted from `TournamentService` so they can be reused
//! by both `TournamentService` (initial bracket generation) and
//! `ProgressionService` (stage advancement).

use std::collections::HashMap;

use portal_core::{DomainError, TournamentMatchId, TournamentRegistrationId};

use crate::entities::tournament::TournamentMatch;
use crate::repositories::tournament::{ParticipantSlot, TournamentMatchRepository};

use super::bracket_generator::{ByeInfo, InitialAssignment};

/// Build a position → match ID mapping from a list of matches.
pub fn build_position_map(matches: &[TournamentMatch]) -> HashMap<String, TournamentMatchId> {
    matches
        .iter()
        .map(|m| (m.bracket_position.clone(), m.id))
        .collect()
}

/// Apply initial participant assignments to matches.
pub async fn apply_initial_assignments<TMR: TournamentMatchRepository>(
    match_repo: &TMR,
    assignments: &[InitialAssignment],
    position_to_match: &HashMap<String, TournamentMatchId>,
) -> Result<(), DomainError> {
    for assignment in assignments {
        if let Some(&match_id) = position_to_match.get(&assignment.bracket_position) {
            let slot = if assignment.slot == 1 {
                ParticipantSlot::One
            } else {
                ParticipantSlot::Two
            };

            match_repo
                .assign_participant(
                    match_id,
                    slot,
                    assignment.participant.registration_id,
                    assignment.participant.participant_name.clone(),
                    assignment.participant.participant_logo_url.clone(),
                    Some(assignment.participant.seed),
                )
                .await?;
        }
    }
    Ok(())
}

/// Apply bye advancements (participants who auto-advance).
pub async fn apply_byes<TMR: TournamentMatchRepository>(
    match_repo: &TMR,
    byes: &[ByeInfo],
    position_to_match: &HashMap<String, TournamentMatchId>,
) -> Result<(), DomainError> {
    for bye in byes {
        if let Some(&match_id) = position_to_match.get(&bye.advances_to_position) {
            let slot = if bye.slot == 1 {
                ParticipantSlot::One
            } else {
                ParticipantSlot::Two
            };

            match_repo
                .assign_participant(
                    match_id,
                    slot,
                    bye.participant.registration_id,
                    bye.participant.participant_name.clone(),
                    bye.participant.participant_logo_url.clone(),
                    Some(bye.participant.seed),
                )
                .await?;
        }
    }
    Ok(())
}

/// Set SE progression links: R{r}M{m} winner → R{r+1}M{ceil(m/2)}.
pub async fn set_se_progression_links<TMR: TournamentMatchRepository>(
    match_repo: &TMR,
    matches: &[TournamentMatch],
    position_to_match: &HashMap<String, TournamentMatchId>,
) -> Result<(), DomainError> {
    for match_ in matches {
        let pos = &match_.bracket_position;
        let parts: Vec<&str> = pos.split('M').collect();
        if parts.len() != 2 {
            continue;
        }
        let round: i32 = parts[0].trim_start_matches('R').parse().unwrap_or(0);
        let match_in_round: i32 = parts[1].parse().unwrap_or(0);
        if round == 0 || match_in_round == 0 {
            continue;
        }

        let next_round = round + 1;
        let next_match_in_round = (match_in_round + 1) / 2;
        let next_pos = format!("R{next_round}M{next_match_in_round}");

        if let Some(&next_match_id) = position_to_match.get(&next_pos) {
            match_repo
                .set_progression_links(match_.id, Some(next_match_id), None)
                .await?;
        }
    }
    Ok(())
}

/// Parse a bracket position like "WR2M3" or "LR1M2" into (round, match_in_round).
/// Returns (0, 0) if parsing fails.
pub fn parse_round_match(position: &str, prefix: &str) -> (i32, i32) {
    let stripped = position.strip_prefix(prefix).unwrap_or("");
    let parts: Vec<&str> = stripped.split('M').collect();
    if parts.len() != 2 {
        return (0, 0);
    }
    let round: i32 = parts[0].parse().unwrap_or(0);
    let match_in_round: i32 = parts[1].parse().unwrap_or(0);
    (round, match_in_round)
}

// =============================================================================
// RESULT OUTCOME DERIVATION
// =============================================================================

/// Derive `(winner, loser)` registrations for a match from a pair of scores.
///
/// The single place that answers "given these two numbers, who won?". Used by
/// `DisputeService::resolve_adjusted` and by the admin score-override added
/// for P-72, so an admin correction and a dispute adjustment cannot disagree
/// about the outcome they imply.
///
/// A **tie is rejected**. The open-coded version this replaced was
/// `if p1 > p2 { participant1 } else { participant2 }`, which silently handed
/// an equal-score adjustment to participant 2 and then wrote that fabricated
/// winner into the bracket. There is no match format in the product where a
/// draw is a legal recorded result, so "cannot determine a winner" is the
/// honest answer.
pub fn derive_result_outcome(
    match_: &TournamentMatch,
    participant1_score: i32,
    participant2_score: i32,
) -> Result<(TournamentRegistrationId, TournamentRegistrationId), DomainError> {
    let (Some(p1), Some(p2)) = (
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ) else {
        return Err(DomainError::InvalidState(
            "Match does not have two participants".to_string(),
        ));
    };

    match participant1_score.cmp(&participant2_score) {
        std::cmp::Ordering::Greater => Ok((p1, p2)),
        std::cmp::Ordering::Less => Ok((p2, p1)),
        std::cmp::Ordering::Equal => Err(DomainError::InvalidState(format!(
            "Cannot determine a winner from a tied score ({participant1_score}-{participant2_score})"
        ))),
    }
}
