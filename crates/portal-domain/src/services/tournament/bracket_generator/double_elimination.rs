//! Double elimination bracket generation.

use super::single_elimination::{generate_seeding_order, next_power_of_two};
use super::{BracketGenerator, ByeInfo, GeneratedBracket, InitialAssignment};
use crate::entities::tournament::SeededParticipant;
use crate::repositories::tournament::CreateTournamentMatch;
use portal_core::types::{MatchFormat, MatchParticipantSource};
use portal_core::{DomainError, TournamentBracketId, TournamentId, TournamentStageId};

/// Generated double elimination bracket structure.
#[derive(Debug, Clone)]
pub struct GeneratedDoubleElimination {
    /// Winners bracket.
    pub winners_bracket: GeneratedBracket,
    /// Losers bracket.
    pub losers_bracket: GeneratedBracket,
    /// Grand final bracket (single match).
    pub grand_final: GeneratedBracket,
    /// Cross-bracket progression links.
    pub cross_bracket_links: Vec<CrossBracketLink>,
}

/// A cross-bracket progression link.
#[derive(Debug, Clone)]
pub struct CrossBracketLink {
    /// Source bracket position (e.g., "WR1M1").
    pub source_bracket_position: String,
    /// Target bracket position (e.g., "LR1M1").
    pub target_bracket_position: String,
    /// Type of link.
    pub link_type: CrossLinkType,
}

/// Type of cross-bracket link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrossLinkType {
    /// Loser of source drops to target (WB → LB).
    LoserDropsTo,
    /// Winner of source advances to target (WB/LB final → GF).
    WinnerAdvancesTo,
}

