//! Game-server updates: the agent-channel half
//! (docs/gameserver-lgsm-updates-design.md §6–§7; wire contract in the
//! portal-server-agent README, "Protocol").
//!
//! Everything here is driven by what an agent says or fails to say: its
//! heartbeat's update fields, the pushed `update_progress` /
//! `update_result` / `update_state` frames, the overrides push on connect,
//! and the lifecycle pass that settles jobs gone silent. The admin
//! endpoints live in `handlers::game_servers::updates`.

use crate::websocket::agent_manager::{AgentCommand, AgentConnectionManager, SendError};
use chrono::Utc;
use dashmap::DashMap;
use futures_util::StreamExt;
use portal_core::ids::GameServerId;
use portal_core::types::UpdateJobStatus;
use portal_db::{PgAdminServerCommandRepository, PgGameServerUpdateRepository};
use portal_domain::entities::{AgentUpdateReport, GameServerUpdateJob, ResultOutcome};
use portal_domain::repositories::{AdminServerCommandRepository, CreateAdminServerCommand};
use portal_domain::services::game_server::{GameServerUpdateService, Reconciled, frame_job_id};
use serde_json::Value;
use std::sync::Arc;

/// The update service over its Postgres repository.
pub type AppGameServerUpdateService = GameServerUpdateService<PgGameServerUpdateRepository>;

/// `update_status` questions in flight at once during a lost-job pass.
const PASS_CONCURRENCY: usize = 8;

/// What the audit row keeps of an agent's answer.
const AUDIT_OUTPUT_MAX: usize = 4 * 1024;

/// Per-connection memory for the overrides push (see
/// [`UpdateChannel::on_heartbeat`]).
#[derive(Debug, Default)]
pub struct ConnectionUpdateState {
    /// Heartbeats seen on this connection.
    heartbeats: u32,
    /// The overrides were pushed (or found applied) on this connection.
    overrides_settled: bool,
}

/// What a `set_matchzy_overrides` push came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverridesPush {
    /// The agent wrote the file and started `repair-config` to apply it.
    Applying { job_id: Option<String> },
    /// The agent wrote the file; another job is running, so the next
    /// `mods`, `all` or `repair-config` applies it.
    Written { reason: String },
    /// The agent did not take it (not connected, not capable, refused, no
    /// answer); it is pushed again on its next connect.
    Pending { reason: String },
}

/// Counters from one lost-job pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct UpdateJobPassSummary {
    /// Stalled jobs the agent accounted for (still running, or finished).
    pub reconciled: u32,
    /// Stalled jobs declared lost.
    pub lost: u32,
    /// Errors (each logged; the pass continues).
    pub errors: u32,
}

/// The pieces the update feature needs to talk to agents and record what
/// they say. Shared by the admin handlers, the agent WebSocket and the
/// lifecycle loop.
#[derive(Clone)]
pub struct UpdateChannel {
    /// Jobs and per-server update state.
    pub service: AppGameServerUpdateService,
    /// Connected agents.
    pub agent_manager: Arc<AgentConnectionManager>,
    /// The admin audit trail (`admin_server_commands`).
    pub audit: Arc<PgAdminServerCommandRepository>,
    /// One lock per server around overrides pushes (see
    /// [`UpdateChannel::push_overrides`]).
    pub push_locks: Arc<DashMap<GameServerId, Arc<tokio::sync::Mutex<()>>>>,
}

/// An agent's `output` for the update commands is a JSON document encoded
/// as a string.
#[must_use]
pub fn parse_output(output: Option<&str>) -> Option<Value> {
    serde_json::from_str(output?).ok()
}

impl UpdateChannel {
    // ------------------------------------------------------------------
    // Inbound frames
    // ------------------------------------------------------------------

