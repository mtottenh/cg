//! Discovered match repository trait.

use crate::entities::discovered_match::DiscoveredMatch;
use async_trait::async_trait;
use portal_core::{DiscoveredMatchId, DomainError, GameId, SteamTrackingId};

/// Exponential-backoff schedule for a retried pipeline stage.
///
/// Delay before attempt `n + 1` is `min(base * 2^(n - 1), cap)`, then jittered
/// into `[50%, 100%]` of that value. Equal jitter rather than full jitter: full
/// jitter can schedule a retry almost immediately, which is the behaviour this
/// whole change exists to remove.
///
/// The arithmetic runs in SQL against the row's own attempt counter so it stays
/// atomic with the status write — computing it in Rust would need a read first,
/// and two enrichers could then interleave and clobber each other's schedule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackoffPolicy {
    pub base_secs: f64,
    pub cap_secs: f64,
}

/// How a demo fetch + parse attempt ended.
///
/// The worker classifies; the repository decides whether that classification
/// still has budget left to retry against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoOutcome {
    /// Parsed, rank updates extracted.
    Succeeded,
    /// Parsed cleanly, no rank updates present. Casual and deathmatch demos are
    /// legitimately empty — terminal success, never retried.
    Empty,
    /// Not on the CDN (404). Ambiguous between "not published yet" and "past
    /// retention", so it is retried; exhausting the budget settles it as
    /// terminally `unavailable`.
    Unavailable,
    /// Any other transient failure — timeout, 5xx, truncated body,
    /// decompression error. Retried, then terminal `failed`.
    Failed,
    /// Definitively gone (410, or a URL Valve has retired). Terminal
    /// immediately; spending eight attempts to re-confirm a 410 is pure waste.
    Gone,
}

impl DemoOutcome {
    /// Wire name used by the enricher's `demo-result` call.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Empty => "empty",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
            Self::Gone => "gone",
        }
    }

    /// Parse the wire name; `None` for anything unrecognised.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "succeeded" => Some(Self::Succeeded),
            "empty" => Some(Self::Empty),
            "unavailable" => Some(Self::Unavailable),
            "failed" => Some(Self::Failed),
            "gone" => Some(Self::Gone),
            _ => None,
        }
    }

    /// Whether another attempt could plausibly succeed, budget permitting.
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::Unavailable | Self::Failed)
    }
}

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

    /// Get matches that are due for an enrichment attempt (oldest first).
    ///
    /// Due means: retry budget remaining AND `next_attempt_at` has passed. A
    /// match that failed a moment ago is deliberately NOT returned — that
    /// immediate re-offer is what let a 90-second outage exhaust the budget.
    async fn find_pending(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError>;

    /// Return rows whose enrichment claim has outlived `lease_secs` to the
    /// queue, costing one retry each.
    ///
    /// A worker can die between `claim` and reporting a result — GC stream
    /// close, a portal 5xx on submit, the cycle deadline, SIGKILL. Without this
    /// the row sits in `enriching` forever, invisible to both the queue and the
    /// retry-exhausted count. Charging a retry matters: a match that reliably
    /// kills its worker must eventually stop being handed out.
    ///
    /// Returns the number of rows reclaimed.
    async fn reclaim_stale(
        &self,
        lease_secs: i64,
        backoff: BackoffPolicy,
    ) -> Result<u64, DomainError>;

    /// Atomically claim a match for enrichment (status: pending/failed →
    /// enriching). Returns true if claimed, false if already taken.
    async fn claim(&self, id: DiscoveredMatchId) -> Result<bool, DomainError>;

    /// Mark as enriched with GC data.
    ///
    /// Also opens the demo stage: `pending` when a demo URL came back,
    /// `not_applicable` when it did not. An already-resolved demo stage is left
    /// alone so a re-delivered enrichment does not re-download a demo that was
    /// parsed successfully.
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

    /// Mark as failed and schedule the next attempt per `backoff`.
    async fn mark_failed(
        &self,
        id: DiscoveredMatchId,
        error: &str,
        backoff: BackoffPolicy,
    ) -> Result<DiscoveredMatch, DomainError>;

    /// Lease up to `limit` demo-extraction jobs, banking one attempt each.
    ///
    /// One statement does the select, the attempt increment and the lease, so
    /// concurrent enrichers cannot take the same job (`FOR UPDATE SKIP LOCKED`)
    /// and a worker that dies mid-parse has already spent its attempt. The
    /// attempt counter lives in the database precisely so a restart does not
    /// hand a poison demo a fresh budget.
    async fn lease_demo_jobs(
        &self,
        game_id: GameId,
        limit: i64,
        lease_secs: i64,
    ) -> Result<Vec<DiscoveredMatch>, DomainError>;

    /// Record the result of a demo attempt.
    ///
    /// Terminal outcomes settle `demo_status` immediately. A retryable outcome
    /// with budget left goes back to `pending` with a backed-off
    /// `demo_next_attempt_at`; with the budget spent it settles terminally as
    /// `unavailable` or `failed` to match the last classification.
    async fn record_demo_result(
        &self,
        id: DiscoveredMatchId,
        outcome: DemoOutcome,
        error: Option<&str>,
        backoff: BackoffPolicy,
    ) -> Result<DiscoveredMatch, DomainError>;

    /// Count rows per status, optionally scoped to one game.
    ///
    /// Backs the operator queue-depth view (P-73): without it a stalled
    /// enricher is only visible in the database.
    async fn count_by_status(
        &self,
        game_id: Option<GameId>,
    ) -> Result<Vec<(String, i64)>, DomainError>;

    /// Count rows per `demo_status`, optionally scoped to one game.
    async fn count_by_demo_status(
        &self,
        game_id: Option<GameId>,
    ) -> Result<Vec<(String, i64)>, DomainError>;

    /// Count rows that have exhausted their retry budget (`status = 'failed'`
    /// with `retry_count >= max_retries`) — the ones `find_pending` will never
    /// hand back again, i.e. permanently stuck.
    async fn count_retry_exhausted(&self, game_id: Option<GameId>) -> Result<i64, DomainError>;

    /// Return failed matches to the enrichment queue with a fresh budget.
    ///
    /// `only_exhausted` narrows to the rows the enricher will never pick up
    /// again on its own (`retry_count >= max_retries`); `None` for `game_id`
    /// means every game. Returns how many rows were requeued.
    ///
    /// This exists because a bug in the worker can spend a budget on
    /// something that was never the match's fault — a dead Steam socket
    /// reported as a per-match GC failure, say. Nothing else can undo that:
    /// `find_pending` excludes an exhausted row by design, so without this
    /// the only recovery is hand-written SQL against production.
    async fn requeue_failed(
        &self,
        game_id: Option<GameId>,
        only_exhausted: bool,
    ) -> Result<u64, DomainError>;

    /// Return one match to the enrichment queue with a fresh budget,
    /// whatever state it is in.
    async fn requeue_one(&self, id: DiscoveredMatchId) -> Result<DiscoveredMatch, DomainError>;

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
