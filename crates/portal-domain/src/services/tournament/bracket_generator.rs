//! Bracket generation for tournaments.
//!
//! This module handles generating match structures for various tournament formats.
//! Currently supports:
//! - Single Elimination (Phase 1)
//! - Double Elimination (Phase 5.1)
//!
//! Future formats (planned):
//! - Round Robin
//! - Swiss System
//! - Groups + Playoffs

use crate::entities::tournament::{SeededParticipant, TournamentRegistration};
use crate::repositories::tournament::CreateTournamentMatch;
use portal_core::types::{MatchFormat, MatchParticipantSource};
use portal_core::{DomainError, TournamentBracketId, TournamentId, TournamentStageId};

/// Generated bracket structure ready for database insertion.
#[derive(Debug, Clone)]
pub struct GeneratedBracket {
    /// Total number of rounds in the bracket.
    pub total_rounds: i32,
    /// Match data to create.
    pub matches: Vec<CreateTournamentMatch>,
    /// Matches that need participant assignment after creation.
    /// Maps `bracket_position` to (participant, slot).
    pub initial_assignments: Vec<InitialAssignment>,
    /// Bye information (participants who automatically advance).
    pub byes: Vec<ByeInfo>,
}

/// Initial participant assignment for a match.
#[derive(Debug, Clone)]
pub struct InitialAssignment {
    /// The bracket position (e.g., "R1M1").
    pub bracket_position: String,
    /// The participant to assign.
    pub participant: SeededParticipant,
    /// Which slot (1 or 2) to assign to.
    pub slot: u8,
}

/// Information about a bye (auto-advance).
#[derive(Debug, Clone)]
pub struct ByeInfo {
    /// The participant who receives the bye.
    pub participant: SeededParticipant,
    /// The bracket position they advance to.
    pub advances_to_position: String,
    /// Which slot (1 or 2) they advance to.
    pub slot: u8,
}

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

