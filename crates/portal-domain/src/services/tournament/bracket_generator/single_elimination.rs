//! Single elimination bracket generation.

use super::{BracketGenerator, ByeInfo, GeneratedBracket, InitialAssignment};
use crate::entities::tournament::SeededParticipant;
use crate::repositories::tournament::CreateTournamentMatch;
use portal_core::types::{MatchFormat, MatchParticipantSource};
use portal_core::{DomainError, TournamentBracketId, TournamentId, TournamentStageId};

impl BracketGenerator {
    /// Generate a single elimination bracket.
    ///
    /// # Arguments
    /// * `tournament_id` - The tournament ID
    /// * `stage_id` - The stage ID
    /// * `bracket_id` - The bracket ID
    /// * `participants` - Seeded participants (should be sorted by seed)
    /// * `match_format` - The default match format
    ///
    /// # Returns
    /// A `GeneratedBracket` containing all matches and assignments.
    pub fn single_elimination(
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        bracket_id: TournamentBracketId,
        participants: Vec<SeededParticipant>,
        match_format: MatchFormat,
    ) -> Result<GeneratedBracket, DomainError> {
        let participant_count = participants.len();

        if participant_count < 2 {
            return Err(DomainError::InsufficientParticipants);
        }

        // Calculate bracket size (next power of 2)
        let bracket_size = next_power_of_two(participant_count);
        let total_rounds = (bracket_size as f64).log2() as i32;

        // Generate seeding order for proper bracket placement
        let seeding_order = generate_seeding_order(bracket_size);

        // Create all matches
        let mut matches = Vec::new();
        let mut match_number = 1;

        // Generate matches for each round
        for round in 1..=total_rounds {
            let matches_in_round = bracket_size / (1 << round);

            for match_idx in 0..matches_in_round {
                let bracket_position = format!("R{round}M{}", match_idx + 1);

                // Determine participant sources
                let (participant1_source, participant2_source) = if round == 1 {
                    // First round: from seeding
                    let seed_idx1 = seeding_order[match_idx * 2];
                    let seed_idx2 = seeding_order[match_idx * 2 + 1];

                    (
                        Some(MatchParticipantSource::Seed(seed_idx1 as i32 + 1)),
                        Some(MatchParticipantSource::Seed(seed_idx2 as i32 + 1)),
                    )
                } else {
                    // Later rounds: winners from previous matches
                    let prev_round = round - 1;
                    let prev_match1 = match_idx * 2 + 1;
                    let prev_match2 = match_idx * 2 + 2;

                    (
                        Some(MatchParticipantSource::WinnerOf(format!(
                            "R{prev_round}M{prev_match1}"
                        ))),
                        Some(MatchParticipantSource::WinnerOf(format!(
                            "R{prev_round}M{prev_match2}"
                        ))),
                    )
                };

                matches.push(CreateTournamentMatch {
                    bracket_id,
                    stage_id,
                    tournament_id,
                    round,
                    match_number,
                    bracket_position,
                    participant1_registration_id: None,
                    participant2_registration_id: None,
                    participant1_name: None,
                    participant1_logo_url: None,
                    participant1_seed: None,
                    participant2_name: None,
                    participant2_logo_url: None,
                    participant2_seed: None,
                    participant1_source,
                    participant2_source,
                    match_format,
                    maps_required: match_format.wins_required(),
                    winner_progresses_to: None, // Will be set after match IDs are created
                    loser_progresses_to: None,
                });

                match_number += 1;
            }
        }

        // Generate initial assignments (for first round)
        let mut initial_assignments = Vec::new();
        let mut byes = Vec::new();

        for (match_idx, seeding_pair) in seeding_order.chunks(2).enumerate() {
            let seed_idx1 = seeding_pair[0];
            let seed_idx2 = seeding_pair[1];
            let bracket_position = format!("R1M{}", match_idx + 1);

            // Check if we have participants for these seed positions
            let participant1 = participants.get(seed_idx1).cloned();
            let participant2 = participants.get(seed_idx2).cloned();

            match (participant1, participant2) {
                (Some(p1), Some(p2)) => {
                    // Both participants present - normal match
                    initial_assignments.push(InitialAssignment {
                        bracket_position: bracket_position.clone(),
                        participant: p1,
                        slot: 1,
                    });
                    initial_assignments.push(InitialAssignment {
                        bracket_position,
                        participant: p2,
                        slot: 2,
                    });
                }
                (Some(p), None) => {
                    // P1 present, P2 is bye - P1 advances
                    let advances_to_position = format!("R2M{}", (match_idx / 2) + 1);
                    let slot = if match_idx % 2 == 0 { 1 } else { 2 };

                    byes.push(ByeInfo {
                        participant: p,
                        advances_to_position,
                        slot,
                    });
                }
                (None, Some(p)) => {
                    // P2 present, P1 is bye - P2 advances
                    let advances_to_position = format!("R2M{}", (match_idx / 2) + 1);
                    let slot = if match_idx % 2 == 0 { 1 } else { 2 };

                    byes.push(ByeInfo {
                        participant: p,
                        advances_to_position,
                        slot,
                    });
                }
                (None, None) => {
                    // Both positions empty (shouldn't happen with proper seeding)
                }
            }
        }

        Ok(GeneratedBracket {
            total_rounds,
            matches,
            initial_assignments,
            byes,
        })
    }
}

