//! Forfeit request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

// =============================================================================
// FORFEIT REQUESTS
// =============================================================================

/// Request to withdraw from a tournament.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct WithdrawFromTournamentRequest {
    /// Optional reason for withdrawal.
    #[validate(length(max = 500))]
    #[serde(default)]
    pub reason: Option<String>,
}

/// Request to forfeit a match (admin).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminForfeitMatchRequest {
    /// Registration ID of the forfeiting team.
    pub forfeiting_registration_id: String,

    /// Type of forfeit.
    pub forfeit_type: String,

    /// Reason for the forfeit.
    #[validate(length(min = 5, max = 500))]
    pub reason: String,
}

/// Request to disqualify a registration (admin).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminDisqualifyRequest {
    /// Reason for disqualification.
    #[validate(length(min = 10, max = 1000))]
    pub reason: String,
}

/// Request to process a double forfeit (admin).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminDoubleForfeitRequest {
    /// Reason for the double forfeit.
    #[validate(length(min = 10, max = 1000))]
    pub reason: String,
}
