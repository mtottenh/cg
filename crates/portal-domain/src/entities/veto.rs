//! Veto (map pick/ban) domain entities.
//!
//! The veto system handles map selection for tournament matches through a
//! turn-based process where teams alternate banning and picking maps.

use chrono::{DateTime, Utc};
use portal_core::{
    TournamentMatchId, TournamentRegistrationId, UserId, VetoActionId, VetoSessionId,
};
// Re-export shared veto types from portal-core so downstream crates
// can continue to import from `portal_domain::entities::veto`.
pub use portal_core::{
    SideSelectionMode, VetoActionType, VetoFormatActionConfig, VetoFormatConfig,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// VETO SESSION
// =============================================================================

/// A map veto session for a tournament match.
///
/// Tracks the state of a map pick/ban process including:
/// - Which maps are in the pool and remaining
/// - Whose turn it is to act
/// - Selected maps in play order
/// - Timeout handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoSession {
    pub id: VetoSessionId,
    pub match_id: TournamentMatchId,

    /// Veto format identifier (from plugin, e.g., "bo1_veto", "bo3_veto")
    pub veto_format_id: String,

    /// Starting map pool
    pub map_pool: Vec<String>,

    /// Who won the coin flip (gets to choose first action or side)
    pub coin_flip_winner_registration_id: Option<TournamentRegistrationId>,

    /// Who has first action (may differ from coin flip winner if they deferred)
    pub first_action_registration_id: Option<TournamentRegistrationId>,

    /// Current action number (0 = not started, 1+ = in progress)
    pub current_action_number: u32,

    /// Whose turn it is to act
    pub current_team_turn: Option<TournamentRegistrationId>,

    /// Maps remaining in pool (updated as veto progresses)
    pub remaining_maps: Vec<String>,

    /// Maps selected for play (in order)
    pub selected_maps: Vec<String>,

    /// Current status
    pub status: VetoStatus,

    /// Deadline for current action
    pub action_deadline: Option<DateTime<Utc>>,

    /// Timeout per action (seconds)
    pub timeout_seconds: u32,

    /// How starting sides are determined for picked maps
    pub side_selection_mode: SideSelectionMode,

    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl VetoSession {
    /// Check if the session is waiting to start.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.status, VetoStatus::Pending)
    }

    /// Check if the session is in coin flip phase.
    #[must_use]
    pub const fn is_coin_flip(&self) -> bool {
        matches!(self.status, VetoStatus::CoinFlip)
    }

    /// Check if the session is in progress.
    #[must_use]
    pub const fn is_in_progress(&self) -> bool {
        matches!(self.status, VetoStatus::InProgress)
    }

    /// Check if the session is complete.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.status, VetoStatus::Completed)
    }

    /// Check if the session is cancelled.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        matches!(self.status, VetoStatus::Cancelled)
    }

    /// Check if the session is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self.status, VetoStatus::Completed | VetoStatus::Cancelled)
    }

    /// Check if the current action deadline has passed.
    #[must_use]
    pub fn is_timed_out(&self) -> bool {
        if let Some(deadline) = self.action_deadline {
            Utc::now() > deadline
        } else {
            false
        }
    }

    /// Check if a map is still available for selection.
    #[must_use]
    pub fn is_map_available(&self, map_id: &str) -> bool {
        self.remaining_maps.iter().any(|m| m == map_id)
    }

    /// Get the number of maps that have been selected.
    #[must_use]
    pub fn selected_count(&self) -> usize {
        self.selected_maps.len()
    }

    /// Get the number of maps remaining in the pool.
    #[must_use]
    pub fn remaining_count(&self) -> usize {
        self.remaining_maps.len()
    }
}

/// Status of a veto session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VetoStatus {
    /// Session created, waiting to start
    #[default]
    Pending,
    /// Coin flip in progress to determine first action
    CoinFlip,
    /// Veto actions in progress
    InProgress,
    /// All maps selected, veto complete
    Completed,
    /// Veto cancelled (match cancelled, etc.)
    Cancelled,
}

impl VetoStatus {
    /// Check if the status allows starting the veto.
    #[must_use]
    pub const fn can_start(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Check if the status allows performing actions.
    #[must_use]
    pub const fn can_act(&self) -> bool {
        matches!(self, Self::InProgress)
    }

    /// Check if the status allows recording coin flip.
    #[must_use]
    pub const fn can_coin_flip(&self) -> bool {
        matches!(self, Self::CoinFlip)
    }
}

impl std::fmt::Display for VetoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::CoinFlip => write!(f, "coin_flip"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for VetoStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "coin_flip" => Ok(Self::CoinFlip),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid veto status: {s}")),
        }
    }
}

// SideSelectionMode is defined in portal-core and re-exported above.