impl BracketGenerator {
    /// Generate a double elimination bracket.
    ///
    /// Creates three brackets: Winners, Losers, and Grand Final.
    /// - Winners bracket: standard single-elimination; losers drop to losers bracket.
    /// - Losers bracket: second-chance bracket; losers are eliminated.
    /// - Grand final: single match between WB champion and LB champion.
    #[allow(clippy::too_many_arguments)]
    pub fn double_elimination(
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        wb_bracket_id: TournamentBracketId,
        lb_bracket_id: TournamentBracketId,
        gf_bracket_id: TournamentBracketId,
        participants: Vec<SeededParticipant>,
        match_format: MatchFormat,
    ) -> Result<GeneratedDoubleElimination, DomainError> {
        let participant_count = participants.len();

        if participant_count < 2 {
            return Err(DomainError::InsufficientParticipants);
        }

        // Calculate bracket size (next power of 2)
        let bracket_size = next_power_of_two(participant_count);
        let wb_rounds = (bracket_size as f64).log2() as i32;

        // =====================================================================
        // WINNERS BRACKET
        // =====================================================================
        let seeding_order = generate_seeding_order(bracket_size);

        let mut wb_matches = Vec::new();
        let mut match_number = 1;

        for round in 1..=wb_rounds {
            let matches_in_round = bracket_size / (1 << round);

            for match_idx in 0..matches_in_round {
                let bracket_position = format!("WR{round}M{}", match_idx + 1);

                let (participant1_source, participant2_source) = if round == 1 {
                    let seed_idx1 = seeding_order[match_idx * 2];
                    let seed_idx2 = seeding_order[match_idx * 2 + 1];
                    (
                        Some(MatchParticipantSource::Seed(seed_idx1 as i32 + 1)),
                        Some(MatchParticipantSource::Seed(seed_idx2 as i32 + 1)),
                    )
                } else {
                    let prev_round = round - 1;
                    let prev_match1 = match_idx * 2 + 1;
                    let prev_match2 = match_idx * 2 + 2;
                    (
                        Some(MatchParticipantSource::WinnerOf(format!(
                            "WR{prev_round}M{prev_match1}"
                        ))),
                        Some(MatchParticipantSource::WinnerOf(format!(
                            "WR{prev_round}M{prev_match2}"
                        ))),
                    )
                };

                wb_matches.push(CreateTournamentMatch {
                    bracket_id: wb_bracket_id,
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
                    winner_progresses_to: None,
                    loser_progresses_to: None,
                });

                match_number += 1;
            }
        }

        // Generate initial assignments and byes for winners bracket
        let mut initial_assignments = Vec::new();
        let mut byes = Vec::new();

        for (match_idx, seeding_pair) in seeding_order.chunks(2).enumerate() {
            let seed_idx1 = seeding_pair[0];
            let seed_idx2 = seeding_pair[1];
            let bracket_position = format!("WR1M{}", match_idx + 1);

            let participant1 = participants.get(seed_idx1).cloned();
            let participant2 = participants.get(seed_idx2).cloned();

            match (participant1, participant2) {
                (Some(p1), Some(p2)) => {
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
                    let advances_to_position = format!("WR2M{}", (match_idx / 2) + 1);
                    let slot = if match_idx % 2 == 0 { 1 } else { 2 };
                    byes.push(ByeInfo {
                        participant: p,
                        advances_to_position,
                        slot,
                    });
                }
                (None, Some(p)) => {
                    let advances_to_position = format!("WR2M{}", (match_idx / 2) + 1);
                    let slot = if match_idx % 2 == 0 { 1 } else { 2 };
                    byes.push(ByeInfo {
                        participant: p,
                        advances_to_position,
                        slot,
                    });
                }
                (None, None) => {}
            }
        }

        // =====================================================================
        // LOSERS BRACKET
        // =====================================================================
        let lb_rounds = 2 * (wb_rounds - 1);
        let mut lb_matches = Vec::new();
        let mut lb_match_number = 1;
        let mut cross_bracket_links = Vec::new();

        let mut lb_round_match_counts: Vec<usize> = Vec::new();

        for lb_round in 1..=lb_rounds {
            let matches_in_round = Self::lb_matches_in_round(lb_round, wb_rounds);
            lb_round_match_counts.push(matches_in_round);

            for match_idx in 0..matches_in_round {
                let bracket_position = format!("LR{lb_round}M{}", match_idx + 1);

                let (participant1_source, participant2_source) =
                    Self::lb_participant_sources(lb_round, match_idx, wb_rounds);

                lb_matches.push(CreateTournamentMatch {
                    bracket_id: lb_bracket_id,
                    stage_id,
                    tournament_id,
                    round: lb_round,
                    match_number: lb_match_number,
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
                    winner_progresses_to: None,
                    loser_progresses_to: None,
                });

                lb_match_number += 1;
            }
        }

        // =====================================================================
        // CROSS-BRACKET LINKS: WB losers → LB
        // =====================================================================
        let wr1_match_count = bracket_size / 2;
        let lr1_match_count = Self::lb_matches_in_round(1, wb_rounds);

        let cross_seed_map = Self::cross_seed_wr1_to_lr1(wr1_match_count, lr1_match_count);
        for (wr1_match_idx, lr1_match_idx, slot) in &cross_seed_map {
            cross_bracket_links.push(CrossBracketLink {
                source_bracket_position: format!("WR1M{}", wr1_match_idx + 1),
                target_bracket_position: format!("LR1M{}", lr1_match_idx + 1),
                link_type: CrossLinkType::LoserDropsTo,
            });
            let _slot = *slot;
        }

        for wb_round in 2..=wb_rounds {
            let lb_dropper_round = 2 * (wb_round - 1);
            let wb_matches_in_round = bracket_size / (1 << wb_round);
            let lb_matches_in_dropper = Self::lb_matches_in_round(lb_dropper_round, wb_rounds);

            for match_idx in 0..wb_matches_in_round {
                let lb_match_idx = if lb_matches_in_dropper == wb_matches_in_round {
                    wb_matches_in_round - 1 - match_idx
                } else {
                    match_idx
                };

                cross_bracket_links.push(CrossBracketLink {
                    source_bracket_position: format!("WR{wb_round}M{}", match_idx + 1),
                    target_bracket_position: format!(
                        "LR{lb_dropper_round}M{}",
                        lb_match_idx + 1
                    ),
                    link_type: CrossLinkType::LoserDropsTo,
                });
            }
        }

        // =====================================================================
        // GRAND FINAL
        // =====================================================================
        let gf_match = CreateTournamentMatch {
            bracket_id: gf_bracket_id,
            stage_id,
            tournament_id,
            round: 1,
            match_number: 1,
            bracket_position: "GFM1".to_string(),
            participant1_registration_id: None,
            participant2_registration_id: None,
            participant1_name: None,
            participant1_logo_url: None,
            participant1_seed: None,
            participant2_name: None,
            participant2_logo_url: None,
            participant2_seed: None,
            participant1_source: Some(MatchParticipantSource::WinnerOf(format!(
                "WR{wb_rounds}M1"
            ))),
            participant2_source: Some(MatchParticipantSource::WinnerOf(format!(
                "LR{lb_rounds}M1"
            ))),
            match_format,
            maps_required: match_format.wins_required(),
            winner_progresses_to: None,
            loser_progresses_to: None,
        };

        cross_bracket_links.push(CrossBracketLink {
            source_bracket_position: format!("WR{wb_rounds}M1"),
            target_bracket_position: "GFM1".to_string(),
            link_type: CrossLinkType::WinnerAdvancesTo,
        });

        cross_bracket_links.push(CrossBracketLink {
            source_bracket_position: format!("LR{lb_rounds}M1"),
            target_bracket_position: "GFM1".to_string(),
            link_type: CrossLinkType::WinnerAdvancesTo,
        });

        Ok(GeneratedDoubleElimination {
            winners_bracket: GeneratedBracket {
                total_rounds: wb_rounds,
                matches: wb_matches,
                initial_assignments,
                byes,
            },
            losers_bracket: GeneratedBracket {
                total_rounds: lb_rounds,
                matches: lb_matches,
                initial_assignments: Vec::new(),
                byes: Vec::new(),
            },
            grand_final: GeneratedBracket {
                total_rounds: 1,
                matches: vec![gf_match],
                initial_assignments: Vec::new(),
                byes: Vec::new(),
            },
            cross_bracket_links,
        })
    }