/// Bracket generator for tournaments.
pub struct BracketGenerator;

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

    /// Generate a double elimination bracket.
    ///
    /// Creates three brackets: Winners, Losers, and Grand Final.
    /// - Winners bracket: standard single-elimination; losers drop to losers bracket.
    /// - Losers bracket: second-chance bracket; losers are eliminated.
    /// - Grand final: single match between WB champion and LB champion.
    ///
    /// # Arguments
    /// * `tournament_id` - The tournament ID
    /// * `stage_id` - The stage ID
    /// * `wb_bracket_id` - Winners bracket ID
    /// * `lb_bracket_id` - Losers bracket ID
    /// * `gf_bracket_id` - Grand final bracket ID
    /// * `participants` - Seeded participants (should be sorted by seed)
    /// * `match_format` - The default match format
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
        // Identical to single elimination, but positions are prefixed with "W".
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

        // Generate initial assignments and byes for winners bracket (same as SE)
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
        // LB has 2 * (wb_rounds - 1) rounds.
        // Odd LB rounds: "survivor" rounds (LB players play each other).
        // Even LB rounds: "dropper" rounds (WB losers enter from above).
        //
        // Exception: LR1 is always a dropper round receiving WR1 losers.
        //
        // For wb_rounds=3 (8-team bracket):
        //   LR1: 2 matches (WR1 losers, cross-seeded)  — dropper
        //   LR2: 2 matches (LR1 winners vs WR2 losers) — dropper
        //   LR3: 1 match  (LR2 winners play each other) — survivor
        //   LR4: 1 match  (LR3 winner vs WR3/WBF loser) — dropper = LB Final
        //
        // For wb_rounds=2 (4-team bracket):
        //   LR1: 1 match  (WR1 losers, cross-seeded)   — dropper
        //   LR2: 1 match  (LR1 winner vs WR2/WBF loser)— dropper = LB Final

        let lb_rounds = 2 * (wb_rounds - 1);
        let mut lb_matches = Vec::new();
        let mut lb_match_number = 1;
        let mut cross_bracket_links = Vec::new();

        // Track matches per LB round for building sources
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
        // WR1 losers → LR1 (cross-seeded to avoid rematches)
        let wr1_match_count = bracket_size / 2;
        let lr1_match_count = Self::lb_matches_in_round(1, wb_rounds);

        // Cross-seeding for WR1 → LR1: pair top-half WR1 losers with bottom-half
        // to avoid immediate rematches. Standard approach: reverse within halves.
        let cross_seed_map = Self::cross_seed_wr1_to_lr1(wr1_match_count, lr1_match_count);
        for (wr1_match_idx, lr1_match_idx, slot) in &cross_seed_map {
            cross_bracket_links.push(CrossBracketLink {
                source_bracket_position: format!("WR1M{}", wr1_match_idx + 1),
                target_bracket_position: format!("LR1M{}", lr1_match_idx + 1),
                link_type: CrossLinkType::LoserDropsTo,
            });
            // Verify the LB match source matches
            let _slot = *slot; // slot 1 or 2 in the LR1 match
        }

        // WR{r} losers → LR dropper rounds (even LB rounds, starting at LR2)
        // WR2 losers → LR2, WR3 losers → LR4, WR{r} losers → LR{2*(r-1)}
        for wb_round in 2..=wb_rounds {
            let lb_dropper_round = 2 * (wb_round - 1);
            let wb_matches_in_round = bracket_size / (1 << wb_round);
            let lb_matches_in_dropper = Self::lb_matches_in_round(lb_dropper_round, wb_rounds);

            // WB losers drop into the dropper round.
            // Cross-seeding: reverse order to avoid bracket region rematches.
            for match_idx in 0..wb_matches_in_round {
                let lb_match_idx = if lb_matches_in_dropper == wb_matches_in_round {
                    // Reverse order for cross-seeding
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

        // WB final winner → GF
        cross_bracket_links.push(CrossBracketLink {
            source_bracket_position: format!("WR{wb_rounds}M1"),
            target_bracket_position: "GFM1".to_string(),
            link_type: CrossLinkType::WinnerAdvancesTo,
        });

        // LB final winner → GF
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
    ///
    /// LB structure for `wb_rounds` WB rounds:
    /// - Total LB rounds: `2 * (wb_rounds - 1)`
    /// - LR1: receives WR1 losers, so match count = WR1 matches / 2
    ///   (two WR1 losers per LR1 match)
    /// - Even LB rounds (dropper): same match count as previous LB round
    ///   (LB survivors + WB droppers pair up 1:1)
    /// - Odd LB rounds > 1 (survivor): half the matches of previous round
    fn lb_matches_in_round(lb_round: i32, wb_rounds: i32) -> usize {
        // For an 8-team bracket (wb_rounds=3, bracket_size=8):
        //   LR1: 2, LR2: 2, LR3: 1, LR4: 1
        // For a 16-team bracket (wb_rounds=4, bracket_size=16):
        //   LR1: 4, LR2: 4, LR3: 2, LR4: 2, LR5: 1, LR6: 1
        // For a 4-team bracket (wb_rounds=2, bracket_size=4):
        //   LR1: 1, LR2: 1

        let bracket_size = 1usize << wb_rounds;
        let wr1_matches = bracket_size / 2;

        // LR1 always has wr1_matches / 2 matches (pairing WR1 losers)
        if lb_round == 1 {
            return wr1_matches / 2;
        }

        // Each pair of LB rounds halves the count:
        // LR1-LR2 have the same count
        // LR3-LR4 have half of LR1-LR2's count
        // LR5-LR6 have half of LR3-LR4's count
        // etc.
        //
        // Round pair index (0-based): (lb_round - 1) / 2
        let pair_index = (lb_round - 1) / 2;
        let base = wr1_matches / 2; // LR1 count
        base >> pair_index as usize
    }

    /// Determine participant sources for a losers bracket match.
    fn lb_participant_sources(
        lb_round: i32,
        match_idx: usize,
        wb_rounds: i32,
    ) -> (Option<MatchParticipantSource>, Option<MatchParticipantSource>) {
        if lb_round == 1 {
            // LR1: both participants come from WR1 losers (cross-seeded)
            // Cross-seeding pairs losers from opposite halves of WR1
            // to avoid immediate rematches.
            let wr1_matches = (1usize << wb_rounds) / 2;
            // Slot 1: from top half of WR1 (match_idx)
            let p1_wr1_idx = match_idx;
            // Slot 2: from bottom half of WR1, reversed
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
            // Even rounds (dropper): one from previous LB round winner, one from WB dropper
            let prev_lb_round = lb_round - 1;
            let wb_source_round = lb_round / 2 + 1; // WR2 drops at LR2, WR3 at LR4, etc.
            let wb_matches_in_round = (1usize << wb_rounds) / (1 << wb_source_round);

            // Slot 1: LB survivor from previous round
            let p1_source = Some(MatchParticipantSource::WinnerOf(format!(
                "LR{prev_lb_round}M{}",
                match_idx + 1
            )));

            // Slot 2: WB dropper (cross-seeded: reversed order)
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
            // Odd rounds > 1 (survivor): both from previous LB round winners
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
    ///
    /// Returns: Vec of (wr1_match_idx, lr1_match_idx, slot) — 0-indexed.
    fn cross_seed_wr1_to_lr1(
        wr1_match_count: usize,
        lr1_match_count: usize,
    ) -> Vec<(usize, usize, u8)> {
        let mut result = Vec::new();

        // Each LR1 match gets two WR1 losers.
        // Standard cross-seeding: pair losers from opposite halves.
        // WR1M1 loser pairs with WR1M{n/2} loser (from opposite bracket region).
        for lr1_idx in 0..lr1_match_count {
            // Slot 1: from top half of WR1
            let wr1_top = lr1_idx;
            // Slot 2: from bottom half of WR1, reversed
            let wr1_bottom = wr1_match_count - 1 - lr1_idx;

            result.push((wr1_top, lr1_idx, 1));
            result.push((wr1_bottom, lr1_idx, 2));
        }

        result
    }

    /// Prepare participants for bracket generation by sorting and assigning seeds.
    ///
    /// # Arguments
    /// * `registrations` - Confirmed registrations to seed
    ///
    /// # Returns
    /// A list of seeded participants sorted by seed.
    pub fn prepare_participants(
        registrations: Vec<TournamentRegistration>,
    ) -> Vec<SeededParticipant> {
        let mut participants: Vec<SeededParticipant> = registrations
            .into_iter()
            .enumerate()
            .map(|(idx, reg)| SeededParticipant {
                registration_id: reg.id,
                seed: reg.seed.unwrap_or((idx + 1) as i32),
                participant_name: reg.participant_name,
                participant_logo_url: reg.participant_logo_url,
            })
            .collect();

        // Sort by seed
        participants.sort_by_key(|p| p.seed);

        // Re-assign sequential seeds if there are gaps
        for (idx, participant) in participants.iter_mut().enumerate() {
            participant.seed = (idx + 1) as i32;
        }

        participants
    }
}

/// Get the next power of 2 >= n.
const fn next_power_of_two(n: usize) -> usize {
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
fn generate_seeding_order(bracket_size: usize) -> Vec<usize> {
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

    // Standard seeding pattern
    // For 16 teams (8 matches in R1):
    // Match 0: 1 vs 16 (indices 0 vs 15)
    // Match 1: 8 vs 9 (indices 7 vs 8)
    // Match 2: 5 vs 12 (indices 4 vs 11)
    // Match 3: 4 vs 13 (indices 3 vs 12)
    // Match 4: 3 vs 14 (indices 2 vs 13)
    // Match 5: 6 vs 11 (indices 5 vs 10)
    // Match 6: 7 vs 10 (indices 6 vs 9)
    // Match 7: 2 vs 15 (indices 1 vs 14)

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
    use portal_core::TournamentRegistrationId;

    fn create_test_participants(count: usize) -> Vec<SeededParticipant> {
        (1..=count)
            .map(|seed| SeededParticipant {
                registration_id: TournamentRegistrationId::new(),
                seed: seed as i32,
                participant_name: format!("Team {seed}"),
                participant_logo_url: None,
            })
            .collect()
    }

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
        // Should produce matchups: 1v4, 2v3
        // So indices: 0v3, 1v2
        assert_eq!(order.len(), 4);
        // First match: seed 1 vs seed 4
        assert_eq!(order[0], 0); // Seed 1
        assert_eq!(order[1], 3); // Seed 4
        // Second match: seed 2 vs seed 3
        assert_eq!(order[2], 1); // Seed 2 (or could be 2)
    }

    #[test]
    fn test_seeding_order_8_teams() {
        let order = generate_seeding_order(8);
        assert_eq!(order.len(), 8);
        // Matchups should be: 1v8, 4v5, 3v6, 2v7
        // Check first match is 1v8
        assert_eq!(order[0], 0); // Seed 1
        assert_eq!(order[1], 7); // Seed 8
    }

    #[test]
    fn test_single_elimination_4_teams() {
        let participants = create_test_participants(4);
        let tournament_id = TournamentId::new();
        let stage_id = TournamentStageId::new();
        let bracket_id = TournamentBracketId::new();

        let result = BracketGenerator::single_elimination(
            tournament_id,
            stage_id,
            bracket_id,
            participants,
            MatchFormat::Bo3,
        )
        .unwrap();

        // 4 teams = 2 rounds (semifinal + final)
        assert_eq!(result.total_rounds, 2);

        // 3 matches total (2 semis + 1 final)
        assert_eq!(result.matches.len(), 3);

        // 4 initial assignments (all 4 teams in round 1)
        assert_eq!(result.initial_assignments.len(), 4);

        // No byes needed
        assert_eq!(result.byes.len(), 0);
    }

    #[test]
    fn test_single_elimination_with_byes() {
        // 5 teams needs 8-team bracket with 3 byes
        let participants = create_test_participants(5);
        let tournament_id = TournamentId::new();
        let stage_id = TournamentStageId::new();
        let bracket_id = TournamentBracketId::new();

        let result = BracketGenerator::single_elimination(
            tournament_id,
            stage_id,
            bracket_id,
            participants,
            MatchFormat::Bo1,
        )
        .unwrap();

        // 8-team bracket = 3 rounds
        assert_eq!(result.total_rounds, 3);

        // 7 matches (4 + 2 + 1)
        assert_eq!(result.matches.len(), 7);

        // Should have 3 byes
        assert_eq!(result.byes.len(), 3);
    }

    #[test]
    fn test_single_elimination_insufficient_participants() {
        let participants = create_test_participants(1);
        let tournament_id = TournamentId::new();
        let stage_id = TournamentStageId::new();
        let bracket_id = TournamentBracketId::new();

        let result = BracketGenerator::single_elimination(
            tournament_id,
            stage_id,
            bracket_id,
            participants,
            MatchFormat::Bo1,
        );

        assert!(matches!(result, Err(DomainError::InsufficientParticipants)));
    }

    #[test]
    fn test_prepare_participants() {
        let registrations = (1..=4)
            .map(|i| TournamentRegistration {
                id: TournamentRegistrationId::new(),
                tournament_id: TournamentId::new(),
                team_season_id: None,
                player_id: None,
                adhoc_team_id: None,
                participant_name: format!("Team {i}"),
                participant_logo_url: None,
                registered_by: portal_core::UserId::new(),
                registered_at: chrono::Utc::now(),
                checked_in: true,
                checked_in_at: Some(chrono::Utc::now()),
                checked_in_by: None,
                seed: Some(i),
                seed_rating: None,
                status: portal_core::types::TournamentRegistrationStatus::Approved,
                admin_notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                withdrawn_at: None,
            })
            .collect();

        let prepared = BracketGenerator::prepare_participants(registrations);

        assert_eq!(prepared.len(), 4);
        assert_eq!(prepared[0].seed, 1);
        assert_eq!(prepared[1].seed, 2);
        assert_eq!(prepared[2].seed, 3);
        assert_eq!(prepared[3].seed, 4);
    }

    // =========================================================================
    // DOUBLE ELIMINATION TESTS
    // =========================================================================

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

        // 4 teams → bracket size 4, wb_rounds = 2
        // WB: 2 + 1 = 3 matches
        assert_eq!(result.winners_bracket.total_rounds, 2);
        assert_eq!(result.winners_bracket.matches.len(), 3);

        // LB: 2 * (2-1) = 2 rounds
        // LR1: 1 match, LR2: 1 match = 2 matches total
        assert_eq!(result.losers_bracket.total_rounds, 2);
        assert_eq!(result.losers_bracket.matches.len(), 2);

        // GF: 1 match
        assert_eq!(result.grand_final.matches.len(), 1);

        // Total: 3 + 2 + 1 = 6 matches (formula: 2S - 2 = 2*4 - 2 = 6)
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 6);

        // 4 initial assignments in WB
        assert_eq!(result.winners_bracket.initial_assignments.len(), 4);
        assert_eq!(result.winners_bracket.byes.len(), 0);
    }

    #[test]
    fn test_double_elimination_8_teams() {
        let result = create_de_result(8);

        // 8 teams → bracket size 8, wb_rounds = 3
        // WB: 4 + 2 + 1 = 7 matches
        assert_eq!(result.winners_bracket.total_rounds, 3);
        assert_eq!(result.winners_bracket.matches.len(), 7);

        // LB: 2 * (3-1) = 4 rounds
        // LR1: 2, LR2: 2, LR3: 1, LR4: 1 = 6 matches
        assert_eq!(result.losers_bracket.total_rounds, 4);
        assert_eq!(result.losers_bracket.matches.len(), 6);

        // GF: 1 match
        assert_eq!(result.grand_final.matches.len(), 1);

        // Total: 7 + 6 + 1 = 14 (formula: 2*8 - 2 = 14)
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 14);

        // 8 initial assignments in WB
        assert_eq!(result.winners_bracket.initial_assignments.len(), 8);
    }

    #[test]
    fn test_double_elimination_16_teams() {
        let result = create_de_result(16);

        // 16 teams → bracket size 16, wb_rounds = 4
        // WB: 8 + 4 + 2 + 1 = 15 matches
        assert_eq!(result.winners_bracket.total_rounds, 4);
        assert_eq!(result.winners_bracket.matches.len(), 15);

        // LB: 2 * (4-1) = 6 rounds
        // LR1: 4, LR2: 4, LR3: 2, LR4: 2, LR5: 1, LR6: 1 = 14 matches
        assert_eq!(result.losers_bracket.total_rounds, 6);
        assert_eq!(result.losers_bracket.matches.len(), 14);

        // Total: 15 + 14 + 1 = 30 (formula: 2*16 - 2 = 30)
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 30);
    }

    #[test]
    fn test_double_elimination_with_byes() {
        // 6 teams in 8-bracket → 2 byes
        let result = create_de_result(6);

        // Bracket size is 8, wb_rounds = 3
        assert_eq!(result.winners_bracket.total_rounds, 3);
        assert_eq!(result.winners_bracket.matches.len(), 7);

        // Should have 2 byes (8 - 6 = 2)
        assert_eq!(result.winners_bracket.byes.len(), 2);

        // WB initial assignments: 6 participants - 2 byes = 4 matched + 2 bye = 8 assignments?
        // Actually: 6 teams, 8 bracket size. 2 byes means 2 teams advance directly.
        // Remaining 4 teams are assigned to WR1 matches. So 4 assignments.
        let assigned_count = result.winners_bracket.initial_assignments.len();
        // 6 teams in 8-bracket: 6 - 2 byes = 4 actual R1 participants (2 pairs)
        // Wait, 8 bracket means 4 WR1 matches. 6 teams fill 6 slots, 2 empty = 2 byes.
        // Each bye match has 1 participant, so 2 assignments go to byes.
        // Remaining 6 - 2 = 4 teams go into 2 full matches = 4 assignments.
        assert_eq!(assigned_count, 4);

        // LB structure unchanged (still based on bracket size 8)
        assert_eq!(result.losers_bracket.matches.len(), 6);
    }

    #[test]
    fn test_double_elimination_cross_bracket_links() {
        let result = create_de_result(8);

        // For 8 teams (wb_rounds=3):
        // WR1 (4 matches) → LR1 (loser drops): 4 links
        // WR2 (2 matches) → LR2 (loser drops): 2 links
        // WR3 (1 match/WBF) → LR4 (loser drops): 1 link
        // WB final winner → GF: 1 link
        // LB final winner → GF: 1 link
        // Total: 4 + 2 + 1 + 2 = 9 links

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

        // Every WB match should have a loser-drops-to link
        assert_eq!(loser_drops.len(), 7); // All 7 WB matches

        // WB final and LB final winners advance to GF
        assert_eq!(winner_advances.len(), 2);

        // Verify GF receives from both finals
        let gf_sources: Vec<_> = winner_advances
            .iter()
            .map(|l| l.target_bracket_position.as_str())
            .collect();
        assert!(gf_sources.iter().all(|p| *p == "GFM1"));
    }

    #[test]
    fn test_double_elimination_no_immediate_rematches() {
        let result = create_de_result(8);

        // In an 8-team bracket, WR1 has 4 matches:
        // WR1M1, WR1M2, WR1M3, WR1M4
        // LR1 has 2 matches: LR1M1, LR1M2
        //
        // Cross-seeding should ensure that losers from WR1M1 and WR1M2
        // don't face each other in LR1 (since they were in the same bracket half).
        // Instead, WR1M1 loser should face WR1M4 loser, WR1M2 loser should face WR1M3 loser.

        let lr1_matches: Vec<_> = result
            .losers_bracket
            .matches
            .iter()
            .filter(|m| m.round == 1)
            .collect();

        assert_eq!(lr1_matches.len(), 2);

        // Check that LR1M1 has sources from opposite halves of WR1
        let lr1m1 = &lr1_matches[0];
        let p1 = lr1m1.participant1_source.as_ref().unwrap();
        let p2 = lr1m1.participant2_source.as_ref().unwrap();

        // p1 should be from WR1M1 (top half) and p2 from WR1M4 (bottom half)
        // or similar opposite-half pairing
        if let (MatchParticipantSource::LoserOf(pos1), MatchParticipantSource::LoserOf(pos2)) =
            (p1, p2)
        {
            // They should be from different halves
            assert_ne!(pos1, pos2, "LR1 match should not pair same-match losers");
            // Verify they're from opposite halves: one from M1/M2, other from M3/M4
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

        // LR1: all LoserOf sources (from WR1)
        for m in result.losers_bracket.matches.iter().filter(|m| m.round == 1) {
            assert!(matches!(
                m.participant1_source,
                Some(MatchParticipantSource::LoserOf(_))
            ));
            assert!(matches!(
                m.participant2_source,
                Some(MatchParticipantSource::LoserOf(_))
            ));
        }

        // LR2 (dropper round): slot 1 = WinnerOf (LB survivor), slot 2 = LoserOf (WB dropper)
        for m in result.losers_bracket.matches.iter().filter(|m| m.round == 2) {
            assert!(
                matches!(m.participant1_source, Some(MatchParticipantSource::WinnerOf(_))),
                "LR2 slot 1 should be WinnerOf, got {:?}",
                m.participant1_source
            );
            assert!(
                matches!(m.participant2_source, Some(MatchParticipantSource::LoserOf(_))),
                "LR2 slot 2 should be LoserOf, got {:?}",
                m.participant2_source
            );
        }

        // LR3 (survivor round): both WinnerOf
        for m in result.losers_bracket.matches.iter().filter(|m| m.round == 3) {
            assert!(matches!(
                m.participant1_source,
                Some(MatchParticipantSource::WinnerOf(_))
            ));
            assert!(matches!(
                m.participant2_source,
                Some(MatchParticipantSource::WinnerOf(_))
            ));
        }

        // GF: both WinnerOf
        let gf = &result.grand_final.matches[0];
        assert!(matches!(
            gf.participant1_source,
            Some(MatchParticipantSource::WinnerOf(_))
        ));
        assert!(matches!(
            gf.participant2_source,
            Some(MatchParticipantSource::WinnerOf(_))
        ));

        // GF source positions
        if let Some(MatchParticipantSource::WinnerOf(pos)) = &gf.participant1_source {
            assert_eq!(pos, "WR3M1", "GF slot 1 should be WB final winner");
        }
        if let Some(MatchParticipantSource::WinnerOf(pos)) = &gf.participant2_source {
            assert_eq!(pos, "LR4M1", "GF slot 2 should be LB final winner");
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

        // 2 teams → bracket size 2, wb_rounds = 1
        // WB: 1 match
        assert_eq!(result.winners_bracket.matches.len(), 1);

        // LB: 2 * (1-1) = 0 rounds, 0 matches
        // With only 2 teams, there's no losers bracket needed
        // Actually, lb_rounds = 0, so no LB matches
        assert_eq!(result.losers_bracket.matches.len(), 0);

        // GF: 1 match
        assert_eq!(result.grand_final.matches.len(), 1);

        // Total: 1 + 0 + 1 = 2 (formula: 2*2 - 2 = 2)
        let total = result.winners_bracket.matches.len()
            + result.losers_bracket.matches.len()
            + result.grand_final.matches.len();
        assert_eq!(total, 2);
    }

    #[test]
    fn test_lb_matches_in_round() {
        // 8-team bracket (wb_rounds=3)
        assert_eq!(BracketGenerator::lb_matches_in_round(1, 3), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(2, 3), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(3, 3), 1);
        assert_eq!(BracketGenerator::lb_matches_in_round(4, 3), 1);

        // 16-team bracket (wb_rounds=4)
        assert_eq!(BracketGenerator::lb_matches_in_round(1, 4), 4);
        assert_eq!(BracketGenerator::lb_matches_in_round(2, 4), 4);
        assert_eq!(BracketGenerator::lb_matches_in_round(3, 4), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(4, 4), 2);
        assert_eq!(BracketGenerator::lb_matches_in_round(5, 4), 1);
        assert_eq!(BracketGenerator::lb_matches_in_round(6, 4), 1);

        // 4-team bracket (wb_rounds=2)
        assert_eq!(BracketGenerator::lb_matches_in_round(1, 2), 1);
        assert_eq!(BracketGenerator::lb_matches_in_round(2, 2), 1);
    }

    #[test]
    fn test_double_elimination_wb_positions() {
        let result = create_de_result(8);

        // WB should have positions: WR1M1..WR1M4, WR2M1..WR2M2, WR3M1
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