    /// Store a heartbeat's update fields and, once per connection, push
    /// the portal's MatchZy overrides when the host does not have them.
    ///
    /// The push is decided here rather than on connect because only the
    /// heartbeat says whether the agent can take it and whether it is
    /// needed. `overrides_applied: false` pushes at once. `null` is
    /// "unknown" on the first heartbeat after a connect (the agent is
    /// still verifying) and "never set on this host" from the second on,
    /// so it pushes from the second. Overrides saved while the agent was
    /// away (`matchzy_overrides_pending`, which also covers clearing them)
    /// push at once whatever the heartbeat says.
    pub async fn on_heartbeat(
        &self,
        server_id: GameServerId,
        report: &AgentUpdateReport,
        conn: &mut ConnectionUpdateState,
    ) {
        if let Err(e) = self.service.record_heartbeat(server_id, report).await {
            tracing::warn!(server_id = %server_id, error = %e,
                "heartbeat update fields not stored");
        }
        conn.heartbeats = conn.heartbeats.saturating_add(1);
        if conn.overrides_settled {
            return;
        }
        let state = match self.service.state(server_id).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(server_id = %server_id, error = %e, "update state not readable");
                return;
            }
        };
        if !state.can_update() {
            return;
        }
        let wanted = if state.matchzy_overrides_pending {
            true
        } else if state.matchzy_overrides.is_empty() {
            conn.overrides_settled = true;
            false
        } else {
            match report.overrides_applied {
                Some(true) => {
                    conn.overrides_settled = true;
                    false
                }
                Some(false) => true,
                None => conn.heartbeats >= 2,
            }
        };
        if !wanted {
            return;
        }
        conn.overrides_settled = true;
        // Never awaited here: the answer comes back through the socket
        // loop that is running this very function.
        let channel = self.clone();
        tokio::spawn(async move {
            let push = channel.push_overrides(server_id).await;
            tracing::info!(server_id = %server_id, outcome = ?push,
                "pushed MatchZy overrides on connect");
        });
    }

    /// `update_progress`: a running job moved on.
    pub async fn on_progress(&self, server_id: GameServerId, frame: &Value) {
        match self.service.record_progress(server_id, frame).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                tracing::debug!(server_id = %server_id, "update_progress frame not applied");
            }
            Err(e) => {
                tracing::warn!(server_id = %server_id, error = %e, "update_progress not stored");
            }
        }
    }

    /// `update_result`: store it, then acknowledge it so the agent stops
    /// replaying it. A result that could not be stored (database error) is
    /// not acknowledged — the agent sends it again on its next connect.
    pub async fn on_result(&self, server_id: GameServerId, frame: &Value) {
        let Some(job_id) = frame_job_id(frame) else {
            tracing::debug!(server_id = %server_id, "update_result without a usable job_id");
            return;
        };
        match self.service.record_result(server_id, frame).await {
            Ok(Some(ResultOutcome::Stored { job, first_outcome })) => {
                tracing::info!(server_id = %server_id, job_id = %job.id,
                    operation = %job.operation, status = %job.status, trigger = %job.trigger,
                    "game-server update job finished");
                if first_outcome {
                    self.audit_finish(&job).await;
                }
            }
            Ok(Some(ResultOutcome::Duplicate)) => {}
            Ok(Some(ResultOutcome::Foreign)) => {
                tracing::warn!(server_id = %server_id, job_id = %job_id,
                    "update_result names another server's job; ignored");
            }
            // Unreadable for good (an operation or status this portal does
            // not know): acknowledging is the only way to stop the replay.
            Ok(None) => {
                tracing::warn!(server_id = %server_id, job_id = %job_id,
                    "update_result not understood; acknowledged without storing");
            }
            Err(e) => {
                tracing::warn!(server_id = %server_id, job_id = %job_id, error = %e,
                    "update_result not stored; left unacknowledged for replay");
                return;
            }
        }
        // Best-effort, and never awaited here (see `on_heartbeat`).
        let manager = Arc::clone(&self.agent_manager);
        tokio::spawn(async move {
            let ack = AgentCommand::UpdateAck {
                job_id: job_id.to_string(),
            };
            match manager.try_send_command(server_id, ack).await {
                Ok(outcome) if outcome.ok => {}
                Ok(outcome) => {
                    tracing::debug!(server_id = %server_id, job_id = %job_id,
                        error = ?outcome.error, "update_ack refused");
                }
                Err(e) => {
                    tracing::debug!(server_id = %server_id, job_id = %job_id, error = %e,
                        "update_ack not delivered; the agent will replay the result");
                }
            }
        });
    }

    /// `update_state`: the last check and what is installed.
    pub async fn on_state(&self, server_id: GameServerId, frame: &Value) {
        if let Err(e) = self.service.record_update_state(server_id, frame).await {
            tracing::warn!(server_id = %server_id, error = %e, "update_state not stored");
        }
    }

    // ------------------------------------------------------------------
    // Overrides
    // ------------------------------------------------------------------

    /// Send the server's stored overrides to its agent and record whether
    /// it took them.
    ///
    /// Pushes to one server are serialized, and each sends what is stored
    /// when it runs (read under the lock), not what its caller saw: two
    /// PUTs racing, or a PUT racing the push on connect, cannot deliver an
    /// older set after a newer one. The last push to run always carries
    /// the newest stored set, and `pending` is cleared only for the set
    /// that was sent (`mark_overrides_delivered` compares), so the host and
    /// the database cannot disagree with `pending` false.
    pub async fn push_overrides(&self, server_id: GameServerId) -> OverridesPush {
        let lock = Arc::clone(
            self.push_locks
                .entry(server_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .value(),
        );
        let _serialized = lock.lock().await;
        let state = match self.service.state(server_id).await {
            Ok(state) => state,
            Err(e) => {
                return OverridesPush::Pending {
                    reason: format!("the stored overrides could not be read: {e}"),
                };
            }
        };
        let overrides = state.matchzy_overrides.clone();
        if !self.agent_manager.is_connected(server_id) {
            return OverridesPush::Pending {
                reason: SendError::NotConnected.to_string(),
            };
        }
        if !state.can_update() {
            return OverridesPush::Pending {
                reason: "this server's agent has no LinuxGSM update helper (no lgsm_update \
                         capability)"
                    .into(),
            };
        }
        let command = AgentCommand::SetMatchzyOverrides {
            overrides: overrides.clone(),
        };
        let outcome = match self
            .agent_manager
            .try_send_command(server_id, command)
            .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                return OverridesPush::Pending {
                    reason: e.to_string(),
                };
            }
        };
        if !outcome.ok {
            return OverridesPush::Pending {
                reason: outcome
                    .error
                    .unwrap_or_else(|| "the agent refused the overrides".into()),
            };
        }
        if let Err(e) = self
            .service
            .mark_overrides_delivered(server_id, &overrides)
            .await
        {
            tracing::warn!(server_id = %server_id, error = %e,
                "overrides delivered but still marked pending");
        }
        let answer = parse_output(outcome.output.as_deref()).unwrap_or(Value::Null);
        if answer.get("started").and_then(Value::as_bool) == Some(false) {
            OverridesPush::Written {
                reason: answer
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("another update job is running")
                    .to_string(),
            }
        } else {
            OverridesPush::Applying {
                job_id: answer
                    .get("job_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }
        }
    }

    // ------------------------------------------------------------------
    // Audit
    // ------------------------------------------------------------------

    /// One `admin_server_commands` row. A failed insert is logged, not
    /// surfaced: the command has already gone out.
    pub async fn audit(&self, row: CreateAdminServerCommand) {
        let server_id = row.server_id;
        let row = CreateAdminServerCommand {
            output: row
                .output
                .map(|o| portal_domain::entities::bound_log_tail(&o, AUDIT_OUTPUT_MAX)),
            ..row
        };
        if let Err(e) = self.audit.insert(row).await {
            tracing::error!(server_id = %server_id, error = %e, "update audit row not written");
        }
    }

    /// The finish of a job an admin started: who, what, how it ended.
    /// Jobs the portal did not start have no admin to attribute and are
    /// recorded by their job row alone.
    pub async fn audit_finish(&self, job: &GameServerUpdateJob) {
        let Some(admin_user_id) = job.requested_by else {
            return;
        };
        let mut output = job.status.to_string();
        if let Some(error) = &job.error {
            output.push_str(": ");
            output.push_str(error);
        }
        self.audit(CreateAdminServerCommand {
            server_id: job.server_id,
            reservation_id: None,
            admin_user_id,
            kind: format!("update_result:{}", job.operation),
            command: format!("update_result {} {}", job.operation, job.id),
            output: Some(output),
            ok: job.status == UpdateJobStatus::Succeeded,
            force: job.forced,
        })
        .await;
    }

    // ------------------------------------------------------------------
    // Lost jobs
    // ------------------------------------------------------------------

    /// One pass over jobs in flight that have not progressed for 20
    /// minutes: ask the agent (`update_status`) and settle from its answer;
    /// without an answer the job is `lost`. Either way the server is back
    /// in allocation the moment the job leaves `queued`/`running`.
    ///
    /// Bounded: at most `UPDATE_JOB_PASS_MAX` jobs (the oldest; the rest
    /// wait for the next pass), asked [`PASS_CONCURRENCY`] at a time, so a
    /// pass of unresponsive agents costs about two command timeouts rather
    /// than one per job. An audit row is written only for a job whose
    /// outcome this pass stored.
    pub async fn run_lost_job_pass(&self) -> UpdateJobPassSummary {
        let mut summary = UpdateJobPassSummary::default();
        let stalled = match self.service.stalled_jobs(Utc::now()).await {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!(error = %e, "lifecycle: stalled update job scan failed");
                summary.errors += 1;
                return summary;
            }
        };
        let mut settled = futures_util::stream::iter(stalled)
            .map(|job| async move {
                let outcome = self.settle_stalled(&job).await;
                (job, outcome)
            })
            .buffer_unordered(PASS_CONCURRENCY);
        while let Some((job, outcome)) = settled.next().await {
            match outcome {
                Ok(Reconciled::Lost(Some(lost))) => {
                    summary.lost += 1;
                    tracing::warn!(server_id = %job.server_id, job_id = %job.id,
                        operation = %job.operation, "game-server update job lost");
                    self.audit_finish(&lost).await;
                }
                Ok(Reconciled::Finished(Some(done))) => {
                    summary.reconciled += 1;
                    self.audit_finish(&done).await;
                }
                // Nothing changed: it moved on or ended since it was read
                // as stalled, or its outcome was already stored.
                Ok(Reconciled::Lost(None) | Reconciled::Finished(None)) => {
                    summary.reconciled += 1;
                }
                Ok(Reconciled::StillRunning) => summary.reconciled += 1,
                Err(e) => {
                    tracing::error!(server_id = %job.server_id, job_id = %job.id, error = %e,
                        "lifecycle: stalled update job not settled");
                    summary.errors += 1;
                }
            }
        }
        summary
    }

    async fn settle_stalled(
        &self,
        job: &GameServerUpdateJob,
    ) -> Result<Reconciled, portal_core::errors::DomainError> {
        let answer = if self.agent_manager.is_connected(job.server_id) {
            self.agent_manager
                .try_send_command(job.server_id, AgentCommand::UpdateStatus)
                .await
                .ok()
                .filter(|o| o.ok)
        } else {
            None
        };
        let Some(answer) = answer else {
            let lost = self
                .service
                .mark_lost(
                    job,
                    "no progress for 20 minutes and the agent could not be asked",
                )
                .await?;
            return Ok(Reconciled::Lost(lost.map(Box::new)));
        };
        let state = parse_output(answer.output.as_deref()).filter(|v| !v.is_null());
        self.service.reconcile_stalled(job, state.as_ref()).await
    }
}
