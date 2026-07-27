//! Shared eligibility-restrictions input DTO.
//!
//! Used by both tournament and league create/update requests so the two
//! surfaces accept the same vocabulary and get the same validation. The
//! typed input is folded into the entity's `settings` JSONB under the
//! `"eligibility"` key (the shape `EligibilityRestrictions::from_settings`
//! reads back).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

/// Typed input for eligibility restrictions.
///
/// All fields are optional — only specified fields are enforced. Rating
/// values are on the game's own rating scale (e.g. CS Rating 0–35000).
#[derive(Debug, Clone, Deserialize, Serialize, Validate, ToSchema)]
#[validate(schema(function = "validate_restriction_bounds"))]
pub struct EligibilityRestrictionsInput {
    /// Max current rating for any individual player.
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rating_per_player: Option<i32>,

    /// Min current rating for any individual player.
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_rating_per_player: Option<i32>,

    /// Max peak (all-time high) rating for any player (anti-smurf).
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_peak_rating_per_player: Option<i32>,

    /// Max average rating for any player (computed from history).
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_avg_rating_per_player: Option<i32>,

    /// Max sum of all team members' current ratings.
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_team_total_rating: Option<i32>,

    /// Min sum of all team members' current ratings.
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_team_total_rating: Option<i32>,

    /// Max average of team members' current ratings.
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_team_average_rating: Option<i32>,

    /// Min average of team members' current ratings.
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_team_average_rating: Option<i32>,

    /// Only allow players in certain rank tiers (e.g., `["silver", "gold"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_rank_tiers: Vec<String>,

    /// Min matches played to be eligible.
    #[validate(range(min = 0))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_matches_played: Option<i32>,
}

/// Struct-level check: every min/max pair must be satisfiable. Without this,
/// `min_rating: 2000, max_rating: 1000` persists silently and creates an
/// entity nobody can ever join.
fn validate_restriction_bounds(
    input: &EligibilityRestrictionsInput,
) -> Result<(), validator::ValidationError> {
    let pairs = [
        (
            input.min_rating_per_player,
            input.max_rating_per_player,
            "min_rating_per_player exceeds max_rating_per_player",
        ),
        (
            input.min_team_total_rating,
            input.max_team_total_rating,
            "min_team_total_rating exceeds max_team_total_rating",
        ),
        (
            input.min_team_average_rating,
            input.max_team_average_rating,
            "min_team_average_rating exceeds max_team_average_rating",
        ),
    ];
    for (min, max, message) in pairs {
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            let mut err = validator::ValidationError::new("unsatisfiable_bounds");
            err.message = Some(message.into());
            return Err(err);
        }
    }
    Ok(())
}

/// Merge an optional typed eligibility input into the settings JSON.
///
/// Only touches the `"eligibility"` key — other keys in the supplied
/// settings object pass through. (Whole-object persistence semantics are the
/// service layer's concern; see the settings merge there.)
pub(crate) fn merge_eligibility_into_settings(
    settings: Option<serde_json::Value>,
    eligibility: Option<EligibilityRestrictionsInput>,
) -> Option<serde_json::Value> {
    let Some(eligibility) = eligibility else {
        return settings;
    };

    let eligibility_json = serde_json::to_value(eligibility).unwrap_or_default();

    let mut settings = settings.unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = settings.as_object_mut() {
        obj.insert("eligibility".to_string(), eligibility_json);
    }
    Some(settings)
}
