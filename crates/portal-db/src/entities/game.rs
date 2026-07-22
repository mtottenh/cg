//! Game database entity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `games` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct GameRow {
    /// UUID primary key.
    pub id: Uuid,

    /// Human-readable identifier (e.g., "cs2", "aoe4") - used in URLs and API.
    pub slug: String,

    // Display Information
    pub display_name: String,
    pub short_name: Option<String>,
    pub description: Option<String>,

    // Media
    pub icon_url: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,

    // Configuration (JSONB)
    pub config: serde_json::Value,
    pub default_queue_config: serde_json::Value,
    pub default_lobby_config: serde_json::Value,

    // Plugin Reference
    pub plugin_id: String,
    pub plugin_version: String,

    // Team Configuration
    pub team_size_min: i32,
    pub team_size_max: i32,
    pub team_size_default: i32,

    // Maps (JSONB arrays)
    pub available_maps: serde_json::Value,
    pub default_map_pool: serde_json::Value,

    // Ranking Configuration (JSONB)
    pub rank_tiers: serde_json::Value,

    // Status
    pub status: String,
    pub is_featured: bool,
    pub sort_order: i32,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new game.
#[derive(Debug, Clone)]
pub struct NewGame {
    pub slug: String,
    pub display_name: String,
    pub short_name: Option<String>,
    pub description: Option<String>,
    pub plugin_id: String,
    pub plugin_version: String,
    pub team_size_min: i32,
    pub team_size_max: i32,
    pub team_size_default: i32,
}

/// Data for updating an existing game.
#[derive(Debug, Clone, Default)]
pub struct UpdateGame {
    pub display_name: Option<String>,
    pub short_name: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub config: Option<serde_json::Value>,
    pub default_queue_config: Option<serde_json::Value>,
    pub default_lobby_config: Option<serde_json::Value>,
    pub available_maps: Option<serde_json::Value>,
    pub default_map_pool: Option<serde_json::Value>,
    pub rank_tiers: Option<serde_json::Value>,
    pub status: Option<String>,
    pub is_featured: Option<bool>,
    pub sort_order: Option<i32>,
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub team_size_default: Option<i32>,
}
