//! Dispute request DTOs.

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

// =============================================================================
// DISPUTE REQUESTS
// =============================================================================

/// Request to raise a dispute against a match result.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RaiseDisputeRequest {
    /// Registration ID of the team raising the dispute.
    pub registration_id: String,

    /// Reason for the dispute.
    pub reason: String,

    /// Detailed description of the dispute.
    #[validate(length(min = 20, max = 2000))]
    pub description: String,

    /// Evidence IDs supporting the dispute.
    #[serde(default)]
    pub evidence_ids: Vec<String>,

    /// Optional result claim ID being disputed.
    #[serde(default)]
    pub result_claim_id: Option<String>,
}

/// Request to add a message to a dispute.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AddDisputeMessageRequest {
    /// Message content.
    #[validate(length(min = 1, max = 2000))]
    pub message: String,

    /// Evidence IDs attached to the message.
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

/// Request to add an admin message to a dispute (can be internal).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminDisputeMessageRequest {
    /// Message content.
    #[validate(length(min = 1, max = 2000))]
    pub message: String,

    /// Evidence IDs attached to the message.
    #[serde(default)]
    pub evidence_ids: Vec<String>,

    /// Whether this is an internal admin note (not visible to participants).
    #[serde(default)]
    pub is_internal: bool,
}

/// Request to assign a dispute for review.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AssignDisputeRequest {
    // No additional fields - dispute ID from path
}

/// Request to resolve a dispute by upholding the original result.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResolveUpholdRequest {
    /// Notes explaining the decision.
    #[validate(length(min = 10, max = 2000))]
    pub notes: String,
}

/// Request to resolve a dispute by overturning the result.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResolveOverturnRequest {
    /// Notes explaining the decision.
    #[validate(length(min = 10, max = 2000))]
    pub notes: String,

    /// New winner registration ID.
    pub new_winner_registration_id: String,

    /// New score for participant 1.
    #[validate(range(min = 0, max = 10))]
    pub new_participant1_score: i32,

    /// New score for participant 2.
    #[validate(range(min = 0, max = 10))]
    pub new_participant2_score: i32,
}

/// Request to resolve a dispute by ordering a rematch.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResolveRematchRequest {
    /// Notes explaining the decision.
    #[validate(length(min = 10, max = 2000))]
    pub notes: String,
}

/// Request to resolve a dispute by adjusting scores.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResolveAdjustedRequest {
    /// Notes explaining the decision.
    #[validate(length(min = 10, max = 2000))]
    pub notes: String,

    /// Adjusted score for participant 1.
    #[validate(range(min = 0, max = 10))]
    pub new_participant1_score: i32,

    /// Adjusted score for participant 2.
    #[validate(range(min = 0, max = 10))]
    pub new_participant2_score: i32,

    /// Optionally change the winner.
    #[serde(default)]
    pub new_winner_registration_id: Option<String>,
}

/// Request to resolve a dispute by disqualifying both teams.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResolveDoubleDqRequest {
    /// Notes explaining the decision.
    #[validate(length(min = 10, max = 2000))]
    pub notes: String,
}

// =============================================================================
// QUERY PARAMETERS
// =============================================================================

/// Query parameters for listing disputes.
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListDisputesQuery {
    /// Filter by status.
    #[serde(default)]
    pub status: Option<String>,

    /// Filter by priority.
    #[serde(default)]
    pub priority: Option<String>,

    /// Filter by tournament ID.
    #[serde(default)]
    pub tournament_id: Option<String>,

    /// Filter by match ID.
    #[serde(default)]
    pub match_id: Option<String>,

    /// Page number (1-indexed).
    #[serde(default = "default_page")]
    pub page: u32,

    /// Page size.
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}
