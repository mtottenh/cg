//! Player response DTOs.

use portal_domain::entities::{Player, SocialLinks};
use portal_plugins::types::DisplayStat;
use serde::Serialize;
use utoipa::ToSchema;

use super::DisplayStatResponse;

/// Social links response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SocialLinksResponse {
    /// Steam profile URL or username.
    #[schema(example = "https://steamcommunity.com/id/username")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steam: Option<String>,
    /// Discord username.
    #[schema(example = "username#1234")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<String>,
    /// Twitch channel URL or username.
    #[schema(example = "twitchuser")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitch: Option<String>,
    /// Twitter/X handle.
    #[schema(example = "twitteruser")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<String>,
    /// `YouTube` channel URL.
    #[schema(example = "https://youtube.com/@channel")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub youtube: Option<String>,
}

impl From<SocialLinks> for SocialLinksResponse {
    fn from(links: SocialLinks) -> Self {
        Self {
            steam: links.steam,
            discord: links.discord,
            twitch: links.twitch,
            twitter: links.twitter,
            youtube: links.youtube,
        }
    }
}

/// Player response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlayerResponse {
    /// Unique player identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// User ID this player belongs to.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440001")]
    pub user_id: String,

    /// Display name.
    #[schema(example = "ProGamer123")]
    pub display_name: String,

    /// Avatar URL.
    #[schema(example = "https://example.com/avatar.png")]
    pub avatar_url: Option<String>,

    /// Banner URL.
    pub banner_url: Option<String>,

    /// Player bio/description.
    #[schema(example = "Professional gamer since 2018")]
    pub bio: Option<String>,

    /// ISO country code.
    #[schema(example = "US")]
    pub country_code: Option<String>,

    /// Region within country.
    #[schema(example = "California")]
    pub region: Option<String>,

    /// Player's timezone.
    #[schema(example = "America/Los_Angeles")]
    pub timezone: Option<String>,

    /// Social media links.
    pub social_links: SocialLinksResponse,

    /// The player's SteamID64 (null if not linked).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "76561198012345678")]
    pub steam_id: Option<String>,

    /// Whether Steam account is linked.
    pub steam_linked: bool,

    /// Whether the player is looking for a team.
    pub looking_for_team: bool,

    /// When the player profile was created.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,

    /// When the player profile was last updated.
    pub updated_at: String,
}

impl From<Player> for PlayerResponse {
    fn from(player: Player) -> Self {
        let steam_linked = player.has_steam_linked();
        Self {
            id: player.id.to_string(),
            user_id: player.user_id.to_string(),
            display_name: player.display_name,
            avatar_url: player.avatar_url,
            banner_url: player.banner_url,
            bio: player.bio,
            country_code: player.country_code,
            region: player.region,
            timezone: player.timezone,
            social_links: SocialLinksResponse::from(player.social_links),
            steam_id: player.steam_id,
            steam_linked,
            looking_for_team: player.looking_for_team,
            created_at: player.created_at.to_rfc3339(),
            updated_at: player.updated_at.to_rfc3339(),
        }
    }
}

/// Player search result DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlayerSearchResponse {
    /// Player ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Display name.
    #[schema(example = "ProGamer123")]
    pub display_name: String,

    /// Avatar URL.
    pub avatar_url: Option<String>,

    /// Country code.
    #[schema(example = "US")]
    pub country_code: Option<String>,

    /// Whether the player is looking for a team.
    pub looking_for_team: bool,

    /// Game-specific display stats (populated when game_id filter is used).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub display_stats: Vec<DisplayStatResponse>,
}

impl PlayerSearchResponse {
    /// Create a search response enriched with plugin-formatted display stats.
    pub fn with_display_stats(player: Player, display_stats: Vec<DisplayStat>) -> Self {
        Self {
            id: player.id.to_string(),
            display_name: player.display_name,
            avatar_url: player.avatar_url,
            country_code: player.country_code,
            looking_for_team: player.looking_for_team,
            display_stats: display_stats.into_iter().map(DisplayStatResponse::from).collect(),
        }
    }
}

impl From<Player> for PlayerSearchResponse {
    fn from(player: Player) -> Self {
        Self {
            id: player.id.to_string(),
            display_name: player.display_name,
            avatar_url: player.avatar_url,
            country_code: player.country_code,
            looking_for_team: player.looking_for_team,
            display_stats: Vec::new(),
        }
    }
}
