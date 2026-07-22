//! Round Robin bracket generation.

use super::{BracketGenerator, GeneratedBracket, InitialAssignment};
use crate::entities::tournament::SeededParticipant;
use crate::repositories::tournament::CreateTournamentMatch;
use portal_core::types::{MatchFormat, MatchParticipantSource};
use portal_core::{DomainError, TournamentBracketId, TournamentId, TournamentStageId};

impl BracketGenerator {
    /// Generate a round robin bracket.
    ///
    /// Uses the circle method: fix participant at index 0, rotate the rest.
    /// For N participants:
    /// - Even N: N-1 rounds, N/2 matches per round
    /// - Odd N: N rounds, (N-1)/2 matches per round (one bye per round)
    ///
    /// All matches are pre-generated and all participants assigned immediately.
    /// No progression links — standings determine final order.
    pub fn round_robin(
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        bracket_id: TournamentBracketId,
        participants: Vec<SeededParticipant>,
        match_format: MatchFormat,
    ) -> Result<GeneratedBracket, DomainError> {
        let n = participants.len();
        if n < 2 {
            return Err(DomainError::InsufficientParticipants);
        }

        // Circle method: if odd, add a BYE sentinel
        let is_odd = !n.is_multiple_of(2);
        let circle_size = if is_odd { n + 1 } else { n };
        let total_rounds = (circle_size - 1) as i32;

        // Build the rotating list (indices into participants; circle_size-1 = BYE sentinel if odd)
        let mut circle: Vec<usize> = (0..circle_size).collect();

        let mut matches = Vec::new();
        let mut initial_assignments = Vec::new();
        let mut match_number = 1;

        for round in 1..=total_rounds {
            let mut round_match = 1;
            let half = circle_size / 2;

            for i in 0..half {
                let idx_a = circle[i];
                let idx_b = circle[circle_size - 1 - i];

                // Skip if either is the BYE sentinel
                if is_odd && (idx_a == n || idx_b == n) {
                    continue;
                }

                let bracket_position = format!("RR{round}M{round_match}");

                let p1 = &participants[idx_a];
                let p2 = &participants[idx_b];

                matches.push(CreateTournamentMatch {
                    bracket_id,
                    stage_id,
                    tournament_id,
                    round,
                    match_number,
                    bracket_position: bracket_position.clone(),
                    participant1_registration_id: None,
                    participant2_registration_id: None,
                    participant1_name: None,
                    participant1_logo_url: None,
                    participant1_seed: None,
                    participant2_name: None,
                    participant2_logo_url: None,
                    participant2_seed: None,
                    participant1_source: Some(MatchParticipantSource::Seed(p1.seed)),
                    participant2_source: Some(MatchParticipantSource::Seed(p2.seed)),
                    match_format,
                    maps_required: match_format.wins_required(),
                    winner_progresses_to: None,
                    loser_progresses_to: None,
                });

                initial_assignments.push(InitialAssignment {
                    bracket_position: bracket_position.clone(),
                    participant: p1.clone(),
                    slot: 1,
                });
                initial_assignments.push(InitialAssignment {
                    bracket_position,
                    participant: p2.clone(),
                    slot: 2,
                });

                match_number += 1;
                round_match += 1;
            }

            // Rotate: fix circle[0], rotate the rest clockwise
            // [0, 1, 2, 3, 4] → [0, 4, 1, 2, 3]
            let last = circle[circle_size - 1];
            for i in (2..circle_size).rev() {
                circle[i] = circle[i - 1];
            }
            circle[1] = last;
        }

        Ok(GeneratedBracket {
            total_rounds,
            matches,
            initial_assignments,
            byes: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tournament::bracket_generator::tests::create_test_participants;
    use portal_core::{TournamentBracketId, TournamentId, TournamentStageId};

    fn create_rr_result(count: usize) -> GeneratedBracket {
        let participants = create_test_participants(count);
        BracketGenerator::round_robin(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo3,
        )
        .unwrap()
    }

    #[test]
    fn test_round_robin_4_teams() {
        let result = create_rr_result(4);

        // 4 teams: 3 rounds, 6 matches total (N*(N-1)/2 = 4*3/2 = 6)
        assert_eq!(result.total_rounds, 3);
        assert_eq!(result.matches.len(), 6);
        // Each match has 2 assignments
        assert_eq!(result.initial_assignments.len(), 12);
        assert!(result.byes.is_empty());
    }

    #[test]
    fn test_round_robin_5_teams() {
        let result = create_rr_result(5);

        // 5 teams (odd): 5 rounds, 10 matches total (5*4/2 = 10)
        // Each round has 2 matches (one participant has a bye)
        assert_eq!(result.total_rounds, 5);
        assert_eq!(result.matches.len(), 10);
        assert_eq!(result.initial_assignments.len(), 20);
    }

    #[test]
    fn test_round_robin_3_teams() {
        let result = create_rr_result(3);

        // 3 teams (odd): 3 rounds, 3 matches total (3*2/2 = 3)
        assert_eq!(result.total_rounds, 3);
        assert_eq!(result.matches.len(), 3);
    }

    #[test]
    fn test_round_robin_8_teams() {
        let result = create_rr_result(8);

        // 8 teams: 7 rounds, 28 matches total (8*7/2 = 28)
        assert_eq!(result.total_rounds, 7);
        assert_eq!(result.matches.len(), 28);
    }

    #[test]
    fn test_round_robin_unique_pairings() {
        let result = create_rr_result(6);

        // Collect all pairings and verify no duplicates
        let mut pairings = std::collections::HashSet::new();
        for assignment_chunk in result.initial_assignments.chunks(2) {
            if assignment_chunk.len() == 2 {
                let id1 = assignment_chunk[0].participant.registration_id;
                let id2 = assignment_chunk[1].participant.registration_id;
                let pair = if id1.to_string() < id2.to_string() {
                    (id1, id2)
                } else {
                    (id2, id1)
                };
                assert!(pairings.insert(pair), "Duplicate pairing found");
            }
        }

        // 6 teams: 15 unique pairings
        assert_eq!(pairings.len(), 15);
    }

    #[test]
    fn test_round_robin_all_participants_assigned() {
        let participants = create_test_participants(4);
        let reg_ids: Vec<_> = participants.iter().map(|p| p.registration_id).collect();

        let result = BracketGenerator::round_robin(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo3,
        )
        .unwrap();

        // Every match should have exactly 2 assignments
        for m in &result.matches {
            let assignments: Vec<_> = result
                .initial_assignments
                .iter()
                .filter(|a| a.bracket_position == m.bracket_position)
                .collect();
            assert_eq!(
                assignments.len(),
                2,
                "Match {} should have 2 assignments",
                m.bracket_position
            );
            // Both should be from our participant set
            assert!(reg_ids.contains(&assignments[0].participant.registration_id));
            assert!(reg_ids.contains(&assignments[1].participant.registration_id));
        }
    }

    #[test]
    fn test_round_robin_insufficient_participants() {
        let participants = create_test_participants(1);
        let result = BracketGenerator::round_robin(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo1,
        );
        assert!(matches!(result, Err(DomainError::InsufficientParticipants)));
    }
}
