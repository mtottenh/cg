//! Swiss System bracket generation.

use super::{BracketGenerator, GeneratedBracket, InitialAssignment};
use crate::entities::tournament::SeededParticipant;
use crate::repositories::tournament::CreateTournamentMatch;
use portal_core::types::{MatchFormat, MatchParticipantSource};
use portal_core::{
    DomainError, TournamentBracketId, TournamentId, TournamentRegistrationId, TournamentStageId,
};

/// A participant's standing for Swiss pairing.
#[derive(Debug, Clone)]
pub struct SwissParticipantStanding {
    /// Registration ID.
    pub registration_id: TournamentRegistrationId,
    /// Participant display name.
    pub participant_name: String,
    /// Participant logo URL.
    pub participant_logo_url: Option<String>,
    /// Seed number.
    pub seed: i32,
    /// Total points accumulated.
    pub points: i32,
    /// Buchholz tiebreaker score.
    pub buchholz_score: f64,
    /// Whether this participant already had a bye in a previous round.
    pub had_bye: bool,
}

impl BracketGenerator {
    /// Generate the first Swiss round (seeded pairing: top half vs bottom half).
    ///
    /// Pairing: seed 1 vs seed N/2+1, seed 2 vs seed N/2+2, etc.
    /// Odd participant count: lowest seed gets a bye (no match created).
    pub fn swiss_initial_round(
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        bracket_id: TournamentBracketId,
        participants: Vec<SeededParticipant>,
        match_format: MatchFormat,
    ) -> Result<(GeneratedBracket, Option<TournamentRegistrationId>), DomainError> {
        let n = participants.len();
        if n < 2 {
            return Err(DomainError::InsufficientParticipants);
        }

        let is_odd = !n.is_multiple_of(2);
        let bye_participant = if is_odd {
            Some(participants[n - 1].registration_id)
        } else {
            None
        };

        let pairs_count = n / 2;
        let half = pairs_count;

        let mut matches = Vec::new();
        let mut initial_assignments = Vec::new();
        let mut match_number = 1;

        for i in 0..half {
            let p1 = &participants[i];
            let p2 = &participants[i + half];
            let bracket_position = format!("SW1M{match_number}");

            matches.push(CreateTournamentMatch {
                bracket_id,
                stage_id,
                tournament_id,
                round: 1,
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
        }

        Ok((
            GeneratedBracket {
                total_rounds: 1,
                matches,
                initial_assignments,
                byes: Vec::new(),
            },
            bye_participant,
        ))
    }

    /// Generate a subsequent Swiss round based on standings.
    ///
    /// Pairing algorithm:
    /// 1. Sort participants by points (desc), then Buchholz (desc), then seed (asc)
    /// 2. Group by points (score groups)
    /// 3. Within each score group, pair top vs bottom
    /// 4. If a pairing would be a rematch, slide down to next available opponent
    /// 5. Unpaired participants carry down to next score group
    /// 6. Odd participant out: lowest-ranked unpaired who hasn't had a bye gets a bye
    pub fn swiss_next_round(
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        bracket_id: TournamentBracketId,
        round_number: i32,
        standings: Vec<SwissParticipantStanding>,
        completed_pairings: &[(TournamentRegistrationId, TournamentRegistrationId)],
        match_format: MatchFormat,
    ) -> Result<(GeneratedBracket, Option<TournamentRegistrationId>), DomainError> {
        if standings.len() < 2 {
            return Err(DomainError::InsufficientParticipants);
        }

        // Sort: by points desc, buchholz desc, seed asc
        let mut sorted: Vec<SwissParticipantStanding> = standings;
        sorted.sort_by(|a, b| {
            b.points
                .cmp(&a.points)
                .then_with(|| {
                    b.buchholz_score
                        .partial_cmp(&a.buchholz_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.seed.cmp(&b.seed))
        });

        // Build set of completed pairings for fast lookup
        let mut pairing_set = std::collections::HashSet::new();
        for (a, b) in completed_pairings {
            pairing_set.insert((*a, *b));
            pairing_set.insert((*b, *a));
        }

        // Handle bye for odd count
        let mut bye_participant = None;
        if !sorted.len().is_multiple_of(2) {
            // Find the lowest-ranked participant who hasn't had a bye yet
            for i in (0..sorted.len()).rev() {
                if !sorted[i].had_bye {
                    bye_participant = Some(sorted[i].registration_id);
                    sorted.remove(i);
                    break;
                }
            }
            // If everyone has had a bye, give it to the lowest-ranked
            if bye_participant.is_none() {
                let last = sorted.len() - 1;
                bye_participant = Some(sorted[last].registration_id);
                sorted.remove(last);
            }
        }

        // Pair participants using greedy matching with rematch avoidance
        let mut paired = vec![false; sorted.len()];
        let mut pairings: Vec<(usize, usize)> = Vec::new();

        for i in 0..sorted.len() {
            if paired[i] {
                continue;
            }
            // Find the best unpaired opponent (closest in ranking)
            for j in (i + 1)..sorted.len() {
                if paired[j] {
                    continue;
                }
                let is_rematch =
                    pairing_set.contains(&(sorted[i].registration_id, sorted[j].registration_id));
                if !is_rematch {
                    paired[i] = true;
                    paired[j] = true;
                    pairings.push((i, j));
                    break;
                }
            }
        }

        // If any participant is still unpaired (all opponents were rematches),
        // pair them with the closest unpaired opponent anyway
        let unpaired: Vec<usize> = (0..sorted.len()).filter(|i| !paired[*i]).collect();
        for chunk in unpaired.chunks(2) {
            if chunk.len() == 2 {
                pairings.push((chunk[0], chunk[1]));
            }
        }

        // Generate matches
        let mut matches = Vec::new();
        let mut initial_assignments = Vec::new();
        let mut match_number = 1;

        for &(i, j) in &pairings {
            let p1 = &sorted[i];
            let p2 = &sorted[j];
            let bracket_position = format!("SW{round_number}M{match_number}");

            let p1_participant = SeededParticipant {
                registration_id: p1.registration_id,
                seed: p1.seed,
                participant_name: p1.participant_name.clone(),
                participant_logo_url: p1.participant_logo_url.clone(),
            };
            let p2_participant = SeededParticipant {
                registration_id: p2.registration_id,
                seed: p2.seed,
                participant_name: p2.participant_name.clone(),
                participant_logo_url: p2.participant_logo_url.clone(),
            };

            matches.push(CreateTournamentMatch {
                bracket_id,
                stage_id,
                tournament_id,
                round: round_number,
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
                participant: p1_participant,
                slot: 1,
            });
            initial_assignments.push(InitialAssignment {
                bracket_position,
                participant: p2_participant,
                slot: 2,
            });

            match_number += 1;
        }

        Ok((
            GeneratedBracket {
                total_rounds: round_number,
                matches,
                initial_assignments,
                byes: Vec::new(),
            },
            bye_participant,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tournament::bracket_generator::tests::create_test_participants;
    use portal_core::{
        TournamentBracketId, TournamentId, TournamentRegistrationId, TournamentStageId,
    };

    #[test]
    fn test_swiss_initial_round_8_teams() {
        let participants = create_test_participants(8);
        let (result, bye) = BracketGenerator::swiss_initial_round(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo3,
        )
        .unwrap();

        // 8 teams: 4 matches, no bye
        assert_eq!(result.matches.len(), 4);
        assert!(bye.is_none());
        assert_eq!(result.initial_assignments.len(), 8);

        // Verify top-half vs bottom-half pairing
        // Match 1: seed 1 vs seed 5
        // Match 2: seed 2 vs seed 6
        // Match 3: seed 3 vs seed 7
        // Match 4: seed 4 vs seed 8
        for m in &result.matches {
            assert!(m.bracket_position.starts_with("SW1M"));
        }
    }

    #[test]
    fn test_swiss_initial_round_odd_teams() {
        let participants = create_test_participants(7);
        let reg_ids: Vec<_> = participants.iter().map(|p| p.registration_id).collect();

        let (result, bye) = BracketGenerator::swiss_initial_round(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo3,
        )
        .unwrap();

        // 7 teams: 3 matches + 1 bye participant
        assert_eq!(result.matches.len(), 3);
        assert!(bye.is_some());

        // Bye should be the lowest seed (seed 7 = last participant)
        assert_eq!(bye.unwrap(), reg_ids[6]);
    }

    #[test]
    fn test_swiss_next_round_avoids_rematches() {
        let reg_ids: Vec<TournamentRegistrationId> =
            (0..4).map(|_| TournamentRegistrationId::new()).collect();

        let standings: Vec<SwissParticipantStanding> = vec![
            SwissParticipantStanding {
                registration_id: reg_ids[0],
                participant_name: "Team 1".to_string(),
                participant_logo_url: None,
                seed: 1,
                points: 3,
                buchholz_score: 0.0,
                had_bye: false,
            },
            SwissParticipantStanding {
                registration_id: reg_ids[1],
                participant_name: "Team 2".to_string(),
                participant_logo_url: None,
                seed: 2,
                points: 3,
                buchholz_score: 0.0,
                had_bye: false,
            },
            SwissParticipantStanding {
                registration_id: reg_ids[2],
                participant_name: "Team 3".to_string(),
                participant_logo_url: None,
                seed: 3,
                points: 0,
                buchholz_score: 0.0,
                had_bye: false,
            },
            SwissParticipantStanding {
                registration_id: reg_ids[3],
                participant_name: "Team 4".to_string(),
                participant_logo_url: None,
                seed: 4,
                points: 0,
                buchholz_score: 0.0,
                had_bye: false,
            },
        ];

        // R1 pairings: 1v3 and 2v4 (completed)
        let completed_pairings = vec![(reg_ids[0], reg_ids[2]), (reg_ids[1], reg_ids[3])];

        let (result, bye) = BracketGenerator::swiss_next_round(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            2,
            standings,
            &completed_pairings,
            MatchFormat::Bo3,
        )
        .unwrap();

        assert_eq!(result.matches.len(), 2);
        assert!(bye.is_none());

        // Verify no rematches
        for chunk in result.initial_assignments.chunks(2) {
            let id1 = chunk[0].participant.registration_id;
            let id2 = chunk[1].participant.registration_id;
            assert!(
                !completed_pairings.contains(&(id1, id2))
                    && !completed_pairings.contains(&(id2, id1)),
                "Rematch detected: {id1} vs {id2}"
            );
        }
    }

    #[test]
    fn test_swiss_next_round_pairs_by_points() {
        let reg_ids: Vec<TournamentRegistrationId> =
            (0..4).map(|_| TournamentRegistrationId::new()).collect();

        // After R1: Teams 1,2 have 3 pts, Teams 3,4 have 0 pts
        let standings: Vec<SwissParticipantStanding> = vec![
            SwissParticipantStanding {
                registration_id: reg_ids[0],
                participant_name: "Team 1".to_string(),
                participant_logo_url: None,
                seed: 1,
                points: 3,
                buchholz_score: 0.0,
                had_bye: false,
            },
            SwissParticipantStanding {
                registration_id: reg_ids[1],
                participant_name: "Team 2".to_string(),
                participant_logo_url: None,
                seed: 2,
                points: 3,
                buchholz_score: 0.0,
                had_bye: false,
            },
            SwissParticipantStanding {
                registration_id: reg_ids[2],
                participant_name: "Team 3".to_string(),
                participant_logo_url: None,
                seed: 3,
                points: 0,
                buchholz_score: 0.0,
                had_bye: false,
            },
            SwissParticipantStanding {
                registration_id: reg_ids[3],
                participant_name: "Team 4".to_string(),
                participant_logo_url: None,
                seed: 4,
                points: 0,
                buchholz_score: 0.0,
                had_bye: false,
            },
        ];

        let completed_pairings = vec![(reg_ids[0], reg_ids[2]), (reg_ids[1], reg_ids[3])];

        let (result, _) = BracketGenerator::swiss_next_round(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            2,
            standings,
            &completed_pairings,
            MatchFormat::Bo3,
        )
        .unwrap();

        // Should pair 1v2 (both 3 pts) and 3v4 (both 0 pts)
        assert_eq!(result.matches.len(), 2);

        let m1_ids: Vec<_> = result.initial_assignments[0..2]
            .iter()
            .map(|a| a.participant.registration_id)
            .collect();

        // First match should be the two 3-point teams
        assert!(
            (m1_ids.contains(&reg_ids[0]) && m1_ids.contains(&reg_ids[1]))
                || (m1_ids.contains(&reg_ids[2]) && m1_ids.contains(&reg_ids[3])),
            "Equal-point teams should be paired together"
        );
    }

    #[test]
    fn test_swiss_initial_round_insufficient() {
        let participants = create_test_participants(1);
        let result = BracketGenerator::swiss_initial_round(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo1,
        );
        assert!(matches!(result, Err(DomainError::InsufficientParticipants)));
    }
}
