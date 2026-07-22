//! CS2 evidence validation.
//!
//! Validates demo stats against claimed match results.

use super::demo_stats::Cs2DemoStats;
use crate::types::{EvidenceValidation, ExtractedResult, GameResult};
use tracing::debug;

/// Validates demo evidence against claimed match results.
pub struct Cs2EvidenceValidator;

impl Cs2EvidenceValidator {
    /// Validate demo stats against a claimed game result.
    ///
    /// # Arguments
    /// * `stats` - Parsed demo stats from the external service
    /// * `claimed_result` - The result being claimed by the player
    /// * `participant1_steam_ids` - Steam IDs for participant 1 (tournament registration)
    /// * `participant2_steam_ids` - Steam IDs for participant 2 (tournament registration)
    ///
    /// # Returns
    /// Validation result with confidence score, warnings, and errors.
    pub fn validate(
        stats: &Cs2DemoStats,
        claimed_result: &GameResult,
        participant1_steam_ids: &[String],
        participant2_steam_ids: &[String],
    ) -> EvidenceValidation {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut confidence = 1.0f32;

        // 1. Verify map matches (if claimed)
        if let Some(claimed_map) = &claimed_result.map_id
            && !Self::maps_match(&stats.map, claimed_map)
        {
            errors.push(format!(
                "Map mismatch: demo has '{}', claimed '{}'",
                stats.map, claimed_map
            ));
            confidence *= 0.0; // Fatal mismatch
        }

        // 2. Verify players participated
        let (p1_present, p1_count, p1_total) = Self::verify_players(stats, participant1_steam_ids);
        let (p2_present, p2_count, p2_total) = Self::verify_players(stats, participant2_steam_ids);

        if !p1_present {
            warnings.push(format!(
                "Only {p1_count}/{p1_total} Participant 1 players found in demo"
            ));
            confidence *= 0.7;
        }
        if !p2_present {
            warnings.push(format!(
                "Only {p2_count}/{p2_total} Participant 2 players found in demo"
            ));
            confidence *= 0.7;
        }

        // 3. Determine which demo team corresponds to which participant
        let team_mapping =
            Self::determine_team_mapping(stats, participant1_steam_ids, participant2_steam_ids);

        if let Some((p1_team_name, p2_team_name, mapping_confidence)) = team_mapping {
            confidence *= mapping_confidence;

            debug!(
                p1_team = %p1_team_name,
                p2_team = %p2_team_name,
                mapping_confidence = %mapping_confidence,
                "Team mapping determined"
            );

            // 4. Extract and compare scores
            let demo_p1_score = stats.score_for_team(&p1_team_name).unwrap_or(0);
            let demo_p2_score = stats.score_for_team(&p2_team_name).unwrap_or(0);

            let scores_match = demo_p1_score == claimed_result.participant1_score
                && demo_p2_score == claimed_result.participant2_score;

            if !scores_match {
                errors.push(format!(
                    "Score mismatch: demo shows {demo_p1_score}-{demo_p2_score}, claimed {}-{}",
                    claimed_result.participant1_score, claimed_result.participant2_score
                ));
                confidence *= 0.0; // Fatal mismatch
            }

            // 5. Verify winner consistency
            let demo_winner_is_p1 = demo_p1_score > demo_p2_score;
            let claimed_winner_is_p1 =
                claimed_result.participant1_score > claimed_result.participant2_score;

            if demo_winner_is_p1 != claimed_winner_is_p1 {
                errors.push("Winner mismatch between demo and claimed result".to_string());
                confidence *= 0.0;
            }

            // Build extracted result
            let player_stats = if stats.player_summaries.is_empty() {
                let summaries: Vec<_> = stats.all_player_summaries();
                serde_json::to_value(&summaries).unwrap_or_default()
            } else {
                serde_json::to_value(&stats.player_summaries).unwrap_or_default()
            };

            let extracted_result = ExtractedResult {
                map_id: stats.map.clone(),
                participant1_score: demo_p1_score,
                participant2_score: demo_p2_score,
                duration_seconds: 0, // Not available in stats format
                player_stats,
            };

            EvidenceValidation {
                is_valid: errors.is_empty() && confidence > 0.5,
                confidence,
                extracted_result: Some(extracted_result),
                warnings,
                errors,
            }
        } else {
            errors.push("Could not determine team mapping from Steam IDs".to_string());
            EvidenceValidation {
                is_valid: false,
                confidence: 0.0,
                extracted_result: None,
                warnings,
                errors,
            }
        }
    }

