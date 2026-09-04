//! Discovered match service with business logic.

use crate::entities::discovered_match::DiscoveredMatch;
use crate::repositories::discovered_match::{
    BackoffPolicy, CreateDiscoveredMatch, DemoOutcome, DiscoveredMatchRepository,
};
use portal_core::{DiscoveredMatchId, DomainError, GameId, SteamTrackingId};
use std::sync::Arc;
use tracing::{info, instrument, warn};

/// Backoff for GC enrichment: 1m, 2m, 4m, 8m, 16m, 32m — capped at an hour.
///
/// Sized against the failure it is absorbing. GC unavailability and Valve-side
/// hiccups resolve on a scale of minutes to an hour, so the default budget of 6
/// attempts spans roughly that. The previous behaviour retried with no delay at
/// all, which meant three attempts inside 90 seconds and a match written off
/// before the outage it hit had any chance to end.
pub const ENRICH_BACKOFF: BackoffPolicy = BackoffPolicy {
    base_secs: 60.0,
    cap_secs: 3_600.0,
};

/// Backoff for demo fetch + parse: 5m, 10m, 20m … capped at 6h.
///
/// A different timescale entirely. Valve does not publish a demo the moment a
/// match ends, and the delay is neither documented nor bounded in practice, so
/// the default 8 attempts stretch over roughly a day — comfortably inside the
/// retention window, and long enough that "not uploaded yet" is not mistaken
/// for "does not exist".
pub const DEMO_BACKOFF: BackoffPolicy = BackoffPolicy {
    base_secs: 300.0,
    cap_secs: 21_600.0,
};

/// How long an enrichment claim is honoured before the row is assumed abandoned.
///
/// Must exceed the worst realistic single-match attempt (a GC call plus the
/// enricher's own cycle deadline) or a slow-but-alive worker gets its match
/// stolen and does duplicate work.
pub const ENRICH_CLAIM_LEASE_SECS: i64 = 15 * 60;

