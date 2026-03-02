//! Discovered match service with business logic.

use crate::entities::discovered_match::DiscoveredMatch;
use crate::repositories::discovered_match::{CreateDiscoveredMatch, DiscoveredMatchRepository};
use portal_core::{DiscoveredMatchId, DomainError, GameId, SteamTrackingId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for discovered match business logic.
pub struct DiscoveredMatchService<DMR>
where
    DMR: DiscoveredMatchRepository,
{
    repo: Arc<DMR>,
}

impl<DMR> DiscoveredMatchService<DMR>
where
    DMR: DiscoveredMatchRepository,
{
    /// Create a new discovered match service.
    pub const fn new(repo: Arc<DMR>) -> Self {
        Self { repo }
    }

    /// Submit a discovered match (idempotent on share_code).
    #[instrument(skip(self, share_code))]
    pub async fn submit(
        &self,
        tracking_id: SteamTrackingId,
        game_id: GameId,
        share_code: &str,
        match_id: i64,
        outcome_id: i64,
        token: i32,
    ) -> Result<DiscoveredMatch, DomainError> {
        let result = self
            .repo
            .upsert(CreateDiscoveredMatch {
                tracking_id,
                game_id,
                share_code: share_code.to_string(),
                match_id,
                outcome_id,
                token,
            })
            .await?;

        info!(
            match_id = %result.id,
            share_code = %result.share_code,
            "Discovered match submitted"
        );

        Ok(result)
    }

    /// Get pending matches for enrichment.
    #[instrument(skip(self))]
    pub async fn get_pending(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        self.repo.find_pending(game_id, limit).await
    }

    /// Claim a match for enrichment.
    #[instrument(skip(self))]
    pub async fn claim(&self, id: DiscoveredMatchId) -> Result<bool, DomainError> {
        self.repo.claim(id).await
    }

    /// Mark a match as enriched with GC data.
    #[instrument(skip(self, gc_data))]
    pub async fn mark_enriched(
        &self,
        id: DiscoveredMatchId,
        gc_data: serde_json::Value,
        demo_url: Option<String>,
    ) -> Result<DiscoveredMatch, DomainError> {
        let result = self.repo.mark_enriched(id, gc_data, demo_url).await?;
        info!(match_id = %id, "Match enriched");
        Ok(result)
    }

    /// Mark a match enrichment as failed.
    #[instrument(skip(self))]
    pub async fn mark_failed(
        &self,
        id: DiscoveredMatchId,
        error: &str,
    ) -> Result<DiscoveredMatch, DomainError> {
        self.repo.mark_failed(id, error).await
    }
}

impl<DMR> Clone for DiscoveredMatchService<DMR>
where
    DMR: DiscoveredMatchRepository,
{
    fn clone(&self) -> Self {
        Self {
            repo: Arc::clone(&self.repo),
        }
    }
}