/// Get the next power of 2 >= n.
pub(super) const fn next_power_of_two(n: usize) -> usize {
    if n <= 1 {
        return 2;
    }

    let mut power = 1;
    while power < n {
        power *= 2;
    }
    power
}

/// Generate seeding order for a bracket of given size.
///
/// This creates a proper seeding bracket where:
/// - Seed 1 plays lowest seed in their quarter
/// - Seeds meet in expected order (1v2 in finals if both win out)
///
/// For a 16-team bracket, this returns:
/// [0, 15, 8, 7, 4, 11, 12, 3, 2, 13, 10, 5, 6, 9, 14, 1]
/// Which maps to seeds: [1, 16, 9, 8, 5, 12, 13, 4, 3, 14, 11, 6, 7, 10, 15, 2]
pub(super) fn generate_seeding_order(bracket_size: usize) -> Vec<usize> {
    if bracket_size == 2 {
        return vec![0, 1];
    }

    let mut result = vec![0usize; bracket_size];

    // Use standard bracket seeding
    let half = bracket_size / 2;
    for i in 0..half {
        // Standard seeding: 1v16, 8v9, 5v12, 4v13, etc.
        let pos1 = i * 2;
        let pos2 = i * 2 + 1;

        // Calculate seed positions using standard bracket seeding formula
        let (seed1, seed2) = get_standard_seeding_pair(i, half);
        result[pos1] = seed1;
        result[pos2] = seed2;
    }

    result
}

/// Get the standard seeding pair for a given match index.
fn get_standard_seeding_pair(match_idx: usize, total_matches: usize) -> (usize, usize) {
    let bracket_size = total_matches * 2;

    // Generate the standard seeding order
    let order = standard_bracket_order(total_matches);
    let high_seed = order[match_idx];
    let low_seed = bracket_size - 1 - high_seed;

    (high_seed, low_seed)
}

/// Generate the standard bracket order for high seeds.
fn standard_bracket_order(num_matches: usize) -> Vec<usize> {
    if num_matches == 1 {
        return vec![0];
    }

    let prev = standard_bracket_order(num_matches / 2);
    let mut result = Vec::with_capacity(num_matches);

    for &seed in &prev {
        result.push(seed);
        result.push(num_matches - 1 - seed);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tournament::bracket_generator::tests::create_test_participants;
    use portal_core::{TournamentBracketId, TournamentId, TournamentStageId};

    #[test]
    fn test_next_power_of_two() {
        assert_eq!(next_power_of_two(1), 2);
        assert_eq!(next_power_of_two(2), 2);
        assert_eq!(next_power_of_two(3), 4);
        assert_eq!(next_power_of_two(4), 4);
        assert_eq!(next_power_of_two(5), 8);
        assert_eq!(next_power_of_two(8), 8);
        assert_eq!(next_power_of_two(9), 16);
        assert_eq!(next_power_of_two(16), 16);
    }

    #[test]
    fn test_seeding_order_4_teams() {
        let order = generate_seeding_order(4);
        assert_eq!(order.len(), 4);
        assert_eq!(order[0], 0); // Seed 1
        assert_eq!(order[1], 3); // Seed 4
        assert_eq!(order[2], 1); // Seed 2 (or could be 2)
    }

    #[test]
    fn test_seeding_order_8_teams() {
        let order = generate_seeding_order(8);
        assert_eq!(order.len(), 8);
        assert_eq!(order[0], 0); // Seed 1
        assert_eq!(order[1], 7); // Seed 8
    }

    #[test]
    fn test_single_elimination_4_teams() {
        let participants = create_test_participants(4);
        let result = BracketGenerator::single_elimination(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo3,
        )
        .unwrap();

        assert_eq!(result.total_rounds, 2);
        assert_eq!(result.matches.len(), 3);
        assert_eq!(result.initial_assignments.len(), 4);
        assert_eq!(result.byes.len(), 0);
    }

    #[test]
    fn test_single_elimination_with_byes() {
        let participants = create_test_participants(5);
        let result = BracketGenerator::single_elimination(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo1,
        )
        .unwrap();

        assert_eq!(result.total_rounds, 3);
        assert_eq!(result.matches.len(), 7);
        assert_eq!(result.byes.len(), 3);
    }

    #[test]
    fn test_single_elimination_insufficient_participants() {
        let participants = create_test_participants(1);
        let result = BracketGenerator::single_elimination(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo1,
        );

        assert!(matches!(result, Err(DomainError::InsufficientParticipants)));
    }
}
