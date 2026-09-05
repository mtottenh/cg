//! Shared veto (map pick/ban) types.
//!
//! These types are shared between `portal-plugins` and `portal-domain`.
//! Plugin defines the format configuration; domain uses it in the veto session state machine.

use serde::{Deserialize, Serialize};

// =============================================================================
// VETO ACTION TYPE
// =============================================================================

/// Type of veto action.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum VetoActionType {
    /// Remove a map from the pool.
    #[default]
    Ban,
    /// Select a map to be played.
    Pick,
    /// Last remaining map (automatic selection).
    Decider,
    /// Server-side weighted random pick ("the wheel"). Recorded as an auto
    /// action (performed_by NULL); never auto-executed — a human triggers
    /// each spin.
    Random,
}

impl std::fmt::Display for VetoActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ban => write!(f, "ban"),
            Self::Pick => write!(f, "pick"),
            Self::Decider => write!(f, "decider"),
            Self::Random => write!(f, "random"),
        }
    }
}

impl std::str::FromStr for VetoActionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ban" => Ok(Self::Ban),
            "pick" => Ok(Self::Pick),
            "decider" => Ok(Self::Decider),
            "random" => Ok(Self::Random),
            _ => Err(format!("invalid veto action type: {s}")),
        }
    }
}

// =============================================================================
// SIDE SELECTION MODE
// =============================================================================

/// How starting sides are determined for picked maps.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SideSelectionMode {
    /// Picker chooses their starting side. Decider maps skip (knife).
    PickerChoice,
    /// Random side assignment after each pick.
    CoinFlip,
    /// No veto-level side selection — decided in-game (e.g., knife round).
    #[default]
    Knife,
}

impl std::fmt::Display for SideSelectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PickerChoice => write!(f, "picker_choice"),
            Self::CoinFlip => write!(f, "coin_flip"),
            Self::Knife => write!(f, "knife"),
        }
    }
}

impl std::str::FromStr for SideSelectionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "picker_choice" => Ok(Self::PickerChoice),
            "coin_flip" => Ok(Self::CoinFlip),
            "knife" => Ok(Self::Knife),
            _ => Err(format!("invalid side selection mode: {s}")),
        }
    }
}

impl SideSelectionMode {
    /// Display name for this mode.
    pub fn display_name(&self) -> &str {
        match self {
            Self::PickerChoice => "Picker Chooses Side",
            Self::CoinFlip => "Coin Flip for Sides",
            Self::Knife => "Knife Round (In-Game)",
        }
    }
}

// =============================================================================
// VETO FORMAT CONFIG
// =============================================================================

/// Map veto format configuration.
///
/// Defines the sequence of actions (bans, picks, decider) for a veto session.
/// Shared between plugin (which defines formats) and domain (which runs the state machine).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoFormatConfig {
    /// Unique identifier for this format (e.g., "bo3_veto").
    pub id: String,
    /// Display name (e.g., "Best of 3 Veto").
    pub display_name: String,
    /// Description of the format.
    pub description: String,
    /// Sequence of veto actions.
    pub sequence: Vec<VetoFormatActionConfig>,
    /// Minimum maps required in the pool.
    pub min_map_pool: usize,
}

