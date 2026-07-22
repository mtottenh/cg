//! Groups + Playoffs bracket generation.
//!
//! Handles distributing participants into groups using snake-draft seeding
//! and cross-seeding group winners for playoff brackets.

use crate::entities::tournament::SeededParticipant;
use portal_core::DomainError;
use serde::Deserialize;

/// Configuration for a Groups + Playoffs tournament.
#[derive(Debug, Clone)]
pub struct GroupsConfig {
    /// Number of groups.
    pub group_count: usize,
    /// How many participants advance from each group.
    pub advance_per_group: usize,
    /// Format used within each group.
    pub group_format: GroupStageFormat,
    /// Format used for the playoff bracket.
    pub playoff_format: PlayoffFormat,
}

/// Format for the group stage brackets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupStageFormat {
    RoundRobin,
    Swiss,
}

/// Format for the playoff bracket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayoffFormat {
    SingleElimination,
    DoubleElimination,
}

impl GroupsConfig {
    /// Parse a `GroupsConfig` from tournament `format_settings` JSON.
    ///
    /// Defaults:
    /// - `group_count`: `max(2, participant_count / 4)`
    /// - `advance_per_group`: 2
    /// - `group_format`: `"round_robin"`
    /// - `playoff_format`: `"single_elimination"`
    pub fn from_format_settings(
        settings: &serde_json::Value,
        participant_count: usize,
    ) -> Result<Self, DomainError> {
        let group_count = settings
            .get("group_count")
            .and_then(serde_json::Value::as_u64)
            .map_or_else(|| (participant_count / 4).max(2), |v| v as usize);

        let advance_per_group = settings
            .get("advance_per_group")
            .and_then(serde_json::Value::as_u64)
            .map_or(2, |v| v as usize);

        let group_format = settings
            .get("group_format")
            .and_then(|v| v.as_str())
            .map_or(GroupStageFormat::RoundRobin, |s| match s {
                "swiss" => GroupStageFormat::Swiss,
                _ => GroupStageFormat::RoundRobin,
            });

        let playoff_format = settings
            .get("playoff_format")
            .and_then(|v| v.as_str())
            .map_or(PlayoffFormat::SingleElimination, |s| match s {
                "double_elimination" => PlayoffFormat::DoubleElimination,
                _ => PlayoffFormat::SingleElimination,
            });

        if group_count < 2 {
            return Err(DomainError::InvalidState(
                "Groups + Playoffs requires at least 2 groups".to_string(),
            ));
        }

        if advance_per_group == 0 {
            return Err(DomainError::InvalidState(
                "advance_per_group must be at least 1".to_string(),
            ));
        }

        let total_advancing = group_count * advance_per_group;
        if total_advancing < 2 {
            return Err(DomainError::InvalidState(
                "Total advancing participants must be at least 2".to_string(),
            ));
        }

        Ok(Self {
            group_count,
            advance_per_group,
            group_format,
            playoff_format,
        })
    }
}

/// Distribute participants into groups using snake-draft seeding.
///
/// Seeds: 1→A, 2→B, 3→C, 4→D, 5→D, 6→C, 7→B, 8→A, 9→A, ...
///
/// This ensures balanced groups where the top seeds are spread evenly.
pub fn distribute_into_groups(
    participants: Vec<SeededParticipant>,
    group_count: usize,
) -> Result<Vec<Vec<SeededParticipant>>, DomainError> {
    let n = participants.len();

    if n < 2 {
        return Err(DomainError::InsufficientParticipants);
    }

    if group_count < 2 {
        return Err(DomainError::InvalidState(
            "Need at least 2 groups".to_string(),
        ));
    }

    if group_count > n {
        return Err(DomainError::InvalidState(format!(
            "Cannot create {group_count} groups from {n} participants"
        )));
    }

    let mut groups: Vec<Vec<SeededParticipant>> = (0..group_count).map(|_| Vec::new()).collect();

    for (i, participant) in participants.into_iter().enumerate() {
        // Snake draft: row = i / group_count
        // Even rows go left-to-right (0, 1, 2, ..., K-1)
        // Odd rows go right-to-left (K-1, K-2, ..., 0)
        let row = i / group_count;
        let col = i % group_count;
        let group_idx = if row.is_multiple_of(2) {
            col
        } else {
            group_count - 1 - col
        };
        groups[group_idx].push(participant);
    }

    Ok(groups)
}

