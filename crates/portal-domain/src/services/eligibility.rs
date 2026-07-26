//! Eligibility checking logic for tournament registration.

use crate::entities::PlayerGameProfile;
use crate::entities::eligibility::{EligibilityRestrictions, EligibilityViolation};
use crate::repositories::player_rating_history::RatingStats;
use portal_core::PlayerId;

/// Default rating used when a player has no profile for the game.
const DEFAULT_RATING: i32 = 1500;
const DEFAULT_PEAK: i32 = 1500;

/// Check individual players against the per-player restrictions only.
///
/// Team-aggregate bounds are deliberately NOT evaluated here: they only make
/// sense against a real roster. Running them on a lone joining player (as the
/// league-join path once did) silently reinterprets a team cap as a
/// per-player cap.
///
/// Each player is represented by their ID, optional game profile, and optional
/// rating stats (from history). Players without a profile are treated as having
/// default values (rating=1500, peak=1500, matches=0, no rank tier).
///
/// Returns an empty list if all players pass. Otherwise returns one violation
/// per failed check.
pub fn check_player_eligibility(
    restrictions: &EligibilityRestrictions,
    player_data: &[(PlayerId, Option<PlayerGameProfile>, Option<RatingStats>)],
) -> Vec<EligibilityViolation> {
    if !restrictions.has_player_restrictions() {
        return vec![];
    }

    let mut violations = Vec::new();

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

    violations
}

