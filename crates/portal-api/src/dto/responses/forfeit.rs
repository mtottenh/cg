//! Forfeit response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::forfeit::{ForfeitRecord, ForfeitResult};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// FORFEIT RESPONSES
// =============================================================================

/// Response DTO for a forfeit record.
#[derive(Debug, Serialize, ToSchema)]
pub struct ForfeitRecordResponse {
    /// Forfeit record ID.
    pub id: String,
    /// Match ID.
    pub match_id: String,
    /// Registration ID of the forfeiting team.
    pub forfeiting_registration_id: String,
    /// Type of forfeit.
    pub forfeit_type: String,
    /// Reason for the forfeit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// User ID who triggered the forfeit (if user-triggered).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered_by_user_id: Option<String>,
    /// Whether the forfeit was system-triggered.
    pub triggered_by_system: bool,
    /// When the forfeit occurred.
    pub forfeited_at: DateTime<Utc>,
}

impl From<ForfeitRecord> for ForfeitRecordResponse {
    fn from(r: ForfeitRecord) -> Self {
        Self {
            id: r.id.to_string(),
            match_id: r.match_id.to_string(),
            forfeiting_registration_id: r.forfeiting_registration_id.to_string(),
            forfeit_type: r.forfeit_type.to_string(),
            reason: r.reason,
            triggered_by_user_id: r.triggered_by_user_id.map(|id| id.to_string()),
            triggered_by_system: r.triggered_by_system,
            forfeited_at: r.forfeited_at,
        }
    }
}

/// Response after processing a forfeit.
#[derive(Debug, Serialize, ToSchema)]
pub struct ForfeitResponse {
    /// The forfeit record.
    pub forfeit: ForfeitRecordResponse,
    /// ID of the winner who received the walkover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner_registration_id: Option<String>,
    /// Whether bracket progression was triggered.
    pub progression_triggered: bool,
}

impl From<ForfeitResult> for ForfeitResponse {
    fn from(r: ForfeitResult) -> Self {
        Self {
            forfeit: ForfeitRecordResponse::from(r.forfeit_record),
            winner_registration_id: r.winner_registration_id.map(|id| id.to_string()),
            progression_triggered: r.progression_triggered,
        }
    }
}

/// Response after withdrawing from a tournament.
#[derive(Debug, Serialize, ToSchema)]
pub struct WithdrawalResponse {
    /// Registration ID that was withdrawn.
    pub registration_id: String,
    /// Number of matches forfeited.
    pub matches_forfeited: usize,
    /// Individual forfeit results.
    pub forfeits: Vec<ForfeitResponse>,
}

/// Response after disqualification.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisqualificationResponse {
    /// Registration ID that was disqualified.
    pub registration_id: String,
    /// Reason for disqualification.
    pub reason: String,
    /// Number of matches forfeited.
    pub matches_forfeited: usize,
    /// Individual forfeit results.
    pub forfeits: Vec<ForfeitResponse>,
}
