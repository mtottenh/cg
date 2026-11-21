//! Game-server update repository trait.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{GameServerId, GameServerUpdateJobId, UserId};
use portal_core::types::{UpdateJobStatus, UpdateOperation};
use serde_json::{Map, Value};

use crate::entities::{
    AgentUpdateReport, GameServerUpdateJob, GameServerUpdateState, ResultOutcome, UpdateProgress,
    UpdateResult,
};

/// A job the portal is about to ask an agent to run.
#[derive(Debug, Clone)]
pub struct CreateUpdateJob {
    pub id: GameServerUpdateJobId,
    pub server_id: GameServerId,
    pub operation: UpdateOperation,
    pub requested_by: UserId,
    pub forced: bool,
}

/// Update jobs and the per-server update state.
#[async_trait]
pub trait GameServerUpdateRepository: Send + Sync + 'static {
    /// The server's update state; `None` when the server does not exist.
    async fn get_state(
        &self,
        server_id: GameServerId,
    ) -> Result<Option<GameServerUpdateState>, DomainError>;

    /// Store the update fields of a heartbeat.
    async fn record_agent_report(
        &self,
        server_id: GameServerId,
        report: &AgentUpdateReport,
    ) -> Result<(), DomainError>;

    /// Store a check (the `check` object of an `update_state` frame) and
    /// replace the best-known installed versions with `installed`.
    async fn record_update_state(
        &self,
        server_id: GameServerId,
        check: &Value,
        checked_at: Option<DateTime<Utc>>,
        installed: Option<&Value>,
    ) -> Result<(), DomainError>;

    /// Overlay the non-null fields of `versions` (a successful job's
    /// `after`) on the best-known installed versions — unless a check
    /// taken after `as_of` is already stored.
    async fn overlay_installed_versions(
        &self,
        server_id: GameServerId,
        versions: &Value,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<(), DomainError>;

    /// Replace the portal-managed MatchZy overrides and mark them as not
    /// yet accepted by the agent.
    async fn set_matchzy_overrides(
        &self,
        server_id: GameServerId,
        overrides: &Map<String, Value>,
    ) -> Result<(), DomainError>;

    /// The agent accepted `overrides`: clear the pending mark — unless the
    /// stored set has changed since (a newer save is still undelivered).
    async fn mark_overrides_delivered(
        &self,
        server_id: GameServerId,
        overrides: &Map<String, Value>,
    ) -> Result<(), DomainError>;

    /// Insert a `queued` job. `Conflict` when the server already has a job
    /// in flight.
    async fn create_job(&self, job: CreateUpdateJob) -> Result<GameServerUpdateJob, DomainError>;

    async fn find_job(
        &self,
        id: GameServerUpdateJobId,
    ) -> Result<Option<GameServerUpdateJob>, DomainError>;

    /// The server's job in flight (`queued` or `running`), if any.
    async fn find_active_job(
        &self,
        server_id: GameServerId,
    ) -> Result<Option<GameServerUpdateJob>, DomainError>;

    /// Newest first.
    async fn list_recent_jobs(
        &self,
        server_id: GameServerId,
        limit: i64,
    ) -> Result<Vec<GameServerUpdateJob>, DomainError>;

    /// Apply a progress report: move the job to `running`, store phase and
    /// log tail, bump `updated_at`. A job the portal has never heard of is
    /// created (after marking any other `running` job of the server `lost`:
    /// the helper runs one at a time). Never touches a job whose outcome
    /// the agent has reported, nor a job of another server. A `lost` job
    /// is revived: `lost` is the portal's guess, and the agent saying the
    /// job is running is better evidence. Returns the job when something
    /// was written.
    async fn apply_progress(
        &self,
        server_id: GameServerId,
        progress: &UpdateProgress,
    ) -> Result<Option<GameServerUpdateJob>, DomainError>;

    /// Store a job's terminal state, creating the row when the portal did
    /// not start the job. Idempotent — see
    /// [`GameServerUpdateJob::accepts_result`].
    async fn apply_result(
        &self,
        server_id: GameServerId,
        result: &UpdateResult,
    ) -> Result<ResultOutcome, DomainError>;

    /// Bump a job the agent still names as in flight: `running`, fresh
    /// `updated_at`, the phase when given. Same rules as `apply_progress`,
    /// except that an unknown job is not created.
    async fn touch_job(
        &self,
        server_id: GameServerId,
        id: GameServerUpdateJobId,
        phase: Option<&str>,
    ) -> Result<(), DomainError>;

    /// End a job in flight with a portal-side verdict (`failed` when the
    /// agent refused it, `lost` when it went silent). Returns the job when
    /// it was still in flight; `None` when it had already ended.
    async fn finish_job(
        &self,
        id: GameServerUpdateJobId,
        status: UpdateJobStatus,
        error: &str,
    ) -> Result<Option<GameServerUpdateJob>, DomainError>;

    /// Jobs in flight with no progress since `cutoff`, oldest first.
    async fn list_stalled_jobs(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<GameServerUpdateJob>, DomainError>;
}
