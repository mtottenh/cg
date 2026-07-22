//! Discovered match domain entity.

use chrono::{DateTime, Utc};
use portal_core::{DemoId, DiscoveredMatchId, GameId, SteamTrackingId};

/// A match discovered from Steam share code polling.
#[derive(Debug, Clone)]
pub struct DiscoveredMatch {
    pub id: DiscoveredMatchId,
    pub tracking_id: SteamTrackingId,
    pub game_id: GameId,
    pub share_code: String,
    pub match_id: i64,
    pub outcome_id: i64,
    pub token: i32,
    pub status: String,
    pub gc_data: Option<serde_json::Value>,
    pub demo_url: Option<String>,
    pub demo_id: Option<DemoId>,
    pub error: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub discovered_at: DateTime<Utc>,
    pub enriched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
