//! Player request DTOs.

use portal_domain::entities::SocialLinks;
use portal_domain::repositories::UpdatePlayer;
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Social links update request DTO.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SocialLinksRequest {
    /// Steam profile URL or username.
    #[schema(example = "https://steamcommunity.com/id/username")]
    pub steam: Option<String>,
    /// Discord username.
    #[schema(example = "username#1234")]
    pub discord: Option<String>,
    /// Twitch channel URL or username.
    #[schema(example = "twitchuser")]
    pub twitch: Option<String>,
    /// Twitter/X handle.
    #[schema(example = "twitteruser")]
    pub twitter: Option<String>,
    /// `YouTube` channel URL.
    #[schema(example = "https://youtube.com/@channel")]
    pub youtube: Option<String>,
}

impl From<SocialLinksRequest> for SocialLinks {
    fn from(req: SocialLinksRequest) -> Self {
        Self {
            steam: req.steam,
            discord: req.discord,
            twitch: req.twitch,
            twitter: req.twitter,
            youtube: req.youtube,
        }
    }
}

/// Request to update the current player's profile.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UpdatePlayerProfileRequest {
    /// New display name.
    #[validate(length(min = 3, max = 32, message = "Display name must be 3-32 characters"))]
    #[schema(example = "NewGamerTag")]
    pub display_name: Option<String>,

    /// New bio/description.
    #[validate(length(max = 500, message = "Bio must be at most 500 characters"))]
    #[schema(example = "I'm a competitive gamer.")]
    pub bio: Option<String>,

    /// ISO country code.
    #[validate(length(equal = 2, message = "Country code must be 2 characters"))]
    #[schema(example = "US")]
    pub country_code: Option<String>,

    /// Region within country.
    #[validate(length(max = 64, message = "Region must be at most 64 characters"))]
    #[schema(example = "California")]
    pub region: Option<String>,

    /// Player's timezone.
    #[validate(length(max = 64, message = "Timezone must be at most 64 characters"))]
    #[schema(example = "America/Los_Angeles")]
    pub timezone: Option<String>,

    /// SteamID64 for linking Steam account (set once, cannot be changed).
    /// Must be a valid SteamID64 numeric string (e.g., "76561198012345678").
    #[validate(length(min = 17, max = 20, message = "Steam ID must be 17-20 characters"))]
    #[schema(example = "76561198012345678")]
    pub steam_id: Option<String>,

    /// Social media links.
    pub social_links: Option<SocialLinksRequest>,
}

impl From<UpdatePlayerProfileRequest> for UpdatePlayer {
    fn from(req: UpdatePlayerProfileRequest) -> Self {
        Self {
            display_name: req.display_name,
            avatar_url: None, // Avatar is updated via file upload
            banner_url: None, // Banner is updated via file upload
            bio: req.bio,
            country_code: req.country_code,
            region: req.region,
            timezone: req.timezone,
            steam_id: req.steam_id,
            steam_id_64: None, // Derived from steam_id in the adapter
            social_links: req.social_links.map(SocialLinks::from),
        }
    }
}
