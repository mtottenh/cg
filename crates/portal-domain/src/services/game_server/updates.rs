//! Game-server update service (docs/gameserver-lgsm-updates-design.md §7).
//!
//! Turns the agent's update frames (portal-server-agent README,
//! "Protocol") into job rows and per-server update state. Whether an agent
//! is connected, and talking to it, is the API layer's business; this
//! service owns what the frames mean and what is stored.

use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{GameServerId, GameServerUpdateJobId, UserId};
use portal_core::types::{UpdateJobStatus, UpdateJobTrigger, UpdateOperation};
use serde_json::{Map, Value};
use std::sync::Arc;

use crate::entities::{
    AgentUpdateReport, GameServerUpdateJob, GameServerUpdateState, LOG_TAIL_MAX_BYTES,
    ResultOutcome, UpdateProgress, UpdateResult, bound_log_tail,
};
use crate::repositories::{CreateUpdateJob, GameServerUpdateRepository};

/// A job in flight with no progress for this long is asked after, then
/// declared lost.
pub const UPDATE_JOB_STALL_MINUTES: i64 = 20;

/// What `update_status` reconciliation concluded about a stalled job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciled {
    /// The helper is still on this job; it stays in flight.
    StillRunning,
    /// The helper had finished it; the real outcome is stored.
    Finished,
    /// The host knows nothing (more) about it.
    Lost,
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// A JSON field that is present and not `null`.
fn opt_json(v: &Value, key: &str) -> Option<Value> {
    v.get(key).filter(|x| !x.is_null()).cloned()
}