    /// Calculate the number of matches in a losers bracket round.
    pub(crate) fn lb_matches_in_round(lb_round: i32, wb_rounds: i32) -> usize {
        let bracket_size = 1usize << wb_rounds;
        let wr1_matches = bracket_size / 2;

        if lb_round == 1 {
            return wr1_matches / 2;
        }

        let pair_index = (lb_round - 1) / 2;
        let base = wr1_matches / 2;
        base >> pair_index as usize
    }

    /// Determine participant sources for a losers bracket match.
    fn lb_participant_sources(
        lb_round: i32,
        match_idx: usize,
        wb_rounds: i32,
    ) -> (Option<MatchParticipantSource>, Option<MatchParticipantSource>) {
        if lb_round == 1 {
            let wr1_matches = (1usize << wb_rounds) / 2;
            let p1_wr1_idx = match_idx;
            let p2_wr1_idx = wr1_matches - 1 - match_idx;
            (
                Some(MatchParticipantSource::LoserOf(format!(
                    "WR1M{}",
                    p1_wr1_idx + 1
                ))),
                Some(MatchParticipantSource::LoserOf(format!(
                    "WR1M{}",
                    p2_wr1_idx + 1
                ))),
            )
        } else if lb_round % 2 == 0 {
            let prev_lb_round = lb_round - 1;
            let wb_source_round = lb_round / 2 + 1;
            let wb_matches_in_round = (1usize << wb_rounds) / (1 << wb_source_round);

            let p1_source = Some(MatchParticipantSource::WinnerOf(format!(
                "LR{prev_lb_round}M{}",
                match_idx + 1
            )));

            let wb_match_idx = if wb_matches_in_round
                == Self::lb_matches_in_round(lb_round, wb_rounds)
            {
                wb_matches_in_round - 1 - match_idx
            } else {
                match_idx
            };
            let p2_source = Some(MatchParticipantSource::LoserOf(format!(
                "WR{wb_source_round}M{}",
                wb_match_idx + 1
            )));

            (p1_source, p2_source)
        } else {
            let prev_lb_round = lb_round - 1;
            let p1_match = match_idx * 2 + 1;
            let p2_match = match_idx * 2 + 2;
            (
                Some(MatchParticipantSource::WinnerOf(format!(
                    "LR{prev_lb_round}M{p1_match}"
                ))),
                Some(MatchParticipantSource::WinnerOf(format!(
                    "LR{prev_lb_round}M{p2_match}"
                ))),
            )
        }
    }