/// Build cross-seeded playoff participants from group standings.
///
/// For 4 groups with top-2 advancing (8 players total), playoff seeds:
///   1. A1, 2. B1, 3. C1, 4. D1, 5. A2, 6. B2, 7. C2, 8. D2
///
/// This ensures same-group participants don't meet until later rounds
/// in an SE bracket (seed 1 vs seed 8, seed 2 vs seed 7, etc.).
pub fn cross_seed_for_playoffs(
    group_standings: Vec<Vec<SeededParticipant>>,
    advance_per_group: usize,
) -> Vec<SeededParticipant> {
    let mut playoff_participants = Vec::new();

    // Iterate by finishing position across groups:
    // First all group winners (position 0), then all runners-up (position 1), etc.
    for position in 0..advance_per_group {
        for group in &group_standings {
            if let Some(participant) = group.get(position) {
                playoff_participants.push(participant.clone());
            }
        }
    }

    // Re-assign sequential seeds
    for (i, p) in playoff_participants.iter_mut().enumerate() {
        p.seed = (i + 1) as i32;
    }

    playoff_participants
}

/// Convert a group index (0-based) to a letter label (A, B, C, ..., Z, AA, AB, ...).
pub fn group_label(index: usize) -> String {
    if index < 26 {
        String::from((b'A' + index as u8) as char)
    } else {
        format!(
            "{}{}",
            group_label(index / 26 - 1),
            (b'A' + (index % 26) as u8) as char
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tournament::bracket_generator::tests::create_test_participants;

    #[test]
    fn test_distribute_8_into_4_groups() {
        let participants = create_test_participants(8);
        let groups = distribute_into_groups(participants, 4).unwrap();

        assert_eq!(groups.len(), 4);
        // Each group should have 2 participants
        for group in &groups {
            assert_eq!(group.len(), 2);
        }

        // Snake draft: 1→A, 2→B, 3→C, 4→D, 5→D, 6→C, 7→B, 8→A
        assert_eq!(groups[0][0].seed, 1); // A: seed 1
        assert_eq!(groups[0][1].seed, 8); // A: seed 8
        assert_eq!(groups[1][0].seed, 2); // B: seed 2
        assert_eq!(groups[1][1].seed, 7); // B: seed 7
        assert_eq!(groups[2][0].seed, 3); // C: seed 3
        assert_eq!(groups[2][1].seed, 6); // C: seed 6
        assert_eq!(groups[3][0].seed, 4); // D: seed 4
        assert_eq!(groups[3][1].seed, 5); // D: seed 5
    }

    #[test]
    fn test_distribute_16_into_4_groups() {
        let participants = create_test_participants(16);
        let groups = distribute_into_groups(participants, 4).unwrap();

        assert_eq!(groups.len(), 4);
        for group in &groups {
            assert_eq!(group.len(), 4);
        }

        // Row 0 (L→R): 1→A, 2→B, 3→C, 4→D
        // Row 1 (R→L): 5→D, 6→C, 7→B, 8→A
        // Row 2 (L→R): 9→A, 10→B, 11→C, 12→D
        // Row 3 (R→L): 13→D, 14→C, 15→B, 16→A
        assert_eq!(
            groups[0].iter().map(|p| p.seed).collect::<Vec<_>>(),
            vec![1, 8, 9, 16]
        );
        assert_eq!(
            groups[1].iter().map(|p| p.seed).collect::<Vec<_>>(),
            vec![2, 7, 10, 15]
        );
        assert_eq!(
            groups[2].iter().map(|p| p.seed).collect::<Vec<_>>(),
            vec![3, 6, 11, 14]
        );
        assert_eq!(
            groups[3].iter().map(|p| p.seed).collect::<Vec<_>>(),
            vec![4, 5, 12, 13]
        );
    }

    #[test]
    fn test_distribute_odd_count() {
        // 7 participants into 3 groups: 3+2+2
        let participants = create_test_participants(7);
        let groups = distribute_into_groups(participants, 3).unwrap();

        assert_eq!(groups.len(), 3);
        // Row 0: 1→A, 2→B, 3→C
        // Row 1: 4→C, 5→B, 6→A
        // Row 2: 7→A (only one left)
        assert_eq!(groups[0].len(), 3); // A: seeds 1, 6, 7
        assert_eq!(groups[1].len(), 2); // B: seeds 2, 5
        assert_eq!(groups[2].len(), 2); // C: seeds 3, 4
    }

    #[test]
    fn test_distribute_insufficient() {
        let participants = create_test_participants(1);
        let result = distribute_into_groups(participants, 2);
        assert!(matches!(result, Err(DomainError::InsufficientParticipants)));
    }

    #[test]
    fn test_distribute_more_groups_than_participants() {
        let participants = create_test_participants(3);
        let result = distribute_into_groups(participants, 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_cross_seed_4_groups_top2() {
        // Simulate 4 groups, each with standings (winner first)
        let groups: Vec<Vec<SeededParticipant>> = (0..4)
            .map(|g| {
                (0..3)
                    .map(|pos| SeededParticipant {
                        registration_id: portal_core::TournamentRegistrationId::new(),
                        seed: (g * 3 + pos + 1) as i32,
                        participant_name: format!("Group {} #{}", group_label(g), pos + 1),
                        participant_logo_url: None,
                    })
                    .collect()
            })
            .collect();

        let playoff = cross_seed_for_playoffs(groups, 2);

        // 4 groups × 2 advancing = 8 participants
        assert_eq!(playoff.len(), 8);

        // Order should be: A1, B1, C1, D1, A2, B2, C2, D2
        assert_eq!(playoff[0].participant_name, "Group A #1");
        assert_eq!(playoff[1].participant_name, "Group B #1");
        assert_eq!(playoff[2].participant_name, "Group C #1");
        assert_eq!(playoff[3].participant_name, "Group D #1");
        assert_eq!(playoff[4].participant_name, "Group A #2");
        assert_eq!(playoff[5].participant_name, "Group B #2");
        assert_eq!(playoff[6].participant_name, "Group C #2");
        assert_eq!(playoff[7].participant_name, "Group D #2");

        // Seeds should be sequential 1-8
        for (i, p) in playoff.iter().enumerate() {
            assert_eq!(p.seed, (i + 1) as i32);
        }
    }

    #[test]
    fn test_cross_seed_2_groups_top2() {
        let groups: Vec<Vec<SeededParticipant>> = (0..2)
            .map(|g| {
                (0..4)
                    .map(|pos| SeededParticipant {
                        registration_id: portal_core::TournamentRegistrationId::new(),
                        seed: (g * 4 + pos + 1) as i32,
                        participant_name: format!("Group {} #{}", group_label(g), pos + 1),
                        participant_logo_url: None,
                    })
                    .collect()
            })
            .collect();

        let playoff = cross_seed_for_playoffs(groups, 2);

        // 2 groups × 2 = 4 participants
        assert_eq!(playoff.len(), 4);
        assert_eq!(playoff[0].participant_name, "Group A #1");
        assert_eq!(playoff[1].participant_name, "Group B #1");
        assert_eq!(playoff[2].participant_name, "Group A #2");
        assert_eq!(playoff[3].participant_name, "Group B #2");
    }

    #[test]
    fn test_groups_config_from_format_settings() {
        let settings = serde_json::json!({
            "group_count": 4,
            "advance_per_group": 2,
            "group_format": "round_robin",
            "playoff_format": "single_elimination"
        });

        let config = GroupsConfig::from_format_settings(&settings, 16).unwrap();
        assert_eq!(config.group_count, 4);
        assert_eq!(config.advance_per_group, 2);
        assert_eq!(config.group_format, GroupStageFormat::RoundRobin);
        assert_eq!(config.playoff_format, PlayoffFormat::SingleElimination);
    }

    #[test]
    fn test_groups_config_defaults() {
        let settings = serde_json::json!({});
        let config = GroupsConfig::from_format_settings(&settings, 16).unwrap();

        assert_eq!(config.group_count, 4); // 16/4 = 4
        assert_eq!(config.advance_per_group, 2);
        assert_eq!(config.group_format, GroupStageFormat::RoundRobin);
        assert_eq!(config.playoff_format, PlayoffFormat::SingleElimination);
    }

    #[test]
    fn test_groups_config_swiss_and_de() {
        let settings = serde_json::json!({
            "group_count": 2,
            "advance_per_group": 3,
            "group_format": "swiss",
            "playoff_format": "double_elimination"
        });

        let config = GroupsConfig::from_format_settings(&settings, 12).unwrap();
        assert_eq!(config.group_format, GroupStageFormat::Swiss);
        assert_eq!(config.playoff_format, PlayoffFormat::DoubleElimination);
    }

    #[test]
    fn test_group_label() {
        assert_eq!(group_label(0), "A");
        assert_eq!(group_label(1), "B");
        assert_eq!(group_label(25), "Z");
        assert_eq!(group_label(26), "AA");
    }
}