fn opt_time(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    v.get(key)
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// The job id of an update frame: a UUID, as the agent requires of the
/// portal and generates itself.
#[must_use]
pub fn frame_job_id(frame: &Value) -> Option<GameServerUpdateJobId> {
    frame.get("job_id")?.as_str()?.parse().ok()
}

fn frame_operation(frame: &Value) -> Option<UpdateOperation> {
    frame.get("operation")?.as_str()?.parse().ok()
}

/// A frame without a (known) trigger was started on the host.
fn frame_trigger(frame: &Value) -> UpdateJobTrigger {
    frame
        .get("trigger")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .unwrap_or(UpdateJobTrigger::Local)
}

/// Read an `update_progress` frame (or a running `update_status` state).
/// `None` when it names no job, an unknown operation, or a `check` (which
/// has no job row while it runs).
#[must_use]
pub fn parse_progress(frame: &Value) -> Option<UpdateProgress> {
    let operation = frame_operation(frame)?;
    if operation == UpdateOperation::Check {
        return None;
    }
    Some(UpdateProgress {
        job_id: frame_job_id(frame)?,
        operation,
        trigger: frame_trigger(frame),
        phase: opt_str(frame, "phase"),
        log_tail: opt_str(frame, "log_tail").map(|t| bound_log_tail(&t, LOG_TAIL_MAX_BYTES)),
    })
}

/// Read an `update_result` frame (or a terminal `update_status` state).
/// `None` when it names no job, an unknown operation, or a status that is
/// not one of the agent's three terminal ones.
#[must_use]
pub fn parse_result(frame: &Value) -> Option<UpdateResult> {
    let status: UpdateJobStatus = frame.get("status")?.as_str()?.parse().ok()?;
    if !matches!(
        status,
        UpdateJobStatus::Succeeded | UpdateJobStatus::Failed | UpdateJobStatus::RolledBack
    ) {
        return None;
    }
    Some(UpdateResult {
        job_id: frame_job_id(frame)?,
        operation: frame_operation(frame)?,
        trigger: frame_trigger(frame),
        status,
        phase: opt_str(frame, "phase"),
        failed_phase: opt_str(frame, "failed_phase"),
        started_at: opt_time(frame, "started_at"),
        finished_at: opt_time(frame, "finished_at"),
        before_versions: opt_json(frame, "before"),
        after_versions: opt_json(frame, "after"),
        latest_versions: opt_json(frame, "latest"),
        load: opt_json(frame, "load"),
        config_report: opt_json(frame, "config_report"),
        restarted: frame.get("restarted").and_then(Value::as_bool),
        error: opt_str(frame, "error").map(|e| bound_log_tail(&e, 2048)),
        log_tail: opt_str(frame, "log_tail").map(|t| bound_log_tail(&t, LOG_TAIL_MAX_BYTES)),
        agent_synthesized: frame
            .get("agent_synthesized")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Game-server update service.
#[derive(Clone)]
pub struct GameServerUpdateService<R>
where
    R: GameServerUpdateRepository,
{
    repo: Arc<R>,
}

impl<R> GameServerUpdateService<R>
where
    R: GameServerUpdateRepository,
{
    /// Create a new update service.
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    // ------------------------------------------------------------------
    // Per-server state
    // ------------------------------------------------------------------

    /// The server's update state.
    pub async fn state(
        &self,
        server_id: GameServerId,
    ) -> Result<GameServerUpdateState, DomainError> {
        self.repo
            .get_state(server_id)
            .await?
            .ok_or(DomainError::GameServerNotFound(server_id))
    }

    /// Store the update fields of a heartbeat. A job the heartbeat names
    /// as in flight counts as progress on it.
    pub async fn record_heartbeat(
        &self,
        server_id: GameServerId,
        report: &AgentUpdateReport,
    ) -> Result<(), DomainError> {
        self.repo.record_agent_report(server_id, report).await?;
        if let Some(job) = &report.update_job
            && let Some(job_id) = frame_job_id(job)
        {
            let phase = job.get("phase").and_then(Value::as_str);
            self.repo.touch_job(server_id, job_id, phase).await?;
        }
        Ok(())
    }

    /// Store an `update_state` frame: the last check and what is installed.
    ///
    /// The agent sends this after every check and again on every connect,
    /// so the same check arrives many times. Only a check newer than the
    /// stored one resets the best-known installed versions (to the check's
    /// `installed`, with the helper's own record of the last `mods` run
    /// filling in or — when it is the newer of the two — overriding it).
    /// A repeat must not: a job that succeeded since has already moved
    /// those versions on. A frame without a check changes nothing; the
    /// agent sends `check: null` until a first check has run, never to
    /// retract one.
    pub async fn record_update_state(
        &self,
        server_id: GameServerId,
        frame: &Value,
    ) -> Result<(), DomainError> {
        let Some(check) = opt_json(frame, "check") else {
            return Ok(());
        };
        let checked_at = opt_time(&check, "checked_at");
        let stored = self.state(server_id).await?;
        let is_new = match (checked_at, stored.update_checked_at) {
            (Some(new), Some(old)) => new > old,
            (_, None) => true,
            (None, Some(_)) => false,
        };
        if !is_new {
            return Ok(());
        }

        let recorded = opt_json(frame, "installed");
        let recorded_at = recorded.as_ref().and_then(|r| opt_time(r, "recorded_at"));
        let from_check = check.get("installed").filter(|v| !v.is_null()).cloned();
        // Later entries win where both know a component.
        let record_is_newer = matches!((recorded_at, checked_at), (Some(r), Some(c)) if r > c);
        let sources = if record_is_newer {
            [from_check, recorded]
        } else {
            [recorded, from_check]
        };
        let mut installed: Map<String, Value> = Map::new();
        for source in sources.into_iter().flatten() {
            if let Some(map) = source.as_object() {
                for (k, v) in map {
                    if !v.is_null() {
                        installed.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        // Bookkeeping of the helper's record, not versions.
        installed.remove("recorded_at");
        installed.remove("job_id");
        let installed = (!installed.is_empty()).then_some(Value::Object(installed));

        self.repo
            .record_update_state(server_id, &check, checked_at, installed.as_ref())
            .await
    }

    /// The portal-managed MatchZy overrides become `overrides` (already
    /// validated with `portal_core::matchzy_overrides`), pending delivery
    /// to the agent.
    pub async fn set_matchzy_overrides(
        &self,
        server_id: GameServerId,
        overrides: &Map<String, Value>,
    ) -> Result<(), DomainError> {
        self.repo.set_matchzy_overrides(server_id, overrides).await
    }

    /// The agent accepted `overrides` (`set_matchzy_overrides` answered ok).
    pub async fn mark_overrides_delivered(
        &self,
        server_id: GameServerId,
        overrides: &Map<String, Value>,
    ) -> Result<(), DomainError> {
        self.repo
            .mark_overrides_delivered(server_id, overrides)
            .await
    }

    // ------------------------------------------------------------------
    // Jobs
    // ------------------------------------------------------------------

    /// Create a `queued` job for an admin. `Conflict` when the server
    /// already has one in flight.
    pub async fn create_job(
        &self,
        server_id: GameServerId,
        operation: UpdateOperation,
        requested_by: UserId,
        forced: bool,
    ) -> Result<GameServerUpdateJob, DomainError> {
        if operation == UpdateOperation::Check {
            return Err(DomainError::Internal(
                "a check is not a job the portal creates".into(),
            ));
        }
        self.repo
            .create_job(CreateUpdateJob {
                id: GameServerUpdateJobId::new(),
                server_id,
                operation,
                requested_by,
                forced,
            })
            .await
    }

    /// A job of this server; `None` when the id is unknown or belongs to
    /// another server.
    pub async fn find_job(
        &self,
        server_id: GameServerId,
        id: GameServerUpdateJobId,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        Ok(self
            .repo
            .find_job(id)
            .await?
            .filter(|j| j.server_id == server_id))
    }

    /// The server's job in flight, if any.
    pub async fn active_job(
        &self,
        server_id: GameServerId,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        self.repo.find_active_job(server_id).await
    }

    /// The server's latest jobs, newest first.
    pub async fn recent_jobs(
        &self,
        server_id: GameServerId,
        limit: i64,
    ) -> Result<Vec<GameServerUpdateJob>, DomainError> {
        self.repo
            .list_recent_jobs(server_id, limit.clamp(1, 100))
            .await
    }

    /// The agent refused the job, or it could not be sent: it ends `failed`
    /// with the reason. Returns the job as stored.
    pub async fn fail_job(
        &self,
        id: GameServerUpdateJobId,
        error: &str,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        self.repo
            .finish_job(id, UpdateJobStatus::Failed, error, None)
            .await
    }

    /// Apply an `update_progress` frame. `None` when nothing was written
    /// (unreadable frame, a terminal job, another server's job).
    pub async fn record_progress(
        &self,
        server_id: GameServerId,
        frame: &Value,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        let Some(progress) = parse_progress(frame) else {
            return Ok(None);
        };
        self.repo.apply_progress(server_id, &progress).await
    }

    /// Apply an `update_result` frame. `None` when the frame is unreadable.
    ///
    /// A successful job's `after` becomes the best-known installed
    /// versions, so the admin sees the new versions without a re-check
    /// (unless a check newer than the job has already been stored).
    pub async fn record_result(
        &self,
        server_id: GameServerId,
        frame: &Value,
    ) -> Result<Option<ResultOutcome>, DomainError> {
        let Some(result) = parse_result(frame) else {
            return Ok(None);
        };
        let outcome = self.repo.apply_result(server_id, &result).await?;
        if matches!(outcome, ResultOutcome::Stored { .. })
            && result.status == UpdateJobStatus::Succeeded
            && result.operation != UpdateOperation::Check
            && let Some(after) = &result.after_versions
        {
            self.repo
                .overlay_installed_versions(server_id, after, result.finished_at)
                .await?;
        }
        Ok(Some(outcome))
    }

    // ------------------------------------------------------------------
    // Lost jobs
    // ------------------------------------------------------------------

    /// Jobs in flight that have not progressed for
    /// [`UPDATE_JOB_STALL_MINUTES`].
    pub async fn stalled_jobs(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<GameServerUpdateJob>, DomainError> {
        self.repo
            .list_stalled_jobs(
                now - chrono::Duration::minutes(UPDATE_JOB_STALL_MINUTES),
                50,
            )
            .await
    }

    /// Declare a stalled job lost — unless it has moved since it was read
    /// as stalled (`job.updated_at`): progress, a heartbeat naming it or a
    /// result that landed meanwhile wins over the guess. Returns the job
    /// when this call ended it, `None` when nothing changed.
    pub async fn mark_lost(
        &self,
        job: &GameServerUpdateJob,
        reason: &str,
    ) -> Result<Option<GameServerUpdateJob>, DomainError> {
        self.repo
            .finish_job(job.id, UpdateJobStatus::Lost, reason, Some(job.updated_at))
            .await
    }

    /// Settle a stalled job from the agent's `update_status` answer (the
    /// helper's state file, or `null`).
    pub async fn reconcile_stalled(
        &self,
        job: &GameServerUpdateJob,
        agent_state: Option<&Value>,
    ) -> Result<Reconciled, DomainError> {
        let state = agent_state.filter(|s| frame_job_id(s) == Some(job.id));
        let Some(state) = state else {
            self.mark_lost(
                job,
                "no progress for 20 minutes and the host has no record of this job",
            )
            .await?;
            return Ok(Reconciled::Lost);
        };
        if state.get("status").and_then(Value::as_str) == Some("running") {
            let phase = state.get("phase").and_then(Value::as_str);
            self.repo.touch_job(job.server_id, job.id, phase).await?;
            return Ok(Reconciled::StillRunning);
        }
        match self.record_result(job.server_id, state).await? {
            Some(ResultOutcome::Stored { .. } | ResultOutcome::Duplicate) => {
                Ok(Reconciled::Finished)
            }
            Some(ResultOutcome::Foreign) | None => {
                self.mark_lost(
                    job,
                    "no progress for 20 minutes and the host's record of this job is unreadable",
                )
                .await?;
                Ok(Reconciled::Lost)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const JOB: &str = "0193a5b0-7c1e-7000-8000-000000000001";

    #[test]
    fn progress_frames_parse() {
        let p = parse_progress(&json!({
            "type": "update_progress", "job_id": JOB, "operation": "mods",
            "phase": "mods-update", "status": "running", "trigger": "portal",
            "log_tail": "line\n",
        }))
        .unwrap();
        assert_eq!(p.job_id.to_string(), JOB);
        assert_eq!(p.operation, UpdateOperation::Mods);
        assert_eq!(p.trigger, UpdateJobTrigger::Portal);
        assert_eq!(p.phase.as_deref(), Some("mods-update"));
        assert_eq!(p.log_tail.as_deref(), Some("line\n"));
    }

    #[test]
    fn progress_without_a_job_or_for_a_check_is_dropped() {
        assert!(parse_progress(&json!({"operation": "mods"})).is_none());
        assert!(parse_progress(&json!({"job_id": "nope", "operation": "mods"})).is_none());
        assert!(parse_progress(&json!({"job_id": JOB, "operation": "frobnicate"})).is_none());
        assert!(parse_progress(&json!({"job_id": JOB, "operation": "check"})).is_none());
    }

    #[test]
    fn a_frame_without_a_trigger_reads_as_local() {
        let p = parse_progress(&json!({"job_id": JOB, "operation": "game"})).unwrap();
        assert_eq!(p.trigger, UpdateJobTrigger::Local);
    }

    #[test]
    fn result_frames_parse_and_nulls_read_as_absent() {
        let r = parse_result(&json!({
            "type": "update_result", "job_id": JOB, "operation": "mods",
            "trigger": "portal", "status": "rolled_back",
            "phase": "rollback", "failed_phase": "verify",
            "started_at": "2026-11-21T04:00:00Z", "finished_at": "2026-11-21T04:03:10Z",
            "before": {"matchzy": "0.9.2"}, "after": {"matchzy": "0.9.2"},
            "latest": null, "load": {"matchzy": true}, "config_report": null,
            "restarted": true, "error": "MMS: Fatal error", "log_tail": "x", "pid": 1,
        }))
        .unwrap();
        assert_eq!(r.status, UpdateJobStatus::RolledBack);
        assert_eq!(r.failed_phase.as_deref(), Some("verify"));
        assert!(r.latest_versions.is_none());
        assert!(r.config_report.is_none());
        assert_eq!(r.restarted, Some(true));
        assert!(!r.agent_synthesized);
        assert_eq!(
            r.finished_at.unwrap().to_rfc3339(),
            "2026-11-21T04:03:10+00:00"
        );
    }

    #[test]
    fn a_result_must_be_terminal() {
        for status in ["running", "queued", "lost", "weird"] {
            assert!(
                parse_result(&json!({"job_id": JOB, "operation": "mods", "status": status}))
                    .is_none(),
                "{status}"
            );
        }
    }

    #[test]
    fn a_synthesized_result_is_flagged_and_logs_are_bounded() {
        let r = parse_result(&json!({
            "job_id": JOB, "operation": "check", "status": "failed",
            "agent_synthesized": true, "log_tail": "x".repeat(LOG_TAIL_MAX_BYTES + 100),
        }))
        .unwrap();
        assert!(r.agent_synthesized);
        assert_eq!(r.operation, UpdateOperation::Check);
        assert_eq!(r.log_tail.unwrap().len(), LOG_TAIL_MAX_BYTES);
    }
}