    /// Cross-seed WR1 losers into LR1 to avoid immediate rematches.
    fn cross_seed_wr1_to_lr1(
        wr1_match_count: usize,
        lr1_match_count: usize,
    ) -> Vec<(usize, usize, u8)> {
        let mut result = Vec::new();

        for lr1_idx in 0..lr1_match_count {
            let wr1_top = lr1_idx;
            let wr1_bottom = wr1_match_count - 1 - lr1_idx;

            result.push((wr1_top, lr1_idx, 1));
            result.push((wr1_bottom, lr1_idx, 2));
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tournament::bracket_generator::tests::create_test_participants;
    use portal_core::{TournamentBracketId, TournamentId, TournamentStageId};

    fn create_de_result(count: usize) -> GeneratedDoubleElimination {
        let participants = create_test_participants(count);
        BracketGenerator::double_elimination(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            TournamentBracketId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo3,
        )
        .unwrap()
    }

    #[test]
    fn test_double_elimination_4_teams() {
        let result = create_de_result(4);
        assert_eq!(result.winners_bracket.total_rounds, 2);
        assert_eq!(result.winners_bracket.matches.len(), 3);
        assert_eq!(result.losers_bracket.total_rounds, 2);
        assert_eq!(result.losers_bracket.matches.len(), 2);
        assert_eq!(result.grand_final.matches.len(), 1);
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 6);
        assert_eq!(result.winners_bracket.initial_assignments.len(), 4);
        assert_eq!(result.winners_bracket.byes.len(), 0);
    }

    #[test]
    fn test_double_elimination_8_teams() {
        let result = create_de_result(8);
        assert_eq!(result.winners_bracket.total_rounds, 3);
        assert_eq!(result.winners_bracket.matches.len(), 7);
        assert_eq!(result.losers_bracket.total_rounds, 4);
        assert_eq!(result.losers_bracket.matches.len(), 6);
        assert_eq!(result.grand_final.matches.len(), 1);
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 14);
        assert_eq!(result.winners_bracket.initial_assignments.len(), 8);
    }

