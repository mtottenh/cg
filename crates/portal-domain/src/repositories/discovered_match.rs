//! Discovered match repository trait.

use crate::entities::discovered_match::DiscoveredMatch;
use async_trait::async_trait;
use portal_core::{DiscoveredMatchId, DomainError, GameId, SteamTrackingId};

/// Repository trait for discovered match operations.
#[async_trait]
pub trait DiscoveredMatchRepository: Send + Sync {
    /// Find by ID.
    async fn find_by_id(
        &self,
        id: DiscoveredMatchId,
    ) -> Result<Option<DiscoveredMatch>, DomainError>;

    /// Find by share code.
    async fn find_by_share_code(
        &self,
        share_code: &str,
    ) -> Result<Option<DiscoveredMatch>, DomainError>;

    /// Create a new discovered match (idempotent on share_code).
    async fn upsert(&self, cmd: CreateDiscoveredMatch) -> Result<DiscoveredMatch, DomainError>;

    /// Get pending/failed matches for enrichment (oldest first).
    async fn find_pending(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError>;

    /// Atomically claim a match for enrichment (status: pending → enriching).
    /// Returns true if claimed, false if already taken.
    async fn claim(&self, id: DiscoveredMatchId) -> Result<bool, DomainError>;

    /// Mark as enriched with GC data.
    async fn mark_enriched(
        &self,
        id: DiscoveredMatchId,
        gc_data: serde_json::Value,
        demo_url: Option<String>,
    ) -> Result<DiscoveredMatch, DomainError>;

    /// Find recent enriched matches that have a demo URL, optionally filtered by tracking ID.
    async fn find_recent_with_demo_url(
        &self,
        game_id: GameId,
        tracking_id: Option<SteamTrackingId>,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError>;

    /// Mark as failed.
    async fn mark_failed(
        &self,
        id: DiscoveredMatchId,
        error: &str,
    ) -> Result<DiscoveredMatch, DomainError>;

    /// Count rows per status, optionally scoped to one game.
    ///
    /// Backs the operator queue-depth view (P-73): without it a stalled
    /// enricher is only visible in the database.
    async fn count_by_status(
        &self,
        game_id: Option<GameId>,
    ) -> Result<Vec<(String, i64)>, DomainError>;

    /// Count rows that have exhausted their retry budget (`status = 'failed'`
    /// with `retry_count >= max_retries`) — the ones `find_pending` will never
    /// hand back again, i.e. permanently stuck.
    async fn count_retry_exhausted(&self, game_id: Option<GameId>) -> Result<i64, DomainError>;

    /// List rows, newest first, optionally filtered by game and status.
    async fn list_by_status(
        &self,
        game_id: Option<GameId>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError>;
}

/// Data for creating a discovered match.
#[derive(Debug, Clone)]
pub struct CreateDiscoveredMatch {
    pub tracking_id: SteamTrackingId,
    pub game_id: GameId,
    pub share_code: String,
    pub match_id: i64,
    pub outcome_id: i64,
    pub token: i32,
}