    /// Validate demo stats without comparing to a claimed result.
    ///
    /// Used to extract results from a demo when no claim exists yet.
    pub fn extract_result(
        stats: &Cs2DemoStats,
        participant1_steam_ids: &[String],
        participant2_steam_ids: &[String],
    ) -> Option<ExtractedResult> {
        let team_mapping =
            Self::determine_team_mapping(stats, participant1_steam_ids, participant2_steam_ids);

        team_mapping.map(|(p1_team_name, p2_team_name, _)| {
            let demo_p1_score = stats.score_for_team(&p1_team_name).unwrap_or(0);
            let demo_p2_score = stats.score_for_team(&p2_team_name).unwrap_or(0);

            let player_stats = if stats.player_summaries.is_empty() {
                let summaries: Vec<_> = stats.all_player_summaries();
                serde_json::to_value(&summaries).unwrap_or_default()
            } else {
                serde_json::to_value(&stats.player_summaries).unwrap_or_default()
            };

            ExtractedResult {
                map_id: stats.map.clone(),
                participant1_score: demo_p1_score,
                participant2_score: demo_p2_score,
                duration_seconds: 0,
                player_stats,
            }
        })
    }

    /// Check if map names match (handles different naming conventions).
    fn maps_match(demo_map: &str, claimed_map: &str) -> bool {
        let normalize = |s: &str| {
            s.to_lowercase()
                .replace("de_", "")
                .replace("cs_", "")
                .replace('_', "")
        };
        normalize(demo_map) == normalize(claimed_map)
    }

    /// Verify that expected players are in the demo.
    ///
    /// Returns (all_present, count_present, total_expected).
    fn verify_players(stats: &Cs2DemoStats, expected_steam_ids: &[String]) -> (bool, usize, usize) {
        if expected_steam_ids.is_empty() {
            return (true, 0, 0);
        }

        let demo_steam_ids = stats.all_steam_ids();
        let present_count = expected_steam_ids
            .iter()
            .filter(|id| demo_steam_ids.contains(id))
            .count();

        (
            present_count == expected_steam_ids.len(),
            present_count,
            expected_steam_ids.len(),
        )
    }

