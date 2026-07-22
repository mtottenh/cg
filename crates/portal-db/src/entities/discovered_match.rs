//! Discovered match database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `discovered_matches` table.
#[derive(Debug, Clone, FromRow)]
pub struct DiscoveredMatchRow {
    pub id: Uuid,
    pub tracking_id: Uuid,
    pub game_id: Uuid,
    pub share_code: String,
    pub match_id: i64,
    pub outcome_id: i64,
    pub token: i32,
    pub status: String,
    pub gc_data: Option<serde_json::Value>,
    pub demo_url: Option<String>,
    pub demo_id: Option<Uuid>,
    pub error: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub discovered_at: DateTime<Utc>,
    pub enriched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
