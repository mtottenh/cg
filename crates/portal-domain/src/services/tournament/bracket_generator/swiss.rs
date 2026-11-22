//! Swiss System bracket generation.

use super::{BracketGenerator, GeneratedBracket, InitialAssignment};
use crate::entities::tournament::SeededParticipant;
use crate::repositories::tournament::CreateTournamentMatch;
use portal_core::types::{MatchFormatPlan, MatchParticipantSource};
use portal_core::{
    DomainError, TournamentBracketId, TournamentId, TournamentRegistrationId, TournamentStageId,
};
use std::collections::HashSet;

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

/// Read the optional `max_rounds` key from Swiss (or Swiss-group)
/// `format_settings`. Absent/null → `None`; anything other than a positive
/// integer is rejected rather than silently replaced by the default.
///
/// A whole number written as a float (`3.0`) is accepted: JSON has one
/// number type and several clients serialise every number that way.
pub fn parse_swiss_max_rounds(settings: &serde_json::Value) -> Result<Option<u32>, DomainError> {
    let Some(value) = settings.get("max_rounds").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let whole = value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|f| f.fract() == 0.0 && (1.0..=f64::from(u32::MAX)).contains(f))
            // Exact: the filter above pinned it to a whole number in range.
            .map(|f| f as u64)
    });
    whole
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n >= 1)
        .map(Some)
        .ok_or_else(|| {
            DomainError::InvalidState("max_rounds must be a positive integer".to_string())
        })
}