    /// Determine which demo team corresponds to which participant.
    ///
    /// Returns `(participant1_team_name, participant2_team_name, confidence)` or `None` if undetermined.
    fn determine_team_mapping(
        stats: &Cs2DemoStats,
        participant1_steam_ids: &[String],
        participant2_steam_ids: &[String],
    ) -> Option<(String, String, f32)> {
        let team_names: Vec<String> = stats.team_names();
        if team_names.len() != 2 {
            return None;
        }

        let team_a = &team_names[0];
        let team_b = &team_names[1];

        let team_a_ids = stats.steam_ids_for_team(team_a);
        let team_b_ids = stats.steam_ids_for_team(team_b);

        // Count how many participant1 players are in each demo team
        let p1_in_team_a = participant1_steam_ids
            .iter()
            .filter(|id| team_a_ids.contains(id))
            .count();
        let p1_in_team_b = participant1_steam_ids
            .iter()
            .filter(|id| team_b_ids.contains(id))
            .count();

        // Also check participant2 for confirmation
        let p2_in_team_a = participant2_steam_ids
            .iter()
            .filter(|id| team_a_ids.contains(id))
            .count();
        let p2_in_team_b = participant2_steam_ids
            .iter()
            .filter(|id| team_b_ids.contains(id))
            .count();

        let total_p1 = participant1_steam_ids.len().max(1);
        let total_p2 = participant2_steam_ids.len().max(1);

        // Determine mapping based on player distribution
        if p1_in_team_a > p1_in_team_b || p2_in_team_b > p2_in_team_a {
            // Participant 1 is likely team A, Participant 2 is team B
            let confidence = f32::midpoint(
                p1_in_team_a as f32 / total_p1 as f32,
                p2_in_team_b as f32 / total_p2 as f32,
            );
            Some((team_a.clone(), team_b.clone(), confidence))
        } else if p1_in_team_b > p1_in_team_a || p2_in_team_a > p2_in_team_b {
            // Participant 1 is team B, Participant 2 is team A
            let confidence = f32::midpoint(
                p1_in_team_b as f32 / total_p1 as f32,
                p2_in_team_a as f32 / total_p2 as f32,
            );
            Some((team_b.clone(), team_a.clone(), confidence))
        } else if p1_in_team_a > 0 && p2_in_team_b > 0 {
            // Both have some players, go with team A for P1
            let confidence = f32::midpoint(
                p1_in_team_a as f32 / total_p1 as f32,
                p2_in_team_b as f32 / total_p2 as f32,
            )
            .max(0.3);
            Some((team_a.clone(), team_b.clone(), confidence))
        } else {
            // Can't determine
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::cs2::demo_stats::{PlayerState, RoundData, RoundPlayerStats, TeamInfo};
    use std::collections::HashMap;

    fn create_test_stats() -> Cs2DemoStats {
        let team_alpha = TeamInfo {
            team_id: 2,
            team_name: "team_Alpha".to_string(),
            team_side: "T".to_string(),
        };
        let team_beta = TeamInfo {
            team_id: 3,
            team_name: "team_Beta".to_string(),
            team_side: "CT".to_string(),
        };

        let mut teams = HashMap::new();
        teams.insert("team_Alpha".to_string(), team_alpha.clone());
        teams.insert("team_Beta".to_string(), team_beta.clone());

        let mut final_score = HashMap::new();
        final_score.insert("team_Alpha".to_string(), 16);
        final_score.insert("team_Beta".to_string(), 10);

        let mut player_states = HashMap::new();
        player_states.insert(
            "76561198000000001".to_string(),
            PlayerState {
                player_id: 76561198000000001,
                player_name: "Player1".to_string(),
                team: team_alpha,
                starting_money: 800,
            },
        );
        player_states.insert(
            "76561198000000002".to_string(),
            PlayerState {
                player_id: 76561198000000002,
                player_name: "Player2".to_string(),
                team: team_beta,
                starting_money: 800,
            },
        );

        let mut player_stats = HashMap::new();
        player_stats.insert(
            "76561198000000001".to_string(),
            RoundPlayerStats {
                kills: 20,
                deaths: 12,
                assists: 5,
                damage: 2400,
            },
        );
        player_stats.insert(
            "76561198000000002".to_string(),
            RoundPlayerStats {
                kills: 15,
                deaths: 18,
                assists: 3,
                damage: 1800,
            },
        );

        let mut round_score = HashMap::new();
        round_score.insert("team_Alpha".to_string(), 16);
        round_score.insert("team_Beta".to_string(), 10);

        Cs2DemoStats {
            schema_version: 3,
            map: "de_dust2".to_string(),
            match_date: "2024-09-14T20:17:30Z".to_string(),
            demo_file: "test_match.dem".to_string(),
            match_id: "test-match-123".to_string(),
            teams,
            final_score,
            player_summaries: HashMap::new(),
            rounds: vec![RoundData {
                round_number: 1,
                winner_team: "team_Alpha".to_string(),
                winner_side: "T".to_string(),
                round_score,
                player_states,
                events: vec![],
                player_stats,
            }],
        }
    }

    #[test]
    fn test_validate_matching_result() {
        let stats = create_test_stats();
        let claimed = GameResult {
            game_number: 1,
            map_id: Some("de_dust2".to_string()),
            participant1_score: 16,
            participant2_score: 10,
        };

        let p1_ids = vec!["76561198000000001".to_string()];
        let p2_ids = vec!["76561198000000002".to_string()];

        let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

        assert!(result.is_valid);
        assert!(result.confidence > 0.8);
        assert!(result.errors.is_empty());
        assert!(result.extracted_result.is_some());

        let extracted = result.extracted_result.unwrap();
        assert_eq!(extracted.map_id, "de_dust2");
        assert_eq!(extracted.participant1_score, 16);
        assert_eq!(extracted.participant2_score, 10);
    }

    #[test]
    fn test_validate_score_mismatch() {
        let stats = create_test_stats();
        let claimed = GameResult {
            game_number: 1,
            map_id: Some("de_dust2".to_string()),
            participant1_score: 16,
            participant2_score: 14, // Wrong score
        };

        let p1_ids = vec!["76561198000000001".to_string()];
        let p2_ids = vec!["76561198000000002".to_string()];

        let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
        assert!(result.errors.iter().any(|e| e.contains("Score mismatch")));
    }

    #[test]
    fn test_validate_map_mismatch() {
        let stats = create_test_stats();
        let claimed = GameResult {
            game_number: 1,
            map_id: Some("de_mirage".to_string()), // Wrong map
            participant1_score: 16,
            participant2_score: 10,
        };

        let p1_ids = vec!["76561198000000001".to_string()];
        let p2_ids = vec!["76561198000000002".to_string()];

        let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("Map mismatch")));
    }

