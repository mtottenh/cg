//! Demo upload attempt repository trait.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{DemoId, GameServerId, TournamentMatchId};
use portal_core::types::{DemoUploadLinkNote, DemoUploadOutcome};

use crate::entities::{
    DemoUploadAttempt, DemoUploadAttemptDetail, LatestServerUpload, MissingDemo,
    ServerUploadsSummary,
};

/// Fields for recording one upload request.
#[derive(Debug, Clone)]
pub struct CreateDemoUploadAttempt {
    pub server_id: Option<GameServerId>,
    pub outcome: DemoUploadOutcome,
    pub http_status: i32,
    pub file_name: Option<String>,
    pub matchzy_id: Option<i64>,
    pub map_number: Option<i32>,
    pub bytes: i64,
    pub duration_ms: i32,
    pub demo_id: Option<DemoId>,
    pub linked_match_id: Option<TournamentMatchId>,
    pub link_note: Option<DemoUploadLinkNote>,
    pub error: Option<String>,
}

/// Admin list filter. Results are newest first; `before` is the keyset
/// cursor on `received_at` (exclusive).
#[derive(Debug, Clone, Default)]
pub struct DemoUploadAttemptFilter {
    pub server_id: Option<GameServerId>,
    pub outcome: Option<DemoUploadOutcome>,
    pub demo_id: Option<DemoId>,
    pub before: Option<DateTime<Utc>>,
    pub limit: i64,
}

/// Repository for MatchZy demo upload attempts.
#[async_trait]
pub trait DemoUploadAttemptRepository: Send + Sync + 'static {
    /// Record one attempt (`received_at` = `now()`).
    async fn record(
        &self,
        attempt: CreateDemoUploadAttempt,
    ) -> Result<DemoUploadAttempt, DomainError>;

    /// Attempts matching `filter`, newest first, with joined names.
    async fn list(
        &self,
        filter: DemoUploadAttemptFilter,
    ) -> Result<Vec<DemoUploadAttemptDetail>, DomainError>;

    /// The server-upload stage counts as of `now` (thresholds in
    /// [`crate::entities::demo_upload`]).
    async fn summary(&self, now: DateTime<Utc>) -> Result<ServerUploadsSummary, DomainError>;

    /// Completed reservations whose demo never arrived, newest completed
    /// first — the same predicate as [`ServerUploadsSummary::missing`].
    async fn list_missing(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MissingDemo>, DomainError>;

    /// Each listed server's most recent attempt (servers that never
    /// uploaded are absent). One query, whatever the number of servers.
    async fn latest_per_server(
        &self,
        server_ids: &[GameServerId],
    ) -> Result<Vec<LatestServerUpload>, DomainError>;

    /// Delete attempts received before `before`. Returns the count.
    async fn prune(&self, before: DateTime<Utc>) -> Result<u64, DomainError>;
}
