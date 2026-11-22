//! Game-server update entities (docs/gameserver-lgsm-updates-design.md).
//!
//! The update itself runs in the agent's root helper on the game host. The
//! portal keeps one [`GameServerUpdateJob`] per job id — whoever started it
//! — and, per server, what the agent last said about the host's update
//! machinery ([`GameServerUpdateState`]).

use chrono::{DateTime, Utc};
use portal_core::ids::{GameServerId, GameServerUpdateJobId, UserId};
use portal_core::types::{UpdateJobStatus, UpdateJobTrigger, UpdateOperation};
use serde_json::{Map, Value};

/// The capability an agent advertises when the host has the LinuxGSM
/// update helper installed and configured.
pub const CAPABILITY_LGSM_UPDATE: &str = "lgsm_update";

/// What a job keeps of its log: the newest 8 KiB.
pub const LOG_TAIL_MAX_BYTES: usize = 8 * 1024;

/// One update job on a game server.
#[derive(Debug, Clone)]
pub struct GameServerUpdateJob {
    pub id: GameServerUpdateJobId,
    pub server_id: GameServerId,
    pub operation: UpdateOperation,
    pub status: UpdateJobStatus,
    pub trigger: UpdateJobTrigger,
    /// The admin who started it; `None` for jobs the portal did not start.
    pub requested_by: Option<UserId>,
    /// Started with `force` over a players/reservation gate.
    pub forced: bool,
    /// Where the job is, or where it stopped.
    pub phase: Option<String>,
    /// The phase that broke (for `rolled_back`: what triggered the rollback).
    pub failed_phase: Option<String>,
    pub before_versions: Option<Value>,
    pub after_versions: Option<Value>,
    pub latest_versions: Option<Value>,
    /// What the console showed after the last (re)start in this job.
    pub load: Option<Value>,
    /// What the update did to operator settings.
    pub config_report: Option<Value>,
    pub restarted: Option<bool>,
    pub error: Option<String>,
    pub log_tail: Option<String>,
    /// The agent made the result up because the helper never produced one.
    pub agent_synthesized: bool,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl GameServerUpdateJob {
    /// Whether an incoming result may replace what is stored.
    ///
    /// Delivery is at-least-once, so a repeat of a stored result is dropped.
    /// A result does replace: a job still in flight; a `lost` verdict (the
    /// portal's own guess); and a result the agent synthesized, once the
    /// helper's real one turns up.
    #[must_use]
    pub fn accepts_result(&self, incoming_synthesized: bool) -> bool {
        !self.status.is_terminal()
            || self.status == UpdateJobStatus::Lost
            || (self.agent_synthesized && !incoming_synthesized)
    }

    /// Whether this job keeps its server out of allocation right now.
    #[must_use]
    pub fn blocks_allocation(&self) -> bool {
        !self.status.is_terminal() && self.operation.restarts_server()
    }
}

/// What the agent last reported about a server's update machinery, plus
/// the portal-managed MatchZy overrides.
#[derive(Debug, Clone)]
pub struct GameServerUpdateState {
    pub server_id: GameServerId,
    /// From the last heartbeat; empty for agents older than 0.3.0.
    pub capabilities: Vec<String>,
    /// The `check` object of the last `update_state` frame.
    pub update_check: Option<Value>,
    pub update_checked_at: Option<DateTime<Utc>>,
    /// Best-known installed versions.
    pub installed_versions: Option<Value>,
    pub demo_upload_wired: Option<bool>,
    pub overrides_applied: Option<bool>,
    pub reload_pending: bool,
    /// The operation in flight per the last heartbeat (checks included).
    pub agent_update_job: Option<Value>,
    pub matchzy_overrides: Map<String, Value>,
    /// The stored overrides have not been accepted by the agent yet.
    pub matchzy_overrides_pending: bool,
}

impl GameServerUpdateState {
    /// Whether the agent can run updates on this host.
    #[must_use]
    pub fn can_update(&self) -> bool {
        self.capabilities
            .iter()
            .any(|c| c == CAPABILITY_LGSM_UPDATE)
    }
}

/// The update-related fields of one agent heartbeat (agent 0.3.0+; an older
/// agent sends none, which reads as the default).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentUpdateReport {
    pub capabilities: Vec<String>,
    pub demo_upload_wired: Option<bool>,
    pub overrides_applied: Option<bool>,
    pub reload_pending: bool,
    /// `{job_id, operation, phase, status}` while an operation runs.
    pub update_job: Option<Value>,
}

