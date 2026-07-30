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
    /// Earliest time the enricher may attempt this match again.
    pub next_attempt_at: DateTime<Utc>,
    /// When the current enricher claimed the row; `None` unless `enriching`.
    pub claimed_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// Demo extraction runs as its own retried stage, independent of `status`.
    pub demo_status: String,
    /// Attempts made at fetching + parsing the demo. Incremented when the job
    /// is leased rather than when it fails, so a worker that dies mid-parse
    /// has still spent an attempt.
    pub demo_retry_count: i32,
    pub demo_max_retries: i32,
    pub demo_next_attempt_at: DateTime<Utc>,
    pub demo_last_attempt_at: Option<DateTime<Utc>>,
    pub demo_error: Option<String>,
    pub discovered_at: DateTime<Utc>,
    pub enriched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