impl VetoFormatConfig {
    /// Create a standard Bo1 veto format (6 bans, 1 decider).
    #[must_use]
    pub fn bo1() -> Self {
        Self {
            id: "bo1_standard".to_string(),
            display_name: "Best of 1".to_string(),
            description: "6 bans alternating, 1 decider".to_string(),
            sequence: vec![
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 0,
                    action_type: VetoActionType::Decider,
                },
            ],
            min_map_pool: 7,
        }
    }

    /// Create a standard Bo3 veto format (Ban-Ban-Pick-Pick-Ban-Ban-Decider).
    #[must_use]
    pub fn bo3() -> Self {
        Self {
            id: "bo3_standard".to_string(),
            display_name: "Best of 3".to_string(),
            description: "Ban-Ban-Pick-Pick-Ban-Ban-Decider".to_string(),
            sequence: vec![
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Pick,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Pick,
                },
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 0,
                    action_type: VetoActionType::Decider,
                },
            ],
            min_map_pool: 7,
        }
    }

    /// Create a standard Bo5 veto format (Ban-Ban-Pick-Pick-Pick-Pick-Decider).
    #[must_use]
    pub fn bo5() -> Self {
        Self {
            id: "bo5_standard".to_string(),
            display_name: "Best of 5".to_string(),
            description: "Ban-Ban-Pick-Pick-Pick-Pick-Decider".to_string(),
            sequence: vec![
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Ban,
                },
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Pick,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Pick,
                },
                VetoFormatActionConfig {
                    team: 1,
                    action_type: VetoActionType::Pick,
                },
                VetoFormatActionConfig {
                    team: 2,
                    action_type: VetoActionType::Pick,
                },
                VetoFormatActionConfig {
                    team: 0,
                    action_type: VetoActionType::Decider,
                },
            ],
            min_map_pool: 7,
        }
    }

    /// Create a standard Bo7 veto format (six alternating picks, decider).
    ///
    /// With the standard seven-map pool every map is in play, so there is
    /// nothing to ban — the veto only decides play order.
    #[must_use]
    pub fn bo7() -> Self {
        let mut sequence: Vec<VetoFormatActionConfig> = (0..6)
            .map(|i| VetoFormatActionConfig {
                team: if i % 2 == 0 { 1 } else { 2 },
                action_type: VetoActionType::Pick,
            })
            .collect();
        sequence.push(VetoFormatActionConfig {
            team: 0,
            action_type: VetoActionType::Decider,
        });
        Self {
            id: "bo7_standard".to_string(),
            display_name: "Best of 7".to_string(),
            description: "Six alternating picks, decider — every map is in play".to_string(),
            sequence,
            min_map_pool: 7,
        }
    }

    /// Create a wheel format: `map_count` weighted-random picks, no bans.
    ///
    /// Used by PUGs. The pool is the players' deduped nominations; weights
    /// live in the PUG layer. Each `random` action is performed by a human
    /// hitting "spin", with the server choosing the winner.
    #[must_use]
    pub fn wheel(map_count: usize) -> Self {
        Self {
            id: format!("wheel_bo{map_count}"),
            display_name: format!("Wheel — Best of {map_count}"),
            description: format!(
                "{map_count} map{} chosen by spinning the wheel",
                if map_count == 1 { "" } else { "s" }
            ),
            sequence: (0..map_count)
                .map(|_| VetoFormatActionConfig {
                    team: 0,
                    action_type: VetoActionType::Random,
                })
                .collect(),
            min_map_pool: map_count,
        }
    }

    /// Wheel format for a Bo1 (one spin).
    #[must_use]
    pub fn wheel_bo1() -> Self {
        Self::wheel(1)
    }

    /// Wheel format for a Bo3 (three spins — one per map that may be played).
    #[must_use]
    pub fn wheel_bo3() -> Self {
        Self::wheel(3)
    }

    /// Wheel format for a Bo5 (five spins — one per map that may be played).
    #[must_use]
    pub fn wheel_bo5() -> Self {
        Self::wheel(5)
    }

    /// THE built-in format table — the single fallback both the veto
    /// service and the HTTP handler consult (review m1: they used to carry
    /// two hand-maintained tables that had already drifted — one knew bo7,
    /// the other knew the wheel formats).
    #[must_use]
    pub fn builtin(format_id: &str) -> Option<Self> {
        match format_id {
            "bo1_veto" | "bo1_standard" => Some(Self::bo1()),
            "bo3_veto" | "bo3_standard" => Some(Self::bo3()),
            "bo5_veto" | "bo5_standard" => Some(Self::bo5()),
            "bo7_veto" | "bo7_standard" => Some(Self::bo7()),
            "wheel_bo1" => Some(Self::wheel_bo1()),
            "wheel_bo3" => Some(Self::wheel_bo3()),
            "wheel_bo5" => Some(Self::wheel_bo5()),
            _ => None,
        }
    }

    /// Whether this format contains any team-performed actions.
    /// Formats with none (e.g. wheel formats) skip the coin-flip stage.
    #[must_use]
    pub fn has_team_actions(&self) -> bool {
        self.sequence.iter().any(|a| a.team != 0)
    }

    /// Get the action at a given index (0-indexed).
    #[must_use]
    pub fn get_action(&self, index: usize) -> Option<&VetoFormatActionConfig> {
        self.sequence.get(index)
    }

    /// Get the total number of actions in this format.
    #[must_use]
    pub fn action_count(&self) -> usize {
        self.sequence.len()
    }

    /// Whether the veto is complete once `action_number` is the next action to
    /// perform. Action numbers are 1-based, so a seven-action format is
    /// complete when the next action would be number 8 — not 7, which is the
    /// trailing decider itself. (`>=` here ended every standard format one
    /// action early and left the decider map `available`.)
    #[must_use]
    pub fn is_complete_at(&self, action_number: usize) -> bool {
        action_number > self.sequence.len()
    }

    /// Count picks in this format (maps that will be played).
    #[must_use]
    pub fn pick_count(&self) -> usize {
        self.sequence
            .iter()
            .filter(|a| matches!(a.action_type, VetoActionType::Pick))
            .count()
    }

    /// Count deciders in this format.
    #[must_use]
    pub fn decider_count(&self) -> usize {
        self.sequence
            .iter()
            .filter(|a| matches!(a.action_type, VetoActionType::Decider))
            .count()
    }

    /// Count random (wheel) selections in this format.
    #[must_use]
    pub fn random_count(&self) -> usize {
        self.sequence
            .iter()
            .filter(|a| matches!(a.action_type, VetoActionType::Random))
            .count()
    }

    /// Get total maps that will be selected (picks + deciders + wheel spins).
    #[must_use]
    pub fn maps_selected(&self) -> usize {
        self.pick_count() + self.decider_count() + self.random_count()
    }
}

/// A single action in the veto format sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoFormatActionConfig {
    /// Which team performs this action.
    /// - 0 = automatic (decider)
    /// - 1 = team with first action
    /// - 2 = team with second action
    pub team: u8,

    /// Action type.
    pub action_type: VetoActionType,
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn standard_formats_complete_only_after_their_last_action() {
        for format in [
            VetoFormatConfig::bo1(),
            VetoFormatConfig::bo3(),
            VetoFormatConfig::bo5(),
        ] {
            let n = format.action_count();
            assert!(
                matches!(
                    format.get_action(n - 1).map(|a| a.action_type),
                    Some(VetoActionType::Decider)
                ),
                "{} should end in a decider",
                format.id
            );
            // After action n-1 the next action is n (the decider): not complete.
            assert!(
                !format.is_complete_at(n),
                "{}: decider must still run",
                format.id
            );
            // After the decider the next action would be n+1: complete.
            assert!(
                format.is_complete_at(n + 1),
                "{}: complete after decider",
                format.id
            );
        }
    }
}