/// How long a leased demo job is held before another worker may take it.
///
/// Covers a download plus a bzip2 decompress plus a full parse, with margin.
pub const DEMO_CLAIM_LEASE_SECS: i64 = 20 * 60;

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

    /// Fetch a discovered match by id.
    #[instrument(skip(self))]
    pub async fn get(&self, id: DiscoveredMatchId) -> Result<DiscoveredMatch, DomainError> {
        self.repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal(format!("Discovered match not found: {id}")))
    }

    /// Get matches due for an enrichment attempt.
    ///
    /// Sweeps expired claims first. Doing it here rather than in a background
    /// task means recovery is driven by the same thing that consumes the queue:
    /// if no enricher is asking for work there is nothing to recover *for*, and
    /// when one comes back the sweep runs before its first fetch.
    #[instrument(skip(self))]
    pub async fn get_pending(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        match self
            .repo
            .reclaim_stale(ENRICH_CLAIM_LEASE_SECS, ENRICH_BACKOFF)
            .await
        {
            Ok(0) => {}
            Ok(n) => warn!(
                reclaimed = n,
                lease_secs = ENRICH_CLAIM_LEASE_SECS,
                "Reclaimed enrichment claims whose worker never reported back"
            ),
            // Recovery is best-effort: a sweep failure must not stop the
            // enricher from draining the work that is already queued.
            Err(e) => warn!(error = %e, "Failed to reclaim stale enrichment claims"),
        }

        self.repo.find_pending(game_id, limit).await
    }

    /// Lease demo-extraction jobs, banking one attempt against each.
    #[instrument(skip(self))]
    pub async fn lease_demo_jobs(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        self.repo
            .lease_demo_jobs(game_id, limit, DEMO_CLAIM_LEASE_SECS)
            .await
    }

    /// Record how a demo attempt ended, scheduling a retry if budget remains.
    #[instrument(skip(self, error))]
    pub async fn record_demo_result(
        &self,
        id: DiscoveredMatchId,
        outcome: DemoOutcome,
        error: Option<&str>,
    ) -> Result<DiscoveredMatch, DomainError> {
        let result = self
            .repo
            .record_demo_result(id, outcome, error, DEMO_BACKOFF)
            .await?;

        if result.demo_status == "pending" {
            info!(
                match_id = %id,
                outcome = outcome.as_str(),
                attempt = result.demo_retry_count,
                max = result.demo_max_retries,
                next_attempt_at = %result.demo_next_attempt_at,
                "Demo attempt failed, retry scheduled"
            );
        } else {
            info!(
                match_id = %id,
                demo_status = %result.demo_status,
                attempts = result.demo_retry_count,
                "Demo stage settled"
            );
        }

        Ok(result)
    }

    /// Demo-stage depth per `demo_status`, optionally scoped to one game.
    #[instrument(skip(self))]
    pub async fn count_by_demo_status(
        &self,
        game_id: Option<GameId>,
    ) -> Result<Vec<(String, i64)>, DomainError> {
        self.repo.count_by_demo_status(game_id).await
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

    /// Get recent enriched matches that have a demo URL.
    #[instrument(skip(self))]
    pub async fn get_recent_with_demo_url(
        &self,
        game_id: GameId,
        tracking_id: Option<SteamTrackingId>,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        self.repo
            .find_recent_with_demo_url(game_id, tracking_id, limit)
            .await
    }

    /// Mark a match enrichment as failed and schedule the next attempt.
    #[instrument(skip(self))]
    pub async fn mark_failed(
        &self,
        id: DiscoveredMatchId,
        error: &str,
    ) -> Result<DiscoveredMatch, DomainError> {
        let result = self.repo.mark_failed(id, error, ENRICH_BACKOFF).await?;

        if result.retry_count >= result.max_retries {
            warn!(
                match_id = %id,
                attempts = result.retry_count,
                error,
                "Enrichment retry budget exhausted — this match will not be retried"
            );
        } else {
            info!(
                match_id = %id,
                attempt = result.retry_count,
                max = result.max_retries,
                next_attempt_at = %result.next_attempt_at,
                "Enrichment attempt failed, retry scheduled"
            );
        }

        Ok(result)
    }

    /// Return failed matches to the enrichment queue with a fresh retry
    /// budget.
    ///
    /// The operator-facing repair for a worker bug that spent budgets on
    /// something other than the match: matches written off with
    /// "Trying to work with closed connection" had never actually been asked
    /// of Valve. `only_exhausted` limits it to the rows the enricher has
    /// given up on for good.
    #[instrument(skip(self))]
    pub async fn requeue_failed(
        &self,
        game_id: Option<GameId>,
        only_exhausted: bool,
    ) -> Result<u64, DomainError> {
        let requeued = self.repo.requeue_failed(game_id, only_exhausted).await?;
        info!(requeued, only_exhausted, "Requeued failed matches");
        Ok(requeued)
    }

    /// Return one match to the enrichment queue with a fresh retry budget.
    #[instrument(skip(self))]
    pub async fn requeue_one(&self, id: DiscoveredMatchId) -> Result<DiscoveredMatch, DomainError> {
        let result = self.repo.requeue_one(id).await?;
        info!(match_id = %id, "Match requeued for enrichment");
        Ok(result)
    }

    /// Queue depth per status, optionally scoped to one game (admin
    /// pipeline view — P-73).
    #[instrument(skip(self))]
    pub async fn count_by_status(
        &self,
        game_id: Option<GameId>,
    ) -> Result<Vec<(String, i64)>, DomainError> {
        self.repo.count_by_status(game_id).await
    }

    /// Count matches whose retry budget is spent — the enricher will never
    /// pick these up again.
    #[instrument(skip(self))]
    pub async fn count_retry_exhausted(&self, game_id: Option<GameId>) -> Result<i64, DomainError> {
        self.repo.count_retry_exhausted(game_id).await
    }

    /// List discovered matches newest-first for the admin pipeline view.
    #[instrument(skip(self))]
    pub async fn list_for_admin(
        &self,
        game_id: Option<GameId>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError> {
        self.repo.list_by_status(game_id, status, limit).await
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
