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

    /// Mark as failed.
    async fn mark_failed(
        &self,
        id: DiscoveredMatchId,
        error: &str,
    ) -> Result<DiscoveredMatch, DomainError>;
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
