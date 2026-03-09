//! Veto system request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

// =============================================================================
// VETO SESSION REQUESTS
// =============================================================================

/// Request to create a veto session for a match.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateVetoSessionRequest {
    /// Veto format ID to use (e.g., "bo1_veto", "bo3_veto").
    #[validate(length(min = 1, max = 64))]
    pub veto_format_id: String,

    /// Optional custom map pool. If not provided, uses tournament default.
    #[serde(default)]
    pub map_pool: Option<Vec<String>>,

    /// Timeout in seconds for each action (default: 30).
    #[validate(range(min = 10, max = 300))]
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u32,

    /// Side selection mode (picker_choice, coin_flip, knife).
    /// If not provided, defaults from tournament settings or plugin default.
    #[validate(length(max = 32))]
    pub side_selection_mode: Option<String>,
}

fn default_timeout_seconds() -> u32 {
    30
}

/// Request to start a veto session.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartVetoSessionRequest {
    /// Which registration should go first (coin flip winner chooses).
    pub first_action_registration_id: String,
}

/// Request to record a coin flip result.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordCoinFlipRequest {
    /// Registration ID of the coin flip winner.
    pub winner_registration_id: String,

    /// Whether the winner goes first (true) or defers to opponent (false).
    #[serde(default = "default_winner_goes_first")]
    pub winner_goes_first: bool,
}

fn default_winner_goes_first() -> bool {
    true
}

/// Request to perform a veto action (ban or pick).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct PerformVetoActionRequest {
    /// Map ID to ban or pick.
    #[validate(length(min = 1, max = 64))]
    pub map_id: String,
}

/// Request to select a side for a picked map.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SelectSideRequest {
    /// Action number (1-based) for which to select side.
    pub action_number: u32,

    /// Selected side (e.g., "ct", "t").
    #[validate(length(min = 1, max = 32))]
    pub side: String,
}

// =============================================================================
// QUERY PARAMETERS
// =============================================================================

/// Query parameters for getting veto session state.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct GetVetoStateQuery {
    /// Include action history in response.
    #[serde(default)]
    pub include_history: bool,
}
