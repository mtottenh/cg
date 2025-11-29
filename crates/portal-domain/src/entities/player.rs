//! Player domain entity.

use chrono::{DateTime, Utc};
use portal_core::{PlayerId, UserId};
use serde::{Deserialize, Serialize};

/// Social media links for a player profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SocialLinks {
    /// Steam profile URL or username.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steam: Option<String>,
    /// Discord username (e.g., "username#1234" or new format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<String>,
    /// Twitch channel URL or username.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitch: Option<String>,
    /// Twitter/X handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<String>,
    /// YouTube channel URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub youtube: Option<String>,
}

impl SocialLinks {
    /// Check if all social links are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steam.is_none()
            && self.discord.is_none()
            && self.twitch.is_none()
            && self.twitter.is_none()
            && self.youtube.is_none()
    }
}

/// Player domain entity.
///
/// A player is the gaming identity linked to a user account.
#[derive(Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub user_id: UserId,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: SocialLinks,
    pub steam_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Player {
    /// Check if the player has linked their Steam account.
    #[must_use]
    pub fn has_steam_linked(&self) -> bool {
        self.steam_id.is_some()
    }

    /// Get the player's country name (if set).
    #[must_use]
    pub fn country(&self) -> Option<&str> {
        self.country_code.as_deref()
    }
}
