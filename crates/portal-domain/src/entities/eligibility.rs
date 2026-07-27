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

    /// Minimum total rating across all team members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_team_total_rating: Option<i32>,

    /// Maximum average rating across all team members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_team_average_rating: Option<i32>,

    /// Minimum average rating across all team members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_team_average_rating: Option<i32>,

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
        self.has_player_restrictions() || self.has_team_restrictions()
    }

    /// Check if any per-player restrictions are configured.
    pub fn has_player_restrictions(&self) -> bool {
        self.max_rating_per_player.is_some()
            || self.min_rating_per_player.is_some()
            || self.max_peak_rating_per_player.is_some()
            || self.max_avg_rating_per_player.is_some()
            || !self.allowed_rank_tiers.is_empty()
            || self.min_matches_played.is_some()
    }

    /// Check if any team-aggregate restrictions are configured.
    pub fn has_team_restrictions(&self) -> bool {
        self.max_team_total_rating.is_some()
            || self.min_team_total_rating.is_some()
            || self.max_team_average_rating.is_some()
            || self.min_team_average_rating.is_some()
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

    /// Why this rule set can never be satisfied, if it can't.
    ///
    /// Each side is validated min<=max on write, but composing two
    /// individually-valid sets can still produce a contradiction: a league
    /// capping team total at 9000 and a tournament requiring 10000 compose
    /// to min>max, which rejects every entrant with a per-registration
    /// message that never says the rules themselves are impossible.
    #[must_use]
    pub fn unsatisfiable_reason(&self) -> Option<String> {
        [
            (
                self.min_rating_per_player,
                self.max_rating_per_player,
                "player rating",
            ),
            (
                self.min_team_total_rating,
                self.max_team_total_rating,
                "team total rating",
            ),
            (
                self.min_team_average_rating,
                self.max_team_average_rating,
                "team average rating",
            ),
        ]
        .into_iter()
        .find_map(|(min, max, label)| match (min, max) {
            (Some(min), Some(max)) if min > max => {
                Some(format!("{label} minimum ({min}) exceeds maximum ({max})"))
            }
            _ => None,
        })
    }

    /// A copy without the minimum-side team bounds.
    ///
    /// For checks against a *subset* of a roster — most importantly the
    /// post-match audit of who actually played. A seven-player roster that
    /// legitimately cleared a team-total floor at registration fields five
    /// starters, whose sub-total is naturally lower; applying the floor to
    /// that lineup flags a fully compliant match. Caps still apply: a lineup
    /// over a maximum is over it however few played.
    #[must_use]
    pub fn without_team_minimums(&self) -> Self {
        Self {
            min_team_total_rating: None,
            min_team_average_rating: None,
            ..self.clone()
        }
    }

    /// Only the team-total rating cap, for enforcement during roster
    /// assembly.
    ///
    /// The total is the one aggregate that grows monotonically with every
    /// addition — once a roster is over a total cap, no later addition can
    /// repair it, so rejecting the offending addition is always correct.
    /// Average caps and all minimum bounds CAN be satisfied by later
    /// additions (a high-rated pickup lowers nothing, but a low-rated one
    /// lowers the average; a thin roster hasn't reached its floor *yet*), so
    /// enforcing them mid-build would reject legal end states. They bind at
    /// commitment points instead: season registration and tournament
    /// registration.
    #[must_use]
    pub fn team_total_cap_only(&self) -> Self {
        Self {
            max_team_total_rating: self.max_team_total_rating,
            ..Self::default()
        }
    }

    /// Compose two restriction sets, keeping the stricter bound on every
    /// axis. Used to enforce a league's entry requirements on tournaments
    /// inside it: the tournament may tighten league rules, never loosen
    /// them.
    ///
    /// Maxima take the smaller value, minima the larger; `allowed_rank_tiers`
    /// intersects when both sides restrict (an empty list means
    /// unrestricted).
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        fn tighter_max(a: Option<i32>, b: Option<i32>) -> Option<i32> {
            match (a, b) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (x, None) | (None, x) => x,
            }
        }
        fn tighter_min(a: Option<i32>, b: Option<i32>) -> Option<i32> {
            match (a, b) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (x, None) | (None, x) => x,
            }
        }

        let allowed_rank_tiers = match (
            self.allowed_rank_tiers.is_empty(),
            other.allowed_rank_tiers.is_empty(),
        ) {
            (true, _) => other.allowed_rank_tiers.clone(),
            (_, true) => self.allowed_rank_tiers.clone(),
            (false, false) => {
                let common: Vec<String> = self
                    .allowed_rank_tiers
                    .iter()
                    .filter(|t| other.allowed_rank_tiers.contains(t))
                    .cloned()
                    .collect();
                if common.is_empty() {
                    // Disjoint restrictions: an empty list means UNRESTRICTED
                    // to the evaluator, which would compose two conflicting
                    // tier rules into no rule at all — the opposite of
                    // strictest-wins. A sentinel no real tier id can match
                    // keeps the correct outcome (nobody qualifies) and reads
                    // as an explanation in the violation message.
                    vec!["(no tier satisfies both restrictions)".to_string()]
                } else {
                    common
                }
            }
        };

        Self {
            max_rating_per_player: tighter_max(
                self.max_rating_per_player,
                other.max_rating_per_player,
            ),
            min_rating_per_player: tighter_min(
                self.min_rating_per_player,
                other.min_rating_per_player,
            ),
            max_peak_rating_per_player: tighter_max(
                self.max_peak_rating_per_player,
                other.max_peak_rating_per_player,
            ),
            max_avg_rating_per_player: tighter_max(
                self.max_avg_rating_per_player,
                other.max_avg_rating_per_player,
            ),
            max_team_total_rating: tighter_max(
                self.max_team_total_rating,
                other.max_team_total_rating,
            ),
            min_team_total_rating: tighter_min(
                self.min_team_total_rating,
                other.min_team_total_rating,
            ),
            max_team_average_rating: tighter_max(
                self.max_team_average_rating,
                other.max_team_average_rating,
            ),
            min_team_average_rating: tighter_min(
                self.min_team_average_rating,
                other.min_team_average_rating,
            ),
            allowed_rank_tiers,
            min_matches_played: tighter_min(self.min_matches_played, other.min_matches_played),
        }
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