    #[test]
    fn test_validate_missing_players_warning() {
        let stats = create_test_stats();
        let claimed = GameResult {
            game_number: 1,
            map_id: Some("de_dust2".to_string()),
            participant1_score: 16,
            participant2_score: 10,
        };

        // Include a player not in the demo
        let p1_ids = vec![
            "76561198000000001".to_string(),
            "76561198000000099".to_string(), // Not in demo
        ];
        let p2_ids = vec!["76561198000000002".to_string()];

        let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

        // Should still be valid but with reduced confidence
        assert!(result.is_valid);
        assert!(result.confidence < 1.0);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_map_name_normalization() {
        let stats = create_test_stats();
        let claimed = GameResult {
            game_number: 1,
            map_id: Some("dust2".to_string()), // Without de_ prefix
            participant1_score: 16,
            participant2_score: 10,
        };

        let p1_ids = vec!["76561198000000001".to_string()];
        let p2_ids = vec!["76561198000000002".to_string()];

        let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

        // Should match despite different naming
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_no_map_claim() {
        let stats = create_test_stats();
        let claimed = GameResult {
            game_number: 1,
            map_id: None, // No map claimed
            participant1_score: 16,
            participant2_score: 10,
        };

        let p1_ids = vec!["76561198000000001".to_string()];
        let p2_ids = vec!["76561198000000002".to_string()];

        let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

        // Should be valid when map is not specified
        assert!(result.is_valid);
    }

    #[test]
    fn test_extract_result() {
        let stats = create_test_stats();

        let p1_ids = vec!["76561198000000001".to_string()];
        let p2_ids = vec!["76561198000000002".to_string()];

        let result = Cs2EvidenceValidator::extract_result(&stats, &p1_ids, &p2_ids);

        assert!(result.is_some());
        let extracted = result.unwrap();
        assert_eq!(extracted.map_id, "de_dust2");
        assert_eq!(extracted.participant1_score, 16);
        assert_eq!(extracted.participant2_score, 10);
    }

    #[test]
    fn test_maps_match() {
        assert!(Cs2EvidenceValidator::maps_match("de_dust2", "de_dust2"));
        assert!(Cs2EvidenceValidator::maps_match("de_dust2", "dust2"));
        assert!(Cs2EvidenceValidator::maps_match("dust2", "de_dust2"));
        assert!(Cs2EvidenceValidator::maps_match("DE_DUST2", "de_dust2"));
        assert!(!Cs2EvidenceValidator::maps_match("de_dust2", "de_mirage"));
    }

    // =========================================================================
    // Property tests (N8)
    // =========================================================================
    //
    // These use proptest to explore input spaces the example-based tests
    // above can't cover exhaustively — arbitrary map names and score pairs.
    // The validator is downstream of untrusted input (demo service JSON),
    // and a panic or "valid" return on a malformed claim is a bug, so
    // checking behavior across generated inputs is worth the extra surface.

    use proptest::prelude::*;

    fn stats_with_scores(alpha: i32, beta: i32) -> Cs2DemoStats {
        let mut s = create_test_stats();
        s.final_score.insert("team_Alpha".to_string(), alpha);
        s.final_score.insert("team_Beta".to_string(), beta);
        s
    }

    proptest! {
        /// `maps_match` is reflexive for any non-empty map name: a demo
        /// always validates against its own map id.
        #[test]
        fn maps_match_reflexive(map in "[a-z0-9_]{1,32}") {
            prop_assert!(Cs2EvidenceValidator::maps_match(&map, &map));
        }

        /// Adding or removing the "de_" / "cs_" prefix leaves the match
        /// relation intact (that's the normalization contract the
        /// function advertises).
        #[test]
        fn maps_match_prefix_invariant(base in "[a-z0-9]{3,16}") {
            let plain = base.clone();
            let with_de = format!("de_{base}");
            let with_cs = format!("cs_{base}");
            prop_assert!(Cs2EvidenceValidator::maps_match(&plain, &with_de));
            prop_assert!(Cs2EvidenceValidator::maps_match(&with_de, &plain));
            prop_assert!(Cs2EvidenceValidator::maps_match(&plain, &with_cs));
            prop_assert!(Cs2EvidenceValidator::maps_match(&with_cs, &with_de));
        }

        /// Validating a claim that matches the demo's scores on both sides
        /// is always valid (modulo map/player checks which `create_test_stats`
        /// fixes). Validating a claim that disagrees is never valid.
        #[test]
        fn score_validation_agrees_with_truth(
            alpha_score in 0i32..=32,
            beta_score in 0i32..=32,
            claimed_alpha in 0i32..=32,
            claimed_beta in 0i32..=32,
        ) {
            // Skip ties — the validator checks that the winner side
            // agrees, and the "winner" of a tie is undefined. Real
            // CS2 matches use overtime to avoid ties; fuzzing them
            // only confuses the contract.
            prop_assume!(alpha_score != beta_score);
            prop_assume!(claimed_alpha != claimed_beta);

            let stats = stats_with_scores(alpha_score, beta_score);
            let claim = GameResult {
                game_number: 1,
                map_id: Some("de_dust2".to_string()),
                participant1_score: claimed_alpha,
                participant2_score: claimed_beta,
            };
            let p1 = vec!["76561198000000001".to_string()];
            let p2 = vec!["76561198000000002".to_string()];

            let result = Cs2EvidenceValidator::validate(&stats, &claim, &p1, &p2);

            let scores_agree = alpha_score == claimed_alpha && beta_score == claimed_beta;
            if scores_agree {
                prop_assert!(result.is_valid, "agreeing scores should be valid; got {result:?}");
            } else {
                prop_assert!(!result.is_valid, "disagreeing scores should be invalid; got {result:?}");
            }
        }

        /// Confidence is always in [0.0, 1.0]. A NaN or >1.0 from the
        /// multiplicative confidence pipeline would be a bug.
        #[test]
        fn confidence_stays_in_unit_interval(
            alpha_score in 0i32..=32,
            beta_score in 0i32..=32,
        ) {
            let stats = stats_with_scores(alpha_score, beta_score);
            let claim = GameResult {
                game_number: 1,
                map_id: Some("de_dust2".to_string()),
                participant1_score: alpha_score,
                participant2_score: beta_score,
            };
            let p1 = vec!["76561198000000001".to_string()];
            let p2 = vec!["76561198000000002".to_string()];

            let result = Cs2EvidenceValidator::validate(&stats, &claim, &p1, &p2);
            prop_assert!(result.confidence >= 0.0 && result.confidence <= 1.0,
                "confidence out of unit interval: {}", result.confidence);
            prop_assert!(!result.confidence.is_nan());
        }
    }
}
