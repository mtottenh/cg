//! Game request DTOs.

use serde::{Deserialize, Serialize};
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

/// Request to add a new map to a game's available maps catalog.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct AddMapRequest {
    /// Map identifier (alphanumeric + underscores).
    #[validate(length(min = 1, max = 64))]
    #[schema(example = "de_custom_map")]
    pub id: String,

    /// Display name.
    #[validate(length(min = 1, max = 128))]
    #[schema(example = "Custom Map")]
    pub display_name: String,

    /// Map image URL.
    #[validate(url)]
    pub image_url: Option<String>,

    /// Game modes this map supports.
    #[schema(example = json!(["competitive"]))]
    pub game_modes: Vec<String>,

    /// External identifier (e.g., Steam Workshop ID).
    #[validate(length(max = 128))]
    pub external_id: Option<String>,

    /// External URL (e.g., Steam Workshop URL).
    #[validate(url)]
    pub external_url: Option<String>,
}

/// Request to update an existing map's metadata.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UpdateMapRequest {
    /// Display name.
    #[validate(length(min = 1, max = 128))]
    pub display_name: Option<String>,

    /// Map image URL.
    #[validate(url)]
    pub image_url: Option<String>,

    /// Game modes this map supports.
    pub game_modes: Option<Vec<String>>,

    /// External identifier (e.g., Steam Workshop ID).
    #[validate(length(max = 128))]
    pub external_id: Option<String>,

    /// External URL (e.g., Steam Workshop URL).
    #[validate(url)]
    pub external_url: Option<String>,
}

/// Replace the full set of rank tiers for a game.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct SetRankTiersRequest {
    /// Rank tiers (1-20 items).
    #[validate(length(min = 1, max = 20))]
    pub rank_tiers: Vec<RankTierInput>,
}

/// A single rank tier definition.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct RankTierInput {
    /// Tier identifier.
    #[validate(length(min = 1, max = 32))]
    #[schema(example = "gold")]
    pub id: String,

    /// Display name.
    #[validate(length(min = 1, max = 64))]
    #[schema(example = "Gold")]
    pub display_name: String,

    /// Minimum rating for this tier.
    #[schema(example = 30000)]
    pub min_rating: i32,

    /// Maximum rating for this tier (None = no upper limit).
    pub max_rating: Option<i32>,

    /// Display color (hex).
    #[schema(example = "#FFD700")]
    pub color: Option<String>,

    /// Icon URL for the rank.
    pub icon_url: Option<String>,

    /// Display order (lower = shown first).
    #[schema(example = 7)]
    pub order: i32,
}

/// Update team size constraints.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UpdateTeamSizeRequest {
    /// Minimum team size (1-100).
    #[validate(range(min = 1, max = 100))]
    #[schema(example = 5)]
    pub min: Option<i32>,

    /// Maximum team size (1-100).
    #[validate(range(min = 1, max = 100))]
    #[schema(example = 5)]
    pub max: Option<i32>,

    /// Default team size (1-100).
    #[validate(range(min = 1, max = 100))]
    #[schema(example = 5)]
    pub default: Option<i32>,
}
