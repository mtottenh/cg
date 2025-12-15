//! Progression request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request to reapply progression with a different winner.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ReapplyProgressionRequest {
    /// The new winner registration ID
    pub new_winner_registration_id: String,
}

/// Request to process match progression with explicit winner/loser.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ProcessProgressionRequest {
    /// The winner registration ID
    pub winner_registration_id: String,
    /// The loser registration ID
    pub loser_registration_id: String,
}
