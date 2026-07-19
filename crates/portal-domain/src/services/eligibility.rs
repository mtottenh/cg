//! Eligibility checking logic for tournament registration.

use crate::entities::PlayerGameProfile;
use crate::entities::eligibility::{EligibilityRestrictions, EligibilityViolation};
use crate::repositories::player_rating_history::RatingStats;
use portal_core::PlayerId;

/// Default rating used when a player has no profile for the game.
const DEFAULT_RATING: i32 = 1500;
const DEFAULT_PEAK: i32 = 1500;

/// Check a set of players against eligibility restrictions.
///
/// Each player is represented by their ID, optional game profile, and optional
/// rating stats (from history). Players without a profile are treated as having
/// default values (rating=1500, peak=1500, matches=0, no rank tier).
///
/// Returns an empty list if all players pass. Otherwise returns one violation
/// per failed check.
pub fn check_eligibility(
    restrictions: &EligibilityRestrictions,
    player_data: &[(PlayerId, Option<PlayerGameProfile>, Option<RatingStats>)],
) -> Vec<EligibilityViolation> {
    if !restrictions.has_restrictions() {
        return vec![];
    }

    let mut violations = Vec::new();

    // Per-player checks
    for (player_id, profile, stats) in player_data {
        let rating = profile.as_ref().map_or(DEFAULT_RATING, |p| p.rating);
        let peak_rating = profile.as_ref().map_or(DEFAULT_PEAK, |p| p.peak_rating);
        let matches_played = profile.as_ref().map_or(0, |p| p.matches_played);
        let rank_tier = profile.as_ref().and_then(|p| p.rank_tier.clone());

        if let Some(max) = restrictions.max_rating_per_player
            && rating > max
        {
            violations.push(EligibilityViolation {
                player_id: *player_id,
                restriction: "max_rating_per_player".to_string(),
                message: format!("Player rating ({rating}) exceeds maximum allowed ({max})"),
            });
        }

        if let Some(min) = restrictions.min_rating_per_player
            && rating < min
        {
            violations.push(EligibilityViolation {
                player_id: *player_id,
                restriction: "min_rating_per_player".to_string(),
                message: format!("Player rating ({rating}) is below minimum required ({min})"),
            });
        }

        if let Some(max_peak) = restrictions.max_peak_rating_per_player
            && peak_rating > max_peak
        {
            violations.push(EligibilityViolation {
                player_id: *player_id,
                restriction: "max_peak_rating_per_player".to_string(),
                message: format!(
                    "Player peak rating ({peak_rating}) exceeds maximum allowed ({max_peak})"
                ),
            });
        }

        if let Some(max_avg) = restrictions.max_avg_rating_per_player
            && let Some(s) = stats
        {
            let avg = s.average_rating as i32;
            if avg > max_avg {
                violations.push(EligibilityViolation {
                    player_id: *player_id,
                    restriction: "max_avg_rating_per_player".to_string(),
                    message: format!(
                        "Player average rating ({avg}) exceeds maximum allowed ({max_avg})"
                    ),
                });
            }
        }

        if let Some(min_matches) = restrictions.min_matches_played
            && matches_played < min_matches
        {
            violations.push(EligibilityViolation {
                player_id: *player_id,
                restriction: "min_matches_played".to_string(),
                message: format!(
                    "Player has played {matches_played} matches, minimum required is {min_matches}"
                ),
            });
        }

        if !restrictions.allowed_rank_tiers.is_empty() {
            let tier = rank_tier.as_deref().unwrap_or("unranked");
            if !restrictions.allowed_rank_tiers.iter().any(|t| t == tier) {
                violations.push(EligibilityViolation {
                    player_id: *player_id,
                    restriction: "allowed_rank_tiers".to_string(),
                    message: format!(
                        "Player rank tier '{tier}' is not in the allowed tiers: {:?}",
                        restrictions.allowed_rank_tiers
                    ),
                });
            }
        }
    }

    // Team-aggregate checks
    if restrictions.max_team_total_rating.is_some()
        || restrictions.max_team_average_rating.is_some()
    {
        let total_rating: i32 = player_data
            .iter()
            .map(|(_, p, _)| p.as_ref().map_or(DEFAULT_RATING, |p| p.rating))
            .sum();
        let count = player_data.len() as i32;

        if let Some(max_total) = restrictions.max_team_total_rating
            && total_rating > max_total
        {
            violations.push(EligibilityViolation {
                player_id: PlayerId::from_uuid(uuid::Uuid::nil()),
                restriction: "max_team_total_rating".to_string(),
                message: format!(
                    "Team total rating ({total_rating}) exceeds maximum allowed ({max_total})"
                ),
            });
        }

        if let Some(max_avg) = restrictions.max_team_average_rating {
            let avg = if count > 0 { total_rating / count } else { 0 };
            if avg > max_avg {
                violations.push(EligibilityViolation {
                    player_id: PlayerId::from_uuid(uuid::Uuid::nil()),
                    restriction: "max_team_average_rating".to_string(),
                    message: format!(
                        "Team average rating ({avg}) exceeds maximum allowed ({max_avg})"
                    ),
                });
            }
        }
    }

    violations
}
