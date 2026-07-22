//! Shared veto (map pick/ban) types.
//!
//! These types are shared between `portal-plugins` and `portal-domain`.
//! Plugin defines the format configuration; domain uses it in the veto session state machine.

use serde::{Deserialize, Serialize};

// =============================================================================
// VETO ACTION TYPE
// =============================================================================

/// Type of veto action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VetoActionType {
    /// Remove a map from the pool.
    #[default]
    Ban,
    /// Select a map to be played.
    Pick,
    /// Last remaining map (automatic selection).
    Decider,
}

impl std::fmt::Display for VetoActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ban => write!(f, "ban"),
            Self::Pick => write!(f, "pick"),
            Self::Decider => write!(f, "decider"),
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
            _ => Err(format!("invalid veto action type: {s}")),
        }
    }
}

// =============================================================================
// SIDE SELECTION MODE
// =============================================================================

/// How starting sides are determined for picked maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
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

    /// Check if an action number would complete the veto.
    #[must_use]
    pub fn is_complete_at(&self, action_number: usize) -> bool {
        action_number >= self.sequence.len()
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

    /// Get total maps that will be selected (picks + deciders).
    #[must_use]
    pub fn maps_selected(&self) -> usize {
        self.pick_count() + self.decider_count()
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