/// An `update_progress` frame (also the shape of a running `update_status`).
#[derive(Debug, Clone)]
pub struct UpdateProgress {
    pub job_id: GameServerUpdateJobId,
    pub operation: UpdateOperation,
    pub trigger: UpdateJobTrigger,
    pub phase: Option<String>,
    pub log_tail: Option<String>,
}

/// An `update_result` frame: a job's terminal state.
#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub job_id: GameServerUpdateJobId,
    pub operation: UpdateOperation,
    pub trigger: UpdateJobTrigger,
    /// `succeeded`, `failed` or `rolled_back`.
    pub status: UpdateJobStatus,
    pub phase: Option<String>,
    pub failed_phase: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub before_versions: Option<Value>,
    pub after_versions: Option<Value>,
    pub latest_versions: Option<Value>,
    pub load: Option<Value>,
    pub config_report: Option<Value>,
    pub restarted: Option<bool>,
    pub error: Option<String>,
    pub log_tail: Option<String>,
    pub agent_synthesized: bool,
}

/// What storing a result did.
#[derive(Debug, Clone)]
pub enum ResultOutcome {
    /// The result is now the job's stored state.
    Stored {
        job: Box<GameServerUpdateJob>,
        /// The job had no agent-reported outcome before (it was in flight,
        /// `lost`, or unknown to the portal) — i.e. this is the finish, not
        /// a real result replacing a synthesized one.
        first_outcome: bool,
    },
    /// The stored state already covers it (a replay); nothing changed.
    Duplicate,
    /// The job id belongs to another server; refused.
    Foreign,
}

/// Keep the newest `max` bytes of `text`, cut on a character boundary.
#[must_use]
pub fn bound_log_tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(status: UpdateJobStatus, synthesized: bool) -> GameServerUpdateJob {
        let now = Utc::now();
        GameServerUpdateJob {
            id: GameServerUpdateJobId::new(),
            server_id: GameServerId::new(),
            operation: UpdateOperation::Mods,
            status,
            trigger: UpdateJobTrigger::Portal,
            requested_by: None,
            forced: false,
            phase: None,
            failed_phase: None,
            before_versions: None,
            after_versions: None,
            latest_versions: None,
            load: None,
            config_report: None,
            restarted: None,
            error: None,
            log_tail: None,
            agent_synthesized: synthesized,
            created_at: now,
            started_at: None,
            finished_at: None,
            updated_at: now,
        }
    }

    #[test]
    fn a_result_replaces_a_job_in_flight_or_lost() {
        for status in [
            UpdateJobStatus::Queued,
            UpdateJobStatus::Running,
            UpdateJobStatus::Lost,
        ] {
            assert!(job(status, false).accepts_result(false), "{status}");
            assert!(job(status, false).accepts_result(true), "{status}");
        }
    }

    #[test]
    fn a_replayed_result_is_dropped() {
        for status in [
            UpdateJobStatus::Succeeded,
            UpdateJobStatus::Failed,
            UpdateJobStatus::RolledBack,
        ] {
            assert!(!job(status, false).accepts_result(false), "{status}");
            assert!(!job(status, false).accepts_result(true), "{status}");
        }
    }

    #[test]
    fn a_real_result_supersedes_a_synthesized_one_but_not_the_reverse() {
        let synthesized = job(UpdateJobStatus::Failed, true);
        assert!(synthesized.accepts_result(false));
        assert!(!synthesized.accepts_result(true));
    }

    #[test]
    fn only_restart_capable_jobs_in_flight_block_allocation() {
        let mut j = job(UpdateJobStatus::Running, false);
        assert!(j.blocks_allocation());
        j.operation = UpdateOperation::Lgsm;
        assert!(!j.blocks_allocation());
        j.operation = UpdateOperation::RepairConfig;
        assert!(!j.blocks_allocation());
        j.operation = UpdateOperation::All;
        j.status = UpdateJobStatus::Succeeded;
        assert!(!j.blocks_allocation());
    }

    #[test]
    fn log_tail_keeps_the_newest_bytes_on_a_char_boundary() {
        assert_eq!(bound_log_tail("abc", 8), "abc");
        assert_eq!(bound_log_tail("abcdef", 3), "def");
        // "é" is two bytes: a cut landing inside it moves forward.
        assert_eq!(bound_log_tail("aéb", 2), "b");
        assert_eq!(bound_log_tail("aéb", 3), "éb");
    }
}
