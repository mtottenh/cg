//! Demo database entities.
//!
//! These entities map to the demo catalog tables:
//! `demos`, `demo_match_links`, `demo_players`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// =============================================================================
// DEMO
// =============================================================================

/// Database row for the `demos` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DemoRow {
    pub id: Uuid,
    pub game_id: Uuid,
    pub file_name: String,

    // S3 storage
    pub s3_bucket: String,
    pub s3_key: String,
    pub file_size_bytes: Option<i64>,

    // Categorization
    pub category: String,
    pub is_hidden: bool,

    // Organization linkage
    pub league_id: Option<Uuid>,
    pub tournament_id: Option<Uuid>,

    // Parsed metadata
    pub metadata: Option<serde_json::Value>,

    // Full stats
    pub stats_json: Option<serde_json::Value>,

    // Processing status
    pub status: String,
    pub stats_fetched_at: Option<DateTime<Utc>>,
    pub stats_fetch_error: Option<String>,

    // Admin actions
    pub categorized_by_user_id: Option<Uuid>,
    pub categorized_at: Option<DateTime<Utc>>,
    pub hidden_by_user_id: Option<Uuid>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,

    // Discovery
    pub discovered_at: DateTime<Utc>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new demo.
#[derive(Debug, Clone)]
pub struct NewDemo {
    pub game_id: Uuid,
    pub file_name: String,
    pub s3_bucket: String,
    pub s3_key: String,
    pub file_size_bytes: Option<i64>,
    pub discovered_at: DateTime<Utc>,
}

/// Data for updating demo stats.
#[derive(Debug, Clone)]
pub struct UpdateDemoStats {
    pub metadata: serde_json::Value,
    pub stats_json: serde_json::Value,
}

// =============================================================================
// DEMO MATCH LINK
// =============================================================================

/// Database row for the `demo_match_links` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DemoMatchLinkRow {
    pub id: Uuid,
    pub demo_id: Uuid,
    pub match_id: Uuid,
    pub game_number: Option<i32>,

    pub link_type: String,
    pub confidence_score: Option<f32>,

    pub validated: bool,
    pub validated_at: Option<DateTime<Utc>>,
    pub validation_result: Option<serde_json::Value>,

    pub linked_by_user_id: Option<Uuid>,
    pub linked_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Data for inserting a new demo-match link.
#[derive(Debug, Clone)]
pub struct NewDemoMatchLink {
    pub demo_id: Uuid,
    pub match_id: Uuid,
    pub game_number: Option<i32>,
    pub link_type: String,
    pub confidence_score: Option<f32>,
    pub linked_by_user_id: Option<Uuid>,
}

// =============================================================================
// DEMO PLAYER
// =============================================================================

/// Database row for the `demo_players` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DemoPlayerRow {
    pub id: Uuid,
    pub demo_id: Uuid,

    // Player identification
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,

    // Portal player link
    pub player_id: Option<Uuid>,

    // Stats
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub damage: i32,
    pub adr: f64,
    pub headshot_kills: i32,
    pub hs_percentage: f64,

    pub created_at: DateTime<Utc>,
}

/// Data for inserting a new demo player.
#[derive(Debug, Clone)]
pub struct NewDemoPlayer {
    pub demo_id: Uuid,
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub damage: i32,
    pub adr: f64,
    pub headshot_kills: i32,
    pub hs_percentage: f64,
}
