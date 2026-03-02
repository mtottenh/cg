//! Eligibility restriction types for tournaments and leagues.

use serde::{Deserialize, Serialize};

/// Eligibility restrictions that can be applied to tournaments or leagues.
///
/// Stored in the `settings` JSONB column under the key `"eligibility"`.
/// All fields are optional — only set restrictions are enforced.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EligibilityRestrictions {
    /// Maximum current rating for any individual player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rating_per_player: Option<i32>,

    /// Minimum current rating for any individual player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_rating_per_player: Option<i32>,

    /// Maximum peak rating for any player (anti-smurf check).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_peak_rating_per_player: Option<i32>,

    /// Maximum average rating for any player (computed from history).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_avg_rating_per_player: Option<i32>,

    /// Maximum total rating across all team members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_team_total_rating: Option<i32>,

    /// Maximum average rating across all team members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_team_average_rating: Option<i32>,

    /// Allowed rank tier IDs (empty means all tiers are allowed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_rank_tiers: Vec<String>,

    /// Minimum number of matches played to be eligible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_matches_played: Option<i32>,
}

impl EligibilityRestrictions {
    /// Check if any restrictions are configured.
    pub fn has_restrictions(&self) -> bool {
        self.max_rating_per_player.is_some()
            || self.min_rating_per_player.is_some()
            || self.max_peak_rating_per_player.is_some()
            || self.max_avg_rating_per_player.is_some()
            || self.max_team_total_rating.is_some()
            || self.max_team_average_rating.is_some()
            || !self.allowed_rank_tiers.is_empty()
            || self.min_matches_played.is_some()
    }

    /// Parse eligibility restrictions from a settings JSON value.
    ///
    /// Looks for the `"eligibility"` key in the settings object.
    /// Returns default (no restrictions) if the key is missing or malformed.
    pub fn from_settings(settings: &serde_json::Value) -> Self {
        settings
            .get("eligibility")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// A single eligibility violation found during registration validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EligibilityViolation {
    /// The player who violated the restriction (zero UUID for team-level violations).
    pub player_id: portal_core::PlayerId,
    /// The restriction key that was violated.
    pub restriction: String,
    /// Human-readable message explaining the violation.
    pub message: String,
}
