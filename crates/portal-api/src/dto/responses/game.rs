//! Game response DTOs.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Summary response for a game (used in list endpoints).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GameSummaryResponse {
    /// Game UUID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Human-readable identifier (e.g., "cs2", "aoe4") - used in URLs.
    #[schema(example = "cs2")]
    pub slug: String,

    /// Display name.
    #[schema(example = "Counter-Strike 2")]
    pub display_name: String,

    /// Short name.
    #[schema(example = "CS2")]
    pub short_name: Option<String>,

    /// Game description.
    #[schema(example = "Valve's tactical FPS")]
    pub description: Option<String>,

    /// Icon URL.
    #[schema(example = "https://example.com/cs2-icon.png")]
    pub icon_url: Option<String>,

    /// Default team size.
    #[schema(example = 5)]
    pub team_size_default: i32,

    /// Game status (active, maintenance, deprecated).
    #[schema(example = "active")]
    pub status: String,

    /// Whether the game is featured on homepage.
    #[schema(example = true)]
    pub is_featured: bool,
}

/// Team size configuration.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TeamSizeConfig {
    /// Minimum team size.
    #[schema(example = 5)]
    pub min: i32,

    /// Maximum team size.
    #[schema(example = 5)]
    pub max: i32,

    /// Default team size.
    #[schema(example = 5)]
    pub default: i32,
}

/// Detailed game information (single game endpoint).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GameDetailResponse {
    /// Game UUID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Human-readable identifier (e.g., "cs2", "aoe4") - used in URLs.
    #[schema(example = "cs2")]
    pub slug: String,

    /// Display name.
    #[schema(example = "Counter-Strike 2")]
    pub display_name: String,

    /// Short name.
    #[schema(example = "CS2")]
    pub short_name: Option<String>,

    /// Game description.
    pub description: Option<String>,

    /// Icon URL.
    pub icon_url: Option<String>,

    /// Logo URL.
    pub logo_url: Option<String>,

    /// Banner URL.
    pub banner_url: Option<String>,

    /// Team size configuration.
    pub team_size: TeamSizeConfig,

    /// Available maps for this game.
    pub maps: Vec<MapInfoResponse>,

    /// Rank tier definitions.
    pub rank_tiers: Vec<RankTierResponse>,

    /// Supported match formats (e.g., `["bo1", "bo3", "bo5"]`).
    #[schema(example = json!(["bo1", "bo3", "bo5"]))]
    pub supported_match_formats: Vec<String>,

    /// Default match format.
    #[schema(example = "bo3")]
    pub default_match_format: String,

    /// Available map pick/ban formats.
    pub map_pick_ban_formats: Vec<MapPickBanFormatResponse>,

    /// Game status.
    #[schema(example = "active")]
    pub status: String,

    /// Whether the game is featured.
    pub is_featured: bool,
}

/// Map information.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MapInfoResponse {
    /// Map identifier.
    #[schema(example = "de_dust2")]
    pub id: String,

    /// Display name.
    #[schema(example = "Dust II")]
    pub display_name: String,

    /// Map image URL.
    pub image_url: Option<String>,

    /// Game modes this map supports.
    #[schema(example = json!(["competitive", "casual"]))]
    pub game_modes: Vec<String>,
}

/// Rank tier definition.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RankTierResponse {
    /// Tier identifier.
    #[schema(example = "gold")]
    pub id: String,

    /// Display name.
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

/// Map pick/ban format.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MapPickBanFormatResponse {
    /// Format identifier.
    #[schema(example = "bo3_veto")]
    pub id: String,

    /// Display name.
    #[schema(example = "Best of 3 Veto")]
    pub display_name: String,

    /// Description of the format.
    #[schema(example = "Ban-Ban-Pick-Pick-Ban-Ban-Decider")]
    pub description: String,
}

// Conversion implementations

impl From<portal_plugins::MapInfo> for MapInfoResponse {
    fn from(info: portal_plugins::MapInfo) -> Self {
        Self {
            id: info.id,
            display_name: info.display_name,
            image_url: info.image_url,
            game_modes: info.game_modes,
        }
    }
}

impl From<portal_plugins::RankTier> for RankTierResponse {
    fn from(tier: portal_plugins::RankTier) -> Self {
        Self {
            id: tier.id,
            display_name: tier.display_name,
            min_rating: tier.min_rating,
            max_rating: tier.max_rating,
            color: tier.color,
            icon_url: tier.icon_url,
            order: tier.order,
        }
    }
}

impl From<portal_plugins::MapPickBanFormat> for MapPickBanFormatResponse {
    fn from(format: portal_plugins::MapPickBanFormat) -> Self {
        Self {
            id: format.id,
            display_name: format.display_name,
            description: format.description,
        }
    }
}
