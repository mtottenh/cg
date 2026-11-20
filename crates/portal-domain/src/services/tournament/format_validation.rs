//! Validation of a tournament's `format_settings`.
//!
//! The settings are free-form JSON interpreted at *start* time (groups
//! config, per-round match formats). Interpreting them only then meant a
//! typo or an impossible group shape was discovered after the start had
//! already claimed the tournament, leaving it `in_progress` with no
//! brackets. These checks run at create/update time (against
//! `max_participants`, the most the event can hold) and again before the
//! start claims anything (against the actual participant count).

use portal_core::DomainError;
use portal_core::types::{MatchFormat, MatchFormatPlan, TournamentFormat};

use super::bracket_generator::groups::GroupsConfig;

/// Largest round-robin bracket (standalone, or one group) we generate.
///
/// A round robin of N plays N·(N−1)/2 matches — 32 is already 496
/// matches; beyond that the event cannot realistically be played and the
/// generation itself becomes a liability.
pub const MAX_ROUND_ROBIN_BRACKET_SIZE: usize = 32;

/// Validate settings that can be checked without knowing who registered:
/// parseable round formats, a coherent groups config, and a shape that is
/// possible at `max_participants`.
pub fn validate_format_settings(
    format: TournamentFormat,
    settings: &serde_json::Value,
    default_match_format: MatchFormat,
    max_participants: i32,
) -> Result<(), DomainError> {
    let max = usize::try_from(max_participants.max(0)).unwrap_or(0);
    validate_plan(settings, default_match_format)?;
    match format {
        TournamentFormat::RoundRobin if max > MAX_ROUND_ROBIN_BRACKET_SIZE => {
            Err(round_robin_too_large(max))
        }
        TournamentFormat::GroupsAndPlayoffs => {
            let config = GroupsConfig::from_format_settings(settings, max)?;
            // At full capacity the smallest group holds max / group_count.
            let min_group = max / config.group_count;
            if min_group < 2 {
                return Err(DomainError::InvalidState(format!(
                    "{} groups cannot each hold 2 of at most {max} participants",
                    config.group_count
                )));
            }
            if config.advance_per_group > min_group {
                return Err(DomainError::InvalidState(format!(
                    "advance_per_group ({}) exceeds the group size ({min_group}) even at \
                     max_participants ({max})",
                    config.advance_per_group
                )));
            }
            let largest_group = max.div_ceil(config.group_count);
            if largest_group > MAX_ROUND_ROBIN_BRACKET_SIZE {
                return Err(round_robin_too_large(largest_group));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Validate settings against the participants actually starting.
///
/// Run before the start claims the tournament, so an impossible configuration is a 400
/// that leaves the event startable instead of a half-started tournament.
pub fn validate_for_start(
    format: TournamentFormat,
    settings: &serde_json::Value,
    default_match_format: MatchFormat,
    participant_count: usize,
) -> Result<(), DomainError> {
    validate_plan(settings, default_match_format)?;
    match format {
        TournamentFormat::RoundRobin if participant_count > MAX_ROUND_ROBIN_BRACKET_SIZE => {
            Err(round_robin_too_large(participant_count))
        }
        TournamentFormat::GroupsAndPlayoffs => {
            let config = GroupsConfig::from_format_settings(settings, participant_count)?;
            let min_per_group = participant_count / config.group_count;
            if min_per_group < 2 {
                return Err(DomainError::InvalidState(format!(
                    "Not enough participants ({participant_count}) for {} groups (need at \
                     least 2 per group)",
                    config.group_count
                )));
            }
            if config.advance_per_group > min_per_group {
                return Err(DomainError::InvalidState(format!(
                    "advance_per_group ({}) exceeds minimum group size ({min_per_group})",
                    config.advance_per_group
                )));
            }
            let largest_group = participant_count.div_ceil(config.group_count);
            if largest_group > MAX_ROUND_ROBIN_BRACKET_SIZE {
                return Err(round_robin_too_large(largest_group));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_plan(
    settings: &serde_json::Value,
    default_match_format: MatchFormat,
) -> Result<(), DomainError> {
    MatchFormatPlan::from_settings(default_match_format, Some(settings))
        .map(|_| ())
        .map_err(|e| DomainError::InvalidState(format!("invalid format settings: {e}")))
}

fn round_robin_too_large(size: usize) -> DomainError {
    DomainError::InvalidState(format!(
        "A round-robin bracket of {size} participants is too large (max \
         {MAX_ROUND_ROBIN_BRACKET_SIZE}); use groups"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_impossible_group_shapes_at_create() {
        let ok = json!({ "group_count": 2, "advance_per_group": 2 });
        assert!(
            validate_format_settings(
                TournamentFormat::GroupsAndPlayoffs,
                &ok,
                MatchFormat::Bo3,
                8
            )
            .is_ok()
        );
        let too_many = json!({ "group_count": 4, "advance_per_group": 3 });
        assert!(
            validate_format_settings(
                TournamentFormat::GroupsAndPlayoffs,
                &too_many,
                MatchFormat::Bo3,
                8
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_bad_round_formats_and_oversized_round_robins() {
        let bad = json!({ "round_formats": { "1": "bo4" } });
        assert!(
            validate_format_settings(TournamentFormat::RoundRobin, &bad, MatchFormat::Bo3, 8)
                .is_err()
        );
        assert!(
            validate_format_settings(
                TournamentFormat::RoundRobin,
                &json!({}),
                MatchFormat::Bo3,
                64
            )
            .is_err()
        );
    }

    #[test]
    fn start_checks_the_actual_group_size() {
        let settings = json!({ "group_count": 2, "advance_per_group": 3 });
        assert!(
            validate_for_start(
                TournamentFormat::GroupsAndPlayoffs,
                &settings,
                MatchFormat::Bo3,
                4
            )
            .is_err()
        );
        assert!(
            validate_for_start(
                TournamentFormat::GroupsAndPlayoffs,
                &settings,
                MatchFormat::Bo3,
                6
            )
            .is_ok()
        );
    }
}
