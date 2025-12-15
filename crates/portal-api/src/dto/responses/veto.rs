//! Veto system response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::veto::{MapStatus, VetoAction, VetoSession, VetoSessionState};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// VETO SESSION RESPONSES
// =============================================================================

/// Response DTO for a veto session.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VetoSessionResponse {
    /// Session ID.
    pub id: String,
    /// Match ID.
    pub match_id: String,
    /// Veto format being used.
    pub veto_format_id: String,
    /// Full map pool.
    pub map_pool: Vec<String>,
    /// Remaining maps available.
    pub remaining_maps: Vec<String>,
    /// Maps selected for play (in order).
    pub selected_maps: Vec<String>,
    /// Current session status.
    pub status: String,
    /// Current action number (0-indexed).
    pub current_action_number: u32,
    /// Registration ID of coin flip winner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coin_flip_winner_registration_id: Option<String>,
    /// Registration ID of the participant who acts first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_action_registration_id: Option<String>,
    /// Registration ID of whose turn it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_team_turn: Option<String>,
    /// Deadline for current action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_deadline: Option<DateTime<Utc>>,
    /// Timeout seconds per action.
    pub timeout_seconds: u32,
    /// When session started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When session completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<VetoSession> for VetoSessionResponse {
    fn from(s: VetoSession) -> Self {
        Self {
            id: s.id.to_string(),
            match_id: s.match_id.to_string(),
            veto_format_id: s.veto_format_id,
            map_pool: s.map_pool,
            remaining_maps: s.remaining_maps,
            selected_maps: s.selected_maps,
            status: s.status.to_string(),
            current_action_number: s.current_action_number,
            coin_flip_winner_registration_id: s
                .coin_flip_winner_registration_id
                .map(|id| id.to_string()),
            first_action_registration_id: s.first_action_registration_id.map(|id| id.to_string()),
            current_team_turn: s.current_team_turn.map(|id| id.to_string()),
            action_deadline: s.action_deadline,
            timeout_seconds: s.timeout_seconds,
            started_at: s.started_at,
            completed_at: s.completed_at,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

// =============================================================================
// VETO ACTION RESPONSES
// =============================================================================

/// Response DTO for a veto action.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VetoActionResponse {
    /// Action ID.
    pub id: String,
    /// Session ID.
    pub session_id: String,
    /// Action number (0-indexed).
    pub action_number: u32,
    /// Action type (ban, pick, decider).
    pub action_type: String,
    /// Map ID affected.
    pub map_id: String,
    /// Registration ID of who performed the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performed_by_registration_id: Option<String>,
    /// User ID of who performed the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performed_by_user_id: Option<String>,
    /// Selected side for this map (if pick action).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_selection: Option<String>,
    /// Registration ID of who selected the side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_selected_by_registration_id: Option<String>,
    /// When side was selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_selected_at: Option<DateTime<Utc>>,
    /// Whether this was an automatic action (timeout).
    pub was_auto_action: bool,
    /// Reason for auto-action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_action_reason: Option<String>,
    /// When the action was performed.
    pub performed_at: DateTime<Utc>,
}

impl From<VetoAction> for VetoActionResponse {
    fn from(a: VetoAction) -> Self {
        Self {
            id: a.id.to_string(),
            session_id: a.session_id.to_string(),
            action_number: a.action_number,
            action_type: a.action_type.to_string(),
            map_id: a.map_id,
            performed_by_registration_id: a.performed_by_registration_id.map(|id| id.to_string()),
            performed_by_user_id: a.performed_by_user_id.map(|id| id.to_string()),
            side_selection: a.side_selection,
            side_selected_by_registration_id: a
                .side_selected_by_registration_id
                .map(|id| id.to_string()),
            side_selected_at: a.side_selected_at,
            was_auto_action: a.was_auto_action,
            auto_action_reason: a.auto_action_reason,
            performed_at: a.performed_at,
        }
    }
}

// =============================================================================
// VETO STATE RESPONSES
// =============================================================================

/// Complete veto session state response.
#[derive(Debug, Serialize, ToSchema)]
pub struct VetoSessionStateResponse {
    /// The veto session.
    pub session: VetoSessionResponse,
    /// Action history (ordered by action_number).
    pub actions: Vec<VetoActionResponse>,
    /// Maps with their current status.
    pub maps: Vec<MapStatusResponse>,
    /// Current action in the veto sequence (if in progress).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_action: Option<VetoFormatActionResponse>,
    /// Veto format details.
    pub format: VetoFormatResponse,
}

impl From<VetoSessionState> for VetoSessionStateResponse {
    fn from(state: VetoSessionState) -> Self {
        // Compute maps_selected before consuming the format
        let maps_selected = state.format.maps_selected();

        Self {
            session: state.session.into(),
            actions: state.actions.into_iter().map(Into::into).collect(),
            maps: state.maps_with_status.into_iter().map(Into::into).collect(),
            current_action: state.current_action.map(|a| VetoFormatActionResponse {
                team: a.team,
                action_type: a.action_type.to_string(),
            }),
            format: VetoFormatResponse {
                id: state.format.id,
                display_name: state.format.display_name,
                description: state.format.description,
                min_map_pool: state.format.min_map_pool,
                maps_selected,
                sequence: state
                    .format
                    .sequence
                    .into_iter()
                    .map(|a| VetoFormatActionResponse {
                        team: a.team,
                        action_type: a.action_type.to_string(),
                    })
                    .collect(),
            },
        }
    }
}

/// Map status within a veto session.
#[derive(Debug, Serialize, ToSchema)]
pub struct MapStatusResponse {
    /// Map ID.
    pub map_id: String,
    /// Map display name.
    pub map_name: String,
    /// Map image URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Current status (available, banned, picked, decider).
    pub status: String,
    /// Registration ID of who banned this map (if banned).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banned_by_registration_id: Option<String>,
    /// Registration ID of who picked this map (if picked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picked_by_registration_id: Option<String>,
    /// Game number this map will be played (if picked/decider).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_number: Option<u32>,
}

impl From<MapStatus> for MapStatusResponse {
    fn from(m: MapStatus) -> Self {
        Self {
            map_id: m.map_id,
            map_name: m.map_name,
            image_url: m.image_url,
            status: m.status.to_string(),
            banned_by_registration_id: m.banned_by.map(|id| id.to_string()),
            picked_by_registration_id: m.picked_by.map(|id| id.to_string()),
            game_number: m.game_number,
        }
    }
}

// =============================================================================
// VETO ACTION RESULT RESPONSE
// =============================================================================

/// Response after performing a veto action.
#[derive(Debug, Serialize, ToSchema)]
pub struct VetoActionResultResponse {
    /// The action that was performed.
    pub action: VetoActionResponse,
    /// Updated session state.
    pub session: VetoSessionResponse,
    /// Whether the veto process is now complete.
    pub is_complete: bool,
    /// Selected maps (if complete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_maps: Option<Vec<String>>,
}

// =============================================================================
// VETO FORMAT RESPONSES
// =============================================================================

/// Response DTO for available veto formats.
#[derive(Debug, Serialize, ToSchema)]
pub struct VetoFormatResponse {
    /// Format ID (e.g., "bo1_veto").
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Description.
    pub description: String,
    /// Minimum maps required in pool.
    pub min_map_pool: usize,
    /// Number of maps selected.
    pub maps_selected: usize,
    /// Action sequence.
    pub sequence: Vec<VetoFormatActionResponse>,
}

/// Single action in veto format sequence.
#[derive(Debug, Serialize, ToSchema)]
pub struct VetoFormatActionResponse {
    /// Team number (0 = auto, 1 = first team, 2 = second team).
    pub team: u8,
    /// Action type (ban, pick, decider).
    pub action_type: String,
}
