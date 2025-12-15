//! Demo validation domain entities.
//!
//! Types for validating demo files against claimed match results.

use serde::{Deserialize, Serialize};

/// Result of validating a demo against a claimed match result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoValidationResult {
    /// Whether the validation passed all critical checks.
    pub is_valid: bool,
    /// Confidence score (0.0 to 1.0) based on roster matches and other factors.
    pub confidence: f32,
    /// Score extracted from the demo (team1_score, team2_score).
    pub extracted_score: Option<(i32, i32)>,
    /// The claimed score being validated against.
    pub claimed_score: (i32, i32),
    /// Whether the map name matches the claimed map.
    pub map_match: bool,
    /// Non-fatal issues that reduce confidence but don't invalidate.
    pub warnings: Vec<String>,
    /// Fatal issues that invalidate the demo as evidence.
    pub errors: Vec<String>,
}

impl DemoValidationResult {
    /// Create a valid result with high confidence.
    #[must_use]
    pub fn valid(extracted_score: (i32, i32), claimed_score: (i32, i32)) -> Self {
        Self {
            is_valid: true,
            confidence: 1.0,
            extracted_score: Some(extracted_score),
            claimed_score,
            map_match: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Create an invalid result with an error.
    #[must_use]
    pub fn invalid(error: impl Into<String>, claimed_score: (i32, i32)) -> Self {
        Self {
            is_valid: false,
            confidence: 0.0,
            extracted_score: None,
            claimed_score,
            map_match: false,
            warnings: Vec::new(),
            errors: vec![error.into()],
        }
    }

    /// Check if there's a roster mismatch warning.
    #[must_use]
    pub fn has_roster_mismatch(&self) -> bool {
        self.warnings.iter().any(|w| {
            let lower = w.to_lowercase();
            lower.contains("player")
                || lower.contains("roster")
                || lower.contains("unrecognized")
        })
    }

    /// Check if there's a score mismatch error.
    #[must_use]
    pub fn has_score_mismatch(&self) -> bool {
        self.errors.iter().any(|e| e.contains("Score mismatch"))
    }

    /// Check if there's a winner mismatch error.
    #[must_use]
    pub fn has_winner_mismatch(&self) -> bool {
        self.errors.iter().any(|e| e.contains("Winner mismatch"))
    }

    /// Check if there's a map mismatch error.
    #[must_use]
    pub fn has_map_mismatch(&self) -> bool {
        self.errors.iter().any(|e| e.contains("Map mismatch"))
    }

    /// Add a warning and reduce confidence.
    pub fn add_warning(&mut self, warning: impl Into<String>, confidence_penalty: f32) {
        self.warnings.push(warning.into());
        self.confidence *= 1.0 - confidence_penalty;
    }

    /// Add an error and mark as invalid.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.is_valid = false;
        self.confidence = 0.0;
    }
}

impl Default for DemoValidationResult {
    fn default() -> Self {
        Self {
            is_valid: false,
            confidence: 0.0,
            extracted_score: None,
            claimed_score: (0, 0),
            map_match: false,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }
}

/// A player found in a demo but not on either team's roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrecognizedPlayer {
    /// The player's Steam ID from the demo.
    pub steam_id: String,
    /// The player's name as it appeared in the demo.
    pub player_name: String,
    /// Which team side this player was on in the demo.
    pub team_side: TeamSide,
    /// The team number (1 or 2) in registration terms.
    pub registration_side: i32,
}

/// Which team side a player was on in a demo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamSide {
    /// The first team (participant 1).
    Team1,
    /// The second team (participant 2).
    Team2,
}

impl std::fmt::Display for TeamSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Team1 => write!(f, "team1"),
            Self::Team2 => write!(f, "team2"),
        }
    }
}

/// Container for demos linked to a match with full data.
#[derive(Debug, Clone)]
pub struct MatchDemoValidation {
    /// The demo validation results for each linked demo.
    pub demos: Vec<DemoValidationEntry>,
    /// Overall validation status for the match.
    pub is_valid: bool,
    /// Any unrecognized players across all demos.
    pub unrecognized_players: Vec<UnrecognizedPlayer>,
}

/// A single demo's validation entry in a match context.
#[derive(Debug, Clone)]
pub struct DemoValidationEntry {
    /// The demo match link ID.
    pub link_id: portal_core::DemoMatchLinkId,
    /// The demo ID.
    pub demo_id: portal_core::DemoId,
    /// Game number this demo corresponds to (if known).
    pub game_number: Option<i32>,
    /// Validation result for this demo.
    pub validation: DemoValidationResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_result() {
        let result = DemoValidationResult::valid((16, 10), (16, 10));
        assert!(result.is_valid);
        assert!((result.confidence - 1.0).abs() < f32::EPSILON);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_invalid_result() {
        let result = DemoValidationResult::invalid("Score mismatch", (16, 14));
        assert!(!result.is_valid);
        assert!(result.has_score_mismatch());
    }

    #[test]
    fn test_add_warning() {
        let mut result = DemoValidationResult::valid((16, 10), (16, 10));
        result.add_warning("Missing player", 0.3);
        assert!(result.is_valid);
        assert!(result.confidence < 1.0);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_add_error() {
        let mut result = DemoValidationResult::valid((16, 10), (16, 10));
        result.add_error("Fatal error");
        assert!(!result.is_valid);
        assert!(result.confidence == 0.0);
    }

    #[test]
    fn test_has_roster_mismatch() {
        let mut result = DemoValidationResult::default();
        result.warnings.push("Unrecognized player found".to_string());
        assert!(result.has_roster_mismatch());
    }

    #[test]
    fn test_team_side_display() {
        assert_eq!(TeamSide::Team1.to_string(), "team1");
        assert_eq!(TeamSide::Team2.to_string(), "team2");
    }
}
