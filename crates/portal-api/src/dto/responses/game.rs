//! Game response DTOs.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Summary response for a game (used in list endpoints).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GameSummaryResponse {
    /// Game UUID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Human-readable identifier (e.g., "cs2", "aoe2") - used in URLs.
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

    /// Display order (lower = shown first).
    ///
    /// P-90: this column has always existed (`migrations/0003_create_games.sql:42`,
    /// seeded `cs2 = 1` / `aoe2 = 2`) and `PATCH /v1/games/{game_id}` has always
    /// accepted it, but no *response* carried it. The admin edit modal therefore
    /// had nothing to seed its "Sort Order" field from and hardcoded `0` — showing
    /// every game a value that was not the truth — and, to avoid writing that
    /// fabricated `0` over the real order, only sent the field when it was
    /// non-zero, which made `0` unsettable. Returning the stored value fixes both
    /// halves.
    #[schema(example = 1)]
    pub sort_order: i32,
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

    /// Human-readable identifier (e.g., "cs2", "aoe2") - used in URLs.
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

    /// Map IDs in the active default pool.
    pub map_pool: Vec<String>,

    /// Game status.
    #[schema(example = "active")]
    pub status: String,

    /// Whether the game is featured.
    pub is_featured: bool,

    /// Display order (lower = shown first). See `GameSummaryResponse::sort_order`.
    #[schema(example = 1)]
    pub sort_order: i32,
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

    /// Engine-level map name (what the server and demo headers call the
    /// map). Absent means it equals `id`; set for workshop maps whose
    /// in-VPK name differs from the portal id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "de_cache")]
    pub engine_name: Option<String>,

    /// External identifier (e.g., Steam Workshop ID).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,

    /// External URL (e.g., full Steam Workshop URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
}

impl MapInfoResponse {
    /// The engine-level map name, falling back to the portal id.
    #[must_use]
    pub fn resolved_engine_name(&self) -> &str {
        self.engine_name.as_deref().unwrap_or(&self.id)
    }
}

/// Steam Workshop item details, for prefilling the admin map form.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WorkshopMapDetailsResponse {
    /// Workshop item id (decimal digits).
    #[schema(example = "3437809122")]
    pub workshop_id: String,

    /// Item title (suggested display name).
    pub title: Option<String>,

    /// Preview image URL (suggested map image).
    pub preview_url: Option<String>,

    /// Engine-level map-name hint derived from the item's upload filename
    /// (e.g. `de_cache.vpk` → `de_cache`). A hint, not a guarantee — the
    /// admin should correct it if demo validation later reports otherwise.
    #[schema(example = "de_cache")]
    pub engine_name_hint: Option<String>,

    /// Item size in bytes (drives the download-stall expectation).
    pub file_size_bytes: Option<i64>,

    /// When the author last updated the item.
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,

    /// App the item belongs to (CS2 = 730) — mismatches mean the pasted
    /// id is not a CS2 map.
    pub consumer_app_id: Option<i64>,

    /// 0 = public, 1 = friends-only, 2 = private, 3 = unlisted. Servers
    /// can only download public/unlisted items.
    pub visibility: Option<i64>,

    /// Whether Steam has banned the item.
    pub banned: bool,

    /// Canonical steamcommunity URL for the item.
    pub workshop_url: String,
}

impl From<crate::steam_workshop::WorkshopFileDetails> for WorkshopMapDetailsResponse {
    fn from(d: crate::steam_workshop::WorkshopFileDetails) -> Self {
        Self {
            workshop_url: format!(
                "https://steamcommunity.com/sharedfiles/filedetails/?id={}",
                d.workshop_id
            ),
            engine_name_hint: d
                .filename
                .as_deref()
                .and_then(crate::steam_workshop::engine_name_hint),
            updated_at: d
                .time_updated
                .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0)),
            workshop_id: d.workshop_id,
            title: d.title,
            preview_url: d.preview_url,
            file_size_bytes: d.file_size_bytes,
            consumer_app_id: d.consumer_app_id,
            visibility: d.visibility,
            banned: d.banned,
        }
    }
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
            engine_name: info.engine_name,
            external_id: info.external_id,
            external_url: info.external_url,
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