// =============================================================================
// VETO ACTION
// =============================================================================

/// A single action in a veto session.
///
/// Records a ban, pick, or decider selection with metadata about
/// who performed it and whether it was automatic (timeout).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoAction {
    pub id: VetoActionId,
    pub session_id: VetoSessionId,

    /// Action sequence number (1-indexed)
    pub action_number: u32,

    /// Type of action
    pub action_type: VetoActionType,

    /// Map selected/banned
    pub map_id: String,

    /// Who performed the action
    pub performed_by_registration_id: Option<TournamentRegistrationId>,
    pub performed_by_user_id: Option<UserId>,

    /// Side selection (for picks, e.g., "ct", "t")
    pub side_selection: Option<String>,
    pub side_selected_by_registration_id: Option<TournamentRegistrationId>,
    pub side_selected_at: Option<DateTime<Utc>>,

    /// Was this an auto-action due to timeout?
    pub was_auto_action: bool,
    pub auto_action_reason: Option<String>,

    pub performed_at: DateTime<Utc>,
}

impl VetoAction {
    /// Check if this is a ban action.
    #[must_use]
    pub const fn is_ban(&self) -> bool {
        matches!(self.action_type, VetoActionType::Ban)
    }

    /// Check if this is a pick action.
    #[must_use]
    pub const fn is_pick(&self) -> bool {
        matches!(self.action_type, VetoActionType::Pick)
    }

    /// Check if this is a decider action.
    #[must_use]
    pub const fn is_decider(&self) -> bool {
        matches!(self.action_type, VetoActionType::Decider)
    }

    /// Check if side selection is pending for this action.
    #[must_use]
    pub const fn needs_side_selection(&self) -> bool {
        matches!(self.action_type, VetoActionType::Pick) && self.side_selection.is_none()
    }

    /// Check if side has been selected.
    #[must_use]
    pub const fn has_side_selection(&self) -> bool {
        self.side_selection.is_some()
    }
}

// VetoActionType is defined in portal-core and re-exported above.

// =============================================================================
// VETO FORMAT (from portal-core)
// =============================================================================

/// Type alias for the canonical veto format config from portal-core.
pub type VetoFormat = VetoFormatConfig;

/// Type alias for the canonical veto format action config from portal-core.
pub type VetoFormatAction = VetoFormatActionConfig;

// =============================================================================
// HELPER TYPES
// =============================================================================

/// Result of performing a veto action.
#[derive(Debug, Clone)]
pub struct VetoActionResult {
    pub session: VetoSession,
    pub action: VetoAction,
    pub veto_complete: bool,
    pub next_team: Option<TournamentRegistrationId>,
    pub next_action_type: Option<VetoActionType>,
}

/// Current state of a veto session for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoSessionState {
    pub session: VetoSession,
    pub actions: Vec<VetoAction>,
    pub format: VetoFormat,
    pub current_action: Option<VetoFormatAction>,
    pub maps_with_status: Vec<MapStatus>,
}

/// Status of a map in the veto process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapStatus {
    pub map_id: String,
    pub map_name: String,
    pub image_url: Option<String>,
    pub status: MapVetoStatus,
    pub banned_by: Option<TournamentRegistrationId>,
    pub picked_by: Option<TournamentRegistrationId>,
    pub game_number: Option<u32>,
}

/// Veto status for a single map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapVetoStatus {
    Available,
    Banned,
    Picked,
    Decider,
}

impl std::fmt::Display for MapVetoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Available => write!(f, "available"),
            Self::Banned => write!(f, "banned"),
            Self::Picked => write!(f, "picked"),
            Self::Decider => write!(f, "decider"),
        }
    }
}

// =============================================================================
// COMMAND TYPES
// =============================================================================

/// Command to create a new veto session.
#[derive(Debug, Clone)]
pub struct CreateVetoSessionCommand {
    pub match_id: TournamentMatchId,
    pub veto_format_id: String,
    pub map_pool: Vec<String>,
    pub timeout_seconds: Option<u32>,
}

/// Command to record a coin flip result.
#[derive(Debug, Clone)]
pub struct RecordCoinFlipCommand {
    pub session_id: VetoSessionId,
    pub winner_registration_id: TournamentRegistrationId,
}

/// Command to perform a veto action.
#[derive(Debug, Clone)]
pub struct PerformVetoActionCommand {
    pub session_id: VetoSessionId,
    pub map_id: String,
    pub performed_by_user_id: UserId,
}

/// Command to select a side after picking a map.
#[derive(Debug, Clone)]
pub struct SelectSideCommand {
    pub session_id: VetoSessionId,
    pub action_number: u32,
    pub side: String,
    pub selected_by_user_id: UserId,
}