    #[test]
    fn test_double_elimination_16_teams() {
        let result = create_de_result(16);
        assert_eq!(result.winners_bracket.total_rounds, 4);
        assert_eq!(result.winners_bracket.matches.len(), 15);
        assert_eq!(result.losers_bracket.total_rounds, 6);
        assert_eq!(result.losers_bracket.matches.len(), 14);
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 30);
    }

    #[test]
    fn test_double_elimination_with_byes() {
        let result = create_de_result(6);
        assert_eq!(result.winners_bracket.total_rounds, 3);
        assert_eq!(result.winners_bracket.matches.len(), 7);
        assert_eq!(result.winners_bracket.byes.len(), 2);
        let assigned_count = result.winners_bracket.initial_assignments.len();
        assert_eq!(assigned_count, 4);
        assert_eq!(result.losers_bracket.matches.len(), 6);
    }

    #[test]
    fn test_double_elimination_cross_bracket_links() {
        let result = create_de_result(8);

        let loser_drops: Vec<_> = result
            .cross_bracket_links
            .iter()
            .filter(|l| l.link_type == CrossLinkType::LoserDropsTo)
            .collect();
        let winner_advances: Vec<_> = result
            .cross_bracket_links
            .iter()
            .filter(|l| l.link_type == CrossLinkType::WinnerAdvancesTo)
            .collect();

        assert_eq!(loser_drops.len(), 7);
        assert_eq!(winner_advances.len(), 2);

        let gf_sources: Vec<_> = winner_advances
            .iter()
            .map(|l| l.target_bracket_position.as_str())
            .collect();
        assert!(gf_sources.iter().all(|p| *p == "GFM1"));
    }

    #[test]
    fn test_double_elimination_no_immediate_rematches() {
        let result = create_de_result(8);

        let lr1_matches: Vec<_> = result
            .losers_bracket
            .matches
            .iter()
            .filter(|m| m.round == 1)
            .collect();

        assert_eq!(lr1_matches.len(), 2);

        let lr1m1 = &lr1_matches[0];
        let p1 = lr1m1.participant1_source.as_ref().unwrap();
        let p2 = lr1m1.participant2_source.as_ref().unwrap();

        if let (MatchParticipantSource::LoserOf(pos1), MatchParticipantSource::LoserOf(pos2)) =
            (p1, p2)
        {
            assert_ne!(pos1, pos2);
            let is_top = |p: &str| p == "WR1M1" || p == "WR1M2";
            let is_bottom = |p: &str| p == "WR1M3" || p == "WR1M4";
            assert!(
                (is_top(pos1) && is_bottom(pos2)) || (is_bottom(pos1) && is_top(pos2)),
                "LR1M1 should pair losers from opposite bracket halves, got {pos1} vs {pos2}"
            );
        } else {
            panic!("LR1 match sources should be LoserOf");
        }
    }

    #[test]
    fn test_double_elimination_participant_sources() {
        let result = create_de_result(8);

        for m in result.losers_bracket.matches.iter().filter(|m| m.round == 1) {
            assert!(matches!(m.participant1_source, Some(MatchParticipantSource::LoserOf(_))));
            assert!(matches!(m.participant2_source, Some(MatchParticipantSource::LoserOf(_))));
        }

        for m in result.losers_bracket.matches.iter().filter(|m| m.round == 2) {
            assert!(matches!(m.participant1_source, Some(MatchParticipantSource::WinnerOf(_))));
            assert!(matches!(m.participant2_source, Some(MatchParticipantSource::LoserOf(_))));
        }

        for m in result.losers_bracket.matches.iter().filter(|m| m.round == 3) {
            assert!(matches!(m.participant1_source, Some(MatchParticipantSource::WinnerOf(_))));
            assert!(matches!(m.participant2_source, Some(MatchParticipantSource::WinnerOf(_))));
        }

        let gf = &result.grand_final.matches[0];
        assert!(matches!(gf.participant1_source, Some(MatchParticipantSource::WinnerOf(_))));
        assert!(matches!(gf.participant2_source, Some(MatchParticipantSource::WinnerOf(_))));

        if let Some(MatchParticipantSource::WinnerOf(pos)) = &gf.participant1_source {
            assert_eq!(pos, "WR3M1");
        }
        if let Some(MatchParticipantSource::WinnerOf(pos)) = &gf.participant2_source {
            assert_eq!(pos, "LR4M1");
        }
    }

    #[test]
    fn test_double_elimination_insufficient_participants() {
        let participants = create_test_participants(1);
        let result = BracketGenerator::double_elimination(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            TournamentBracketId::new(),
            TournamentBracketId::new(),
            participants,
            MatchFormat::Bo1,
        );
        assert!(matches!(result, Err(DomainError::InsufficientParticipants)));
    }

    #[test]
    fn test_double_elimination_2_teams() {
        let result = create_de_result(2);
        assert_eq!(result.winners_bracket.matches.len(), 1);
        assert_eq!(result.losers_bracket.matches.len(), 0);
        assert_eq!(result.grand_final.matches.len(), 1);
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_lb_matches_in_round() {
        assert_eq!(BracketGenerator::lb_matches_in_round(1, 3), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(2, 3), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(3, 3), 1);
        assert_eq!(BracketGenerator::lb_matches_in_round(4, 3), 1);

        assert_eq!(BracketGenerator::lb_matches_in_round(1, 4), 4);
        assert_eq!(BracketGenerator::lb_matches_in_round(2, 4), 4);
        assert_eq!(BracketGenerator::lb_matches_in_round(3, 4), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(4, 4), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(5, 4), 1);
        assert_eq!(BracketGenerator::lb_matches_in_round(6, 4), 1);

        assert_eq!(BracketGenerator::lb_matches_in_round(1, 2), 1);
        assert_eq!(BracketGenerator::lb_matches_in_round(2, 2), 1);
    }

    #[test]
    fn test_double_elimination_wb_positions() {
        let result = create_de_result(8);
        let positions: Vec<&str> = result
            .winners_bracket
            .matches
            .iter()
            .map(|m| m.bracket_position.as_str())
            .collect();

        assert!(positions.contains(&"WR1M1"));
        assert!(positions.contains(&"WR1M4"));
        assert!(positions.contains(&"WR2M1"));
        assert!(positions.contains(&"WR2M2"));
        assert!(positions.contains(&"WR3M1"));
    }

    #[test]
    fn test_double_elimination_lb_positions() {
        let result = create_de_result(8);
        let positions: Vec<&str> = result
            .losers_bracket
            .matches
            .iter()
            .map(|m| m.bracket_position.as_str())
            .collect();

        assert!(positions.contains(&"LR1M1"));
        assert!(positions.contains(&"LR1M2"));
        assert!(positions.contains(&"LR2M1"));
        assert!(positions.contains(&"LR2M2"));
        assert!(positions.contains(&"LR3M1"));
        assert!(positions.contains(&"LR4M1"));
        assert_eq!(positions.len(), 6);
    }
}
