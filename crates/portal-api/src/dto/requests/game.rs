//! Game request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request to update a game's settings.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UpdateGameRequest {
    /// New display name.
    #[validate(length(min = 1, max = 64))]
    #[schema(example = "Counter-Strike 2")]
    pub display_name: Option<String>,

    /// New short name.
    #[validate(length(max = 16))]
    #[schema(example = "CS2")]
    pub short_name: Option<String>,

    /// New description.
    #[validate(length(max = 1000))]
    #[schema(example = "Valve's tactical FPS")]
    pub description: Option<String>,

    /// New icon URL.
    #[validate(url)]
    #[schema(example = "https://example.com/cs2-icon.png")]
    pub icon_url: Option<String>,

    /// Whether the game is featured on the homepage.
    pub is_featured: Option<bool>,

    /// Sort order for display (lower = higher priority).
    pub sort_order: Option<i32>,
}

/// Request to set a game's custom map pool.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct SetMapPoolRequest {
    /// List of map IDs to include in the pool.
    /// Must contain at least 1 and at most 20 maps.
    #[validate(length(min = 1, max = 20))]
    #[schema(example = json!(["de_dust2", "de_mirage", "de_inferno"]))]
    pub map_ids: Vec<String>,
}
