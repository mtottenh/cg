//! Player game profile request DTOs.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::ToSchema;

/// Request body for submitting a player's in-game rating update.
///
/// Used by external services (e.g., steam bot) to update a player's
/// game-specific rating (e.g., CS2 Premier ELO).
#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitRatingRequest {
    /// The player's current in-game rating.
    #[schema(example = 15000)]
    pub rating: i32,

    /// Source of the rating update (e.g., "mm_demo", "manual", "bot_sync").
    #[schema(example = "mm_demo")]
    pub source: String,

    /// When the rating was observed in-game.
    #[schema(example = "2026-03-01T12:00:00Z")]
    pub recorded_at: DateTime<Utc>,
}
