//! Player database entities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `players` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerRow {
    pub id: Uuid,

    // Relationships
    pub user_id: Uuid,

    // Profile Information
    pub display_name: String,
    pub display_name_normalized: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,

    // Location
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,

    // Social Links (JSONB)
    pub social_links: serde_json::Value,

    // Privacy Settings (JSONB)
    pub privacy_settings: serde_json::Value,

    // Platform Settings (JSONB)
    pub notification_settings: serde_json::Value,
    pub ui_preferences: serde_json::Value,

    // Steam Integration
    pub steam_id: Option<String>,
    pub steam_id_64: Option<i64>,
    pub steam_profile: Option<serde_json::Value>,

    // Looking for Team
    pub looking_for_team: bool,

    // Metadata
    pub featured_badge_id: Option<Uuid>,
    pub title: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new player.
#[derive(Debug, Clone)]
pub struct NewPlayer {
    pub user_id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub country_code: Option<String>,
}

/// Data for updating an existing player.
#[derive(Debug, Clone, Default)]
pub struct UpdatePlayer {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: Option<serde_json::Value>,
    pub privacy_settings: Option<serde_json::Value>,
    pub notification_settings: Option<serde_json::Value>,
    pub steam_id: Option<String>,
    pub steam_id_64: Option<i64>,
}

/// Database row for the `player_game_profiles` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerGameProfileRow {
    pub id: Uuid,

    // Relationships
    pub player_id: Uuid,
    pub game_id: Uuid,

    // Rating System (Glicko-2)
    pub rating: i32,
    pub rating_deviation: i32,
    pub volatility: f64,
    pub peak_rating: i32,
    pub peak_rating_at: Option<DateTime<Utc>>,

    // Rank Display
    pub rank_tier: Option<String>,
    pub rank_division: Option<i32>,
    pub rank_points: Option<i32>,
    pub rank_updated_at: Option<DateTime<Utc>>,

    // Match Statistics
    pub matches_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub win_streak: i32,
    pub best_win_streak: i32,

    // Time Statistics
    pub total_playtime_minutes: i32,
    pub avg_match_duration_minutes: Option<i32>,

    // Game-Specific Stats (JSONB)
    pub game_specific_stats: serde_json::Value,

    // Achievements & Badges (JSONB)
    pub achievements: serde_json::Value,
    pub equipped_badge_id: Option<String>,

    // Timestamps
    pub first_match_at: Option<DateTime<Utc>>,
    pub last_match_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new player game profile.
#[derive(Debug, Clone)]
pub struct NewPlayerGameProfile {
    pub player_id: Uuid,
    pub game_id: Uuid,
}

/// Data for updating rating after a match.
#[derive(Debug, Clone)]
pub struct UpdatePlayerRating {
    pub rating: i32,
    pub rating_deviation: i32,
    pub volatility: f64,
    pub is_win: bool,
    pub is_loss: bool,
    pub is_draw: bool,
    pub match_duration_minutes: Option<i32>,
}
