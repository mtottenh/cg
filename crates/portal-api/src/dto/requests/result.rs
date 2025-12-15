//! Result submission request DTOs.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

// =============================================================================
// RESULT CLAIM REQUESTS
// =============================================================================

/// Request to submit a match result claim.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SubmitResultClaimRequest {
    /// Registration ID of the claimed winner.
    pub claimed_winner_registration_id: String,

    /// Score for participant 1.
    #[validate(range(min = 0, max = 10))]
    pub participant1_score: i32,

    /// Score for participant 2.
    #[validate(range(min = 0, max = 10))]
    pub participant2_score: i32,

    /// Game-by-game results (for series matches).
    #[serde(default)]
    #[validate(length(max = 7))]
    pub game_results: Vec<GameResultInput>,

    /// Evidence IDs (screenshots, VODs, etc.).
    #[serde(default)]
    pub evidence_ids: Vec<String>,

    /// Demo match link IDs (from demo catalog).
    /// These reference demos already linked to this match via demo_match_links.
    #[serde(default)]
    pub demo_link_ids: Vec<String>,

    /// Optional notes from submitter.
    #[validate(length(max = 1000))]
    #[serde(default)]
    pub notes: Option<String>,
}

/// Input for a single game result in a series.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct GameResultInput {
    /// Game number (1-indexed).
    #[validate(range(min = 1, max = 7))]
    pub game_number: i32,

    /// Map ID played.
    #[validate(length(min = 1, max = 64))]
    pub map_id: String,

    /// Score for participant 1.
    #[validate(range(min = 0))]
    pub participant1_score: i32,

    /// Score for participant 2.
    #[validate(range(min = 0))]
    pub participant2_score: i32,

    /// Duration of the game in seconds.
    #[serde(default)]
    pub duration_seconds: Option<i64>,

    /// Evidence IDs specific to this game.
    #[serde(default)]
    pub evidence_ids: Vec<String>,

    /// Demo match link ID for this specific game (from demo catalog).
    #[serde(default)]
    pub demo_link_id: Option<String>,
}

/// Request to confirm a result claim.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfirmResultClaimRequest {
    // No additional fields - claim ID from path
}

/// Request to dispute a result claim.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct DisputeResultClaimRequest {
    /// Reason for the dispute.
    #[validate(length(min = 10, max = 1000))]
    pub reason: String,

    /// Evidence IDs supporting the dispute.
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

/// Request to cancel a result claim.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CancelResultClaimRequest {
    // No additional fields - claim ID from path
}

/// Request for admin to resolve a result dispute.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminResolveResultRequest {
    /// Resolution decision.
    #[validate(length(min = 10, max = 1000))]
    pub resolution: String,

    /// Final winner registration ID.
    pub winner_registration_id: String,

    /// Final score for participant 1.
    #[validate(range(min = 0, max = 10))]
    pub participant1_score: i32,

    /// Final score for participant 2.
    #[validate(range(min = 0, max = 10))]
    pub participant2_score: i32,

    /// Updated game results (if needed).
    #[serde(default)]
    pub game_results: Option<Vec<GameResultInput>>,
}

// =============================================================================
// QUERY PARAMETERS
// =============================================================================

/// Query parameters for listing result claims.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ListResultClaimsQuery {
    /// Filter by status.
    #[serde(default)]
    pub status: Option<String>,

    /// Include only the latest claim per match.
    #[serde(default)]
    pub latest_only: bool,
}