/// Check a team roster: every per-player restriction plus the team-aggregate
/// bounds (total and average rating, min and max sides).
///
/// The aggregate is computed over exactly the players passed in — callers
/// decide whether that's a seasonal roster or a match lineup.
pub fn check_team_eligibility(
    restrictions: &EligibilityRestrictions,
    player_data: &[(PlayerId, Option<PlayerGameProfile>, Option<RatingStats>)],
) -> Vec<EligibilityViolation> {
    let mut violations = check_player_eligibility(restrictions, player_data);

    if !restrictions.has_team_restrictions() || player_data.is_empty() {
        return violations;
    }

    let team_violation = |restriction: &str, message: String| EligibilityViolation {
        player_id: PlayerId::from_uuid(uuid::Uuid::nil()),
        restriction: restriction.to_string(),
        message,
    };

    let total_rating: i32 = player_data
        .iter()
        .map(|(_, p, _)| p.as_ref().map_or(DEFAULT_RATING, |p| p.rating))
        .sum();
    let count = player_data.len() as i32;
    let avg = total_rating / count;

    if let Some(max_total) = restrictions.max_team_total_rating
        && total_rating > max_total
    {
        violations.push(team_violation(
            "max_team_total_rating",
            format!("Team total rating ({total_rating}) exceeds maximum allowed ({max_total})"),
        ));
    }

    if let Some(min_total) = restrictions.min_team_total_rating
        && total_rating < min_total
    {
        violations.push(team_violation(
            "min_team_total_rating",
            format!("Team total rating ({total_rating}) is below minimum required ({min_total})"),
        ));
    }

    if let Some(max_avg) = restrictions.max_team_average_rating
        && avg > max_avg
    {
        violations.push(team_violation(
            "max_team_average_rating",
            format!("Team average rating ({avg}) exceeds maximum allowed ({max_avg})"),
        ));
    }

    if let Some(min_avg) = restrictions.min_team_average_rating
        && avg < min_avg
    {
        violations.push(team_violation(
            "min_team_average_rating",
            format!("Team average rating ({avg}) is below minimum required ({min_avg})"),
        ));
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(rating: i32) -> (PlayerId, Option<PlayerGameProfile>, Option<RatingStats>) {
        let now = chrono::Utc::now();
        let profile = PlayerGameProfile {
            id: portal_core::PlayerGameProfileId::new(),
            player_id: PlayerId::new(),
            game_id: portal_core::GameId::new(),
            rating,
            rating_deviation: 350,
            volatility: 0.06,
            peak_rating: rating,
            peak_rating_at: None,
            rank_tier: None,
            rank_division: None,
            rank_points: None,
            matches_played: 100,
            wins: 0,
            losses: 0,
            draws: 0,
            win_streak: 0,
            best_win_streak: 0,
            total_playtime_minutes: 0,
            game_specific_stats: serde_json::json!({}),
            first_match_at: None,
            last_match_at: None,
            created_at: now,
            updated_at: now,
        };
        (profile.player_id, Some(profile), None)
    }

    fn team_restrictions() -> EligibilityRestrictions {
        EligibilityRestrictions {
            max_team_average_rating: Some(2000),
            min_team_average_rating: Some(1000),
            max_team_total_rating: Some(10000),
            min_team_total_rating: Some(5000),
            ..EligibilityRestrictions::default()
        }
    }

    #[test]
    fn player_check_never_evaluates_team_bounds() {
        // The league-join hazard: one joining player, team caps configured.
        // Their personal rating (1500) is far below min_team_total (5000) —
        // a naive aggregate over the singleton would reject them.
        let violations = check_player_eligibility(&team_restrictions(), &[player(1500)]);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn team_check_enforces_min_and_max_aggregates() {
        let r = team_restrictions();

        // 5 × 1500: total 7500, avg 1500 — inside every bound.
        let ok: Vec<_> = (0..5).map(|_| player(1500)).collect();
        assert!(check_team_eligibility(&r, &ok).is_empty());

        // 5 × 800: total 4000 < 5000 and avg 800 < 1000.
        let weak: Vec<_> = (0..5).map(|_| player(800)).collect();
        let violations = check_team_eligibility(&r, &weak);
        let keys: Vec<_> = violations.iter().map(|v| v.restriction.as_str()).collect();
        assert!(keys.contains(&"min_team_total_rating"), "{keys:?}");
        assert!(keys.contains(&"min_team_average_rating"), "{keys:?}");

        // 5 × 2500: total 12500 > 10000 and avg 2500 > 2000.
        let strong: Vec<_> = (0..5).map(|_| player(2500)).collect();
        let violations = check_team_eligibility(&r, &strong);
        let keys: Vec<_> = violations.iter().map(|v| v.restriction.as_str()).collect();
        assert!(keys.contains(&"max_team_total_rating"), "{keys:?}");
        assert!(keys.contains(&"max_team_average_rating"), "{keys:?}");
    }

    #[test]
    fn team_check_includes_per_player_rules() {
        let r = EligibilityRestrictions {
            min_rating_per_player: Some(1200),
            ..EligibilityRestrictions::default()
        };
        let roster = vec![player(1500), player(900)];
        let violations = check_team_eligibility(&r, &roster);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].restriction, "min_rating_per_player");
    }

    #[test]
    fn without_team_minimums_keeps_caps() {
        let r = team_restrictions().without_team_minimums();
        // A two-player roster being assembled: min bounds must not fire...
        let building = vec![player(1500), player(1500)];
        assert!(check_team_eligibility(&r, &building).is_empty());
        // ...but the caps still do.
        let heavy = vec![player(4000), player(4000), player(4000)];
        let keys: Vec<_> = check_team_eligibility(&r, &heavy)
            .iter()
            .map(|v| v.restriction.clone())
            .collect();
        assert!(keys.contains(&"max_team_total_rating".to_string()), "{keys:?}");
    }

    #[test]
    fn intersect_keeps_stricter_bounds() {
        let league = EligibilityRestrictions {
            min_rating_per_player: Some(12000),
            max_team_average_rating: Some(20000),
            allowed_rank_tiers: vec!["gold".into(), "silver".into()],
            ..EligibilityRestrictions::default()
        };
        let tournament = EligibilityRestrictions {
            min_rating_per_player: Some(10000),
            max_rating_per_player: Some(18000),
            max_team_average_rating: Some(16000),
            allowed_rank_tiers: vec!["silver".into(), "bronze".into()],
            ..EligibilityRestrictions::default()
        };
        let combined = league.intersect(&tournament);
        // The tournament may not loosen the league's floor.
        assert_eq!(combined.min_rating_per_player, Some(12000));
        assert_eq!(combined.max_rating_per_player, Some(18000));
        assert_eq!(combined.max_team_average_rating, Some(16000));
        assert_eq!(combined.allowed_rank_tiers, vec!["silver".to_string()]);
    }
}