/// Number of Swiss rounds a bracket of `participant_count` plays.
///
/// Defaults to `ceil(log2(n))` (at least 1). Whatever is requested is
/// clamped to the number of rounds that can be paired without a rematch —
/// a full round robin: `n - 1` rounds for an even field, `n` for an odd one
/// (each player sits out one round). Past that every round would be forced
/// rematches. Within the cap, `swiss_next_round` searches for pairings that
/// keep the remaining rounds rematch-free, so a full run to the cap is
/// rematch-free whenever its search budget suffices (always, at group
/// sizes).
#[must_use]
pub fn swiss_total_rounds(participant_count: usize, requested: Option<u32>) -> i32 {
    let n = participant_count.max(2);
    let rematch_free_limit = if n.is_multiple_of(2) { n - 1 } else { n };
    let default = (usize::BITS - (n - 1).leading_zeros()) as usize; // ceil(log2(n))
    let rounds = requested.map_or(default, |r| r as usize);
    rounds.clamp(1, rematch_free_limit) as i32
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
        format_plan: &MatchFormatPlan,
    ) -> Result<(GeneratedBracket, Option<TournamentRegistrationId>), DomainError> {
        let match_format = format_plan.format_for(1, 0);
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
                maps_required: match_format.game_count(),
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
    /// 2. Pair in that order, each participant with the closest-ranked
    ///    opponent they have not met — so score groups pair internally and
    ///    an odd one out floats down to the next score group
    /// 3. Odd field: the bye is a virtual lowest-ranked opponent, so it goes
    ///    to the lowest-ranked participant who has not had one
    /// 4. The search backtracks rather than settling for a rematch: it looks
    ///    for a rematch-free pairing that also leaves the remaining rounds
    ///    (up to `total_rounds`) pairable without rematches, then for one
    ///    that is merely rematch-free this round, and only then falls back to
    ///    a greedy pairing that allows rematches
    #[allow(clippy::too_many_arguments)]
    pub fn swiss_next_round(
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        bracket_id: TournamentBracketId,
        round_number: i32,
        total_rounds: i32,
        standings: Vec<SwissParticipantStanding>,
        completed_pairings: &[(TournamentRegistrationId, TournamentRegistrationId)],
        format_plan: &MatchFormatPlan,
    ) -> Result<(GeneratedBracket, Option<TournamentRegistrationId>), DomainError> {
        // Swiss rounds are generated one at a time and the round count is not
        // fixed up front, so only per-round overrides apply — no final
        // semantics.
        let match_format = format_plan.format_for(round_number, 0);
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
        let mut pairing_set = HashSet::new();
        for (a, b) in completed_pairings {
            pairing_set.insert((*a, *b));
            pairing_set.insert((*b, *a));
        }

        // Pair the round (bye included for an odd field). See `pair_round`.
        let future_rounds = usize::try_from(total_rounds - round_number).unwrap_or(0);
        let (pairings, bye_participant) = pair_round(&sorted, &pairing_set, future_rounds);

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
                maps_required: match_format.game_count(),
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

/// DFS node budget for one pairing search. Groups are small, so this is
/// generous; on a huge field a search that runs out of budget degrades to
/// the next, weaker pairing strategy instead of stalling.
const PAIRING_SEARCH_BUDGET: usize = 200_000;

/// Pair one Swiss round.
///
/// Returns index pairs into `sorted` (ranking order) and the bye, if the
/// field is odd. Strategies, strongest first:
/// 1. rematch-free, and the remaining `future_rounds` stay rematch-free;
/// 2. rematch-free this round;
/// 3. greedy, allowing rematches (only when no rematch-free pairing exists
///    or the search budget ran out).
fn pair_round(
    sorted: &[SwissParticipantStanding],
    played: &HashSet<(TournamentRegistrationId, TournamentRegistrationId)>,
    future_rounds: usize,
) -> (Vec<(usize, usize)>, Option<TournamentRegistrationId>) {
    let n = sorted.len();
    let odd = !n.is_multiple_of(2);
    // Node `n` (odd fields only) is the bye: last in ranking order, and
    // already "met" by everyone who has had a bye.
    let nodes = if odd { n + 1 } else { n };
    let mut used = vec![vec![false; nodes]; nodes];
    for i in 0..n {
        for j in (i + 1)..n {
            if played.contains(&(sorted[i].registration_id, sorted[j].registration_id)) {
                used[i][j] = true;
                used[j][i] = true;
            }
        }
        if odd && sorted[i].had_bye {
            used[i][n] = true;
            used[n][i] = true;
        }
    }

    let strategies: &[usize] = if future_rounds > 0 {
        &[future_rounds, 0]
    } else {
        &[0]
    };
    for &lookahead in strategies {
        let mut budget = PAIRING_SEARCH_BUDGET;
        if let Some(pairs) = search_pairing(&mut used, lookahead, &mut budget) {
            let mut bye = None;
            let mut real = Vec::with_capacity(pairs.len());
            for (i, j) in pairs {
                if odd && j == n {
                    bye = Some(sorted[i].registration_id);
                } else {
                    real.push((i, j));
                }
            }
            return (real, bye);
        }
    }

    greedy_pairing(sorted, played)
}

/// Find a perfect matching of the nodes avoiding `used` pairs, each node
/// taking the closest-ranked opponent available; with `rounds_after > 0`
/// what is left must also still admit that many further disjoint matchings.
fn search_pairing(
    used: &mut [Vec<bool>],
    rounds_after: usize,
    budget: &mut usize,
) -> Option<Vec<(usize, usize)>> {
    let mut paired = vec![false; used.len()];
    let mut current = Vec::with_capacity(used.len() / 2);
    extend_pairing(used, &mut paired, &mut current, rounds_after, budget).then_some(current)
}

fn extend_pairing(
    used: &mut [Vec<bool>],
    paired: &mut [bool],
    current: &mut Vec<(usize, usize)>,
    rounds_after: usize,
    budget: &mut usize,
) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;

    let Some(i) = paired.iter().position(|p| !p) else {
        if rounds_after == 0 {
            return true;
        }
        // This round is complete: can the remaining rounds still be paired?
        for &(a, b) in current.iter() {
            used[a][b] = true;
            used[b][a] = true;
        }
        let feasible = search_pairing(used, rounds_after - 1, budget).is_some();
        for &(a, b) in current.iter() {
            used[a][b] = false;
            used[b][a] = false;
        }
        return feasible;
    };

    paired[i] = true;
    for j in (i + 1)..paired.len() {
        if paired[j] || used[i][j] {
            continue;
        }
        paired[j] = true;
        current.push((i, j));
        if extend_pairing(used, paired, current, rounds_after, budget) {
            return true;
        }
        current.pop();
        paired[j] = false;
    }
    paired[i] = false;
    false
}

/// Last resort when no rematch-free pairing exists: the original greedy
/// pairing. Bye to the lowest-ranked participant without one (else the
/// lowest-ranked), then each participant takes the closest-ranked opponent
/// they have not met, and whoever is left is paired regardless.
fn greedy_pairing(
    sorted: &[SwissParticipantStanding],
    played: &HashSet<(TournamentRegistrationId, TournamentRegistrationId)>,
) -> (Vec<(usize, usize)>, Option<TournamentRegistrationId>) {
    let n = sorted.len();
    let mut paired = vec![false; n];
    let bye = if n.is_multiple_of(2) {
        None
    } else {
        let idx = (0..n).rev().find(|&i| !sorted[i].had_bye).unwrap_or(n - 1);
        paired[idx] = true;
        Some(sorted[idx].registration_id)
    };

    let mut pairings = Vec::with_capacity(n / 2);
    for i in 0..n {
        if paired[i] {
            continue;
        }
        if let Some(j) = ((i + 1)..n).find(|&j| {
            !paired[j] && !played.contains(&(sorted[i].registration_id, sorted[j].registration_id))
        }) {
            paired[i] = true;
            paired[j] = true;
            pairings.push((i, j));
        }
    }
    let unpaired: Vec<usize> = (0..n).filter(|&i| !paired[i]).collect();
    for chunk in unpaired.chunks(2) {
        if let [a, b] = *chunk {
            pairings.push((a, b));
        }
    }
    (pairings, bye)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tournament::bracket_generator::tests::create_test_participants;
    use portal_core::types::MatchFormat;
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
            &MatchFormatPlan::uniform(MatchFormat::Bo3),
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
            &MatchFormatPlan::uniform(MatchFormat::Bo3),
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
            3,
            standings,
            &completed_pairings,
            &MatchFormatPlan::uniform(MatchFormat::Bo3),
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
            3,
            standings,
            &completed_pairings,
            &MatchFormatPlan::uniform(MatchFormat::Bo3),
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
            &MatchFormatPlan::uniform(MatchFormat::Bo1),
        );
        assert!(matches!(result, Err(DomainError::InsufficientParticipants)));
    }

    #[test]
    fn test_swiss_total_rounds_defaults_to_ceil_log2() {
        assert_eq!(swiss_total_rounds(2, None), 1);
        assert_eq!(swiss_total_rounds(3, None), 2);
        assert_eq!(swiss_total_rounds(4, None), 2);
        assert_eq!(swiss_total_rounds(5, None), 3);
        assert_eq!(swiss_total_rounds(8, None), 3);
        assert_eq!(swiss_total_rounds(9, None), 4);
        assert_eq!(swiss_total_rounds(16, None), 4);
    }

    #[test]
    fn test_swiss_total_rounds_clamps_to_rematch_free_limit() {
        // Even field: n - 1 rounds is a full round robin.
        assert_eq!(swiss_total_rounds(4, Some(3)), 3);
        assert_eq!(swiss_total_rounds(4, Some(10)), 3);
        // Group of two: one possible pairing.
        assert_eq!(swiss_total_rounds(2, Some(5)), 1);
        // Odd field: n rounds, each player sitting out once.
        assert_eq!(swiss_total_rounds(3, Some(3)), 3);
        assert_eq!(swiss_total_rounds(3, Some(7)), 3);
        assert_eq!(swiss_total_rounds(5, Some(2)), 2);
    }

    #[test]
    fn test_parse_swiss_max_rounds() {
        assert_eq!(
            parse_swiss_max_rounds(&serde_json::json!({})).unwrap(),
            None
        );
        assert_eq!(
            parse_swiss_max_rounds(&serde_json::json!({ "max_rounds": null })).unwrap(),
            None
        );
        assert_eq!(
            parse_swiss_max_rounds(&serde_json::json!({ "max_rounds": 3.0 })).unwrap(),
            Some(3)
        );
        assert_eq!(
            parse_swiss_max_rounds(&serde_json::json!({ "max_rounds": 5 })).unwrap(),
            Some(5)
        );
        for bad in [
            serde_json::json!({ "max_rounds": 0 }),
            serde_json::json!({ "max_rounds": -1 }),
            serde_json::json!({ "max_rounds": 2.5 }),
            serde_json::json!({ "max_rounds": "3" }),
        ] {
            assert!(parse_swiss_max_rounds(&bad).is_err(), "{bad}");
        }
    }

    /// Pairs `(p1, p2)` of a generated round, read off its assignments.
    fn round_pairs(
        generated: &GeneratedBracket,
    ) -> Vec<(TournamentRegistrationId, TournamentRegistrationId)> {
        let mut by_pos: std::collections::BTreeMap<&str, [Option<TournamentRegistrationId>; 2]> =
            std::collections::BTreeMap::new();
        for a in &generated.initial_assignments {
            by_pos.entry(a.bracket_position.as_str()).or_default()[usize::from(a.slot - 1)] =
                Some(a.participant.registration_id);
        }
        by_pos
            .into_values()
            .map(|[a, b]| (a.unwrap(), b.unwrap()))
            .collect()
    }

    /// Play a Swiss bracket of `n` to its rematch-free cap, deciding each
    /// match with `p1_wins(round, match_index)`, and assert there is never a
    /// rematch and (odd n) nobody sits out twice.
    fn run_to_cap(n: usize, p1_wins: impl Fn(i32, usize) -> bool) {
        let total = swiss_total_rounds(n, Some(u32::MAX));
        let participants = create_test_participants(n);
        let plan = MatchFormatPlan::uniform(MatchFormat::Bo1);
        let (r1, mut bye) = BracketGenerator::swiss_initial_round(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            participants.clone(),
            &plan,
        )
        .unwrap();

        let mut points: std::collections::HashMap<TournamentRegistrationId, i32> = participants
            .iter()
            .map(|p| (p.registration_id, 0))
            .collect();
        let mut byes: std::collections::HashMap<TournamentRegistrationId, usize> =
            std::collections::HashMap::new();
        let mut met = HashSet::new();
        let mut history = Vec::new();
        let mut pairs = round_pairs(&r1);

        for round in 1..=total {
            if let Some(b) = bye {
                *byes.entry(b).or_default() += 1;
                *points.get_mut(&b).unwrap() += 3;
            }
            for (k, &(a, b)) in pairs.iter().enumerate() {
                assert!(
                    met.insert((a, b)) && met.insert((b, a)),
                    "rematch in round {round} of a {n}-player field"
                );
                history.push((a, b));
                let winner = if p1_wins(round, k) { a } else { b };
                *points.get_mut(&winner).unwrap() += 3;
            }
            if round == total {
                break;
            }

            let standings = participants
                .iter()
                .map(|p| SwissParticipantStanding {
                    registration_id: p.registration_id,
                    participant_name: p.participant_name.clone(),
                    participant_logo_url: None,
                    seed: p.seed,
                    points: points[&p.registration_id],
                    buchholz_score: 0.0,
                    had_bye: byes.contains_key(&p.registration_id),
                })
                .collect();
            let (next, next_bye) = BracketGenerator::swiss_next_round(
                TournamentId::new(),
                TournamentStageId::new(),
                TournamentBracketId::new(),
                round + 1,
                total,
                standings,
                &history,
                &plan,
            )
            .unwrap();
            pairs = round_pairs(&next);
            assert_eq!(pairs.len(), n / 2);
            bye = next_bye;
        }

        // Ran to a full round robin: everyone met everyone exactly once.
        assert_eq!(history.len(), n * (n - 1) / 2, "{n}-player field");
        if !n.is_multiple_of(2) {
            assert_eq!(byes.len(), n, "every player sits out once");
            assert!(byes.values().all(|&c| c == 1), "{byes:?}");
        }
    }

    #[test]
    fn test_swiss_runs_to_cap_without_rematches() {
        for n in [2, 3, 4, 5, 6, 7, 8, 9, 10] {
            run_to_cap(n, |_, _| true); // higher-ranked side always wins
            run_to_cap(n, |_, _| false); // lower-ranked side always wins
            run_to_cap(n, |round, k| (round as usize + k).is_multiple_of(2));
            run_to_cap(n, |round, k| (round as usize * 7 + k * 3) % 5 < 2);
        }
    }

    /// The old greedy pairing painted itself into a corner here: 1v2, then
    /// 3 and 4 have both met 5 and 6 is left with 6's only legal opponent
    /// taken. Backtracking finds the rematch-free pairing.
    #[test]
    fn test_swiss_next_round_backtracks_instead_of_rematching() {
        let ids: Vec<TournamentRegistrationId> =
            (0..6).map(|_| TournamentRegistrationId::new()).collect();
        let standings: Vec<SwissParticipantStanding> = (0..6)
            .map(|i| SwissParticipantStanding {
                registration_id: ids[i],
                participant_name: format!("Team {}", i + 1),
                participant_logo_url: None,
                seed: i as i32 + 1,
                points: (6 - i) as i32,
                buchholz_score: 0.0,
                had_bye: false,
            })
            .collect();
        // Greedy would take 1v2, 3v4, then be left with 5v6 — a rematch.
        let played = vec![(ids[4], ids[5]), (ids[0], ids[3]), (ids[1], ids[2])];
        let (result, _) = BracketGenerator::swiss_next_round(
            TournamentId::new(),
            TournamentStageId::new(),
            TournamentBracketId::new(),
            2,
            2,
            standings,
            &played,
            &MatchFormatPlan::uniform(MatchFormat::Bo1),
        )
        .unwrap();
        let pairs = round_pairs(&result);
        assert_eq!(pairs.len(), 3);
        for (a, b) in pairs {
            assert!(
                !played.contains(&(a, b)) && !played.contains(&(b, a)),
                "rematch"
            );
        }
    }
}
