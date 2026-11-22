//! Game-server updates through LinuxGSM, and the portal-managed MatchZy
//! overrides (docs/gameserver-lgsm-updates-design.md §7).
//!
//! The update itself runs in the agent's root helper on the game host.
//! These endpoints check `admin.servers.manage`, apply the safety gates,
//! own the job row, and write one `admin_server_commands` row per command
//! sent (the same audit trail the console uses). The agent's side of the
//! conversation is in `crate::game_server_updates`.

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::game_server_updates::{OverridesPush, parse_output};
use crate::state::AppState;
use crate::websocket::agent_manager::{AgentCommand, SendError};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Duration, Utc};
use portal_core::ids::{GameServerId, GameServerUpdateJobId, UserId};
use portal_core::matchzy_overrides::validate_matchzy_overrides;
use portal_core::permissions;
use portal_core::types::{
    GameServerStatus, ReservationKind, ReservationStatus, UpdateJobStatus, UpdateJobTrigger,
    UpdateOperation,
};
use portal_domain::entities::{
    GameServer, GameServerUpdateJob, GameServerUpdateState, ServerBooking, ServerReservation,
};
use portal_domain::repositories::{CreateAdminServerCommand, ServerReservationRepository};
use portal_domain::services::game_server::UPDATE_JOB_STALL_MINUTES;
use portal_plugins::games::cs2::console;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Jobs listed by the overview.
const RECENT_JOBS: i64 = 10;
/// A booking starting this soon counts as "imminent" for the restart gate.
const IMMINENT_MINUTES: i64 = 30;
/// A heartbeat `status` older than this says nothing about who is
/// connected now (heartbeats arrive every 30 s).
const STATUS_FRESH_SECS: i64 = 120;

fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

fn parse_server_id(id: &str) -> Result<GameServerId, ApiError> {
    id.parse()
        .map_err(|_| ApiError::bad_request(format!("invalid game server id: {id}")))
}

fn bad_gateway(detail: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_GATEWAY, "Bad Gateway", detail)
}

// =============================================================================
// DTOs
// =============================================================================

/// Versions of the five things the helper tracks; `null` = unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct UpdateVersions {
    /// LinuxGSM release, e.g. `v26.6.0`.
    pub lgsm: Option<String>,
    /// CS2 Steam build id.
    pub cs2_build: Option<String>,
    /// Metamod version as the helper reports it (e.g. `2.0.0-git1491`);
    /// known only after a `mods`/`all` run through the helper.
    pub metamod: Option<String>,
    /// CounterStrikeSharp release.
    pub cssharp: Option<String>,
    /// MatchZy release.
    pub matchzy: Option<String>,
}

/// Whether an update is available, per component; `null` = could not be
/// compared (a version unknown, or the upstream lookup failed).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct UpdateAvailability {
    pub lgsm: Option<bool>,
    /// The CS2 build (`cs2_build`).
    pub game: Option<bool>,
    pub metamod: Option<bool>,
    pub cssharp: Option<bool>,
    pub matchzy: Option<bool>,
    /// Any of the three plugins.
    pub mods: Option<bool>,
}

/// What the console showed about the plugin stack after a (re)start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct UpdateLoadHealth {
    pub metamod: Option<bool>,
    pub cssharp: Option<bool>,
    pub matchzy: Option<bool>,
    /// The `MMS: Fatal error` line, if there was one.
    pub error: Option<String>,
}

/// The last `check` the host ran.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateCheckInfo {
    pub checked_at: Option<DateTime<Utc>>,
    /// Latest upstream versions as of the check.
    pub latest: UpdateVersions,
    /// Recomputed against what is installed NOW, so a component updated
    /// since the check no longer shows as outdated.
    pub update_available: UpdateAvailability,
    /// The demo-upload block was in the MatchZy config at the check.
    pub demo_upload_block_present: Option<bool>,
}

/// The operation the agent's last heartbeat named as in flight — checks
/// included, which have no job row.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateAgentJob {
    pub job_id: Option<String>,
    pub operation: Option<String>,
    pub phase: Option<String>,
    pub status: Option<String>,
}

/// One cvar an update changed in MatchZy's config.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateCvarChange {
    pub name: String,
    /// `[redacted]` for portal-owned or secret settings.
    pub before: Option<String>,
    pub after: Option<String>,
}

/// What an update did to operator settings in `cfg/MatchZy` (the managed
/// blocks excluded).
#[derive(Debug, Default, Serialize, ToSchema)]
pub struct UpdateConfigReport {
    pub cvars_changed: Vec<UpdateCvarChange>,
    pub cvars_removed: Vec<String>,
    pub cvars_added: Vec<String>,
    pub files_changed: Vec<String>,
}

/// An update job without its bulky parts.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateJobSummary {
    pub id: String,
    pub server_id: String,
    pub operation: UpdateOperation,
    pub status: UpdateJobStatus,
    pub trigger: UpdateJobTrigger,
    /// The admin who started it; null for jobs the portal did not start.
    pub requested_by: Option<String>,
    pub requested_by_username: Option<String>,
    /// Started with `force` over the players/reservation gate.
    pub forced: bool,
    /// Where the job is, or where it stopped (`done` on success).
    pub phase: Option<String>,
    /// The phase that broke; for `rolled_back`, what triggered the rollback.
    pub failed_phase: Option<String>,
    /// One line; for `rolled_back` the reason for the rollback.
    pub error: Option<String>,
    /// Whether CS2 was restarted; null while unknown.
    pub restarted: Option<bool>,
    /// The agent made this result up because the helper never reported.
    pub agent_synthesized: bool,
    /// Whether the server is out of match allocation because of this job.
    pub blocks_allocation: bool,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Last progress; a job in flight for 20 minutes without any is
    /// declared `lost`.
    pub updated_at: DateTime<Utc>,
}

/// A whole update job.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateJobResponse {
    #[serde(flatten)]
    pub summary: UpdateJobSummary,
    pub before: Option<UpdateVersions>,
    pub after: Option<UpdateVersions>,
    pub load: Option<UpdateLoadHealth>,
    /// Null for jobs that do not touch the config and for `rolled_back`.
    pub config_report: Option<UpdateConfigReport>,
    /// The newest 8 KiB of the job's log.
    pub log_tail: Option<String>,
}

/// The live reservation a restart would interrupt.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateSafetyReservation {
    pub id: String,
    pub match_id: String,
    pub kind: ReservationKind,
    pub status: ReservationStatus,
}

/// A booking that is active or starts within 30 minutes.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateSafetyBooking {
    pub id: String,
    pub reason: Option<String>,
    pub tournament_id: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

/// What a restart would interrupt right now.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateSafety {
    /// Humans connected at the last heartbeat; null when unknown (agent
    /// away, or it reports no `status`).
    pub humans_connected: Option<u32>,
    /// Their names, bots excluded.
    pub players: Vec<String>,
    pub reservation: Option<UpdateSafetyReservation>,
    pub bookings: Vec<UpdateSafetyBooking>,
    /// Whether `game`, `mods` and `all` need `force: true` right now. The
    /// gate fails closed: unknowns (player count, match state, a failed
    /// reservation or booking lookup) count as reasons too.
    pub restart_needs_force: bool,
    /// Why, one line each — the same lines the 409 names.
    pub reasons: Vec<String>,
}

/// Everything the Updates tab shows at once.
#[derive(Debug, Serialize, ToSchema)]
pub struct GameServerUpdatesResponse {
    pub server_id: String,
    pub agent_connected: bool,
    pub agent_version: Option<String>,
    /// Whether the agent can run updates on this host (it advertises
    /// `lgsm_update`). False for agents older than 0.3.0 and hosts without
    /// the helper: hide the feature.
    pub supported: bool,
    pub capabilities: Vec<String>,
    /// Best-known installed versions: the last check, moved on by any job
    /// that succeeded since.
    pub installed: Option<UpdateVersions>,
    /// Null until the host has run a first check.
    pub check: Option<UpdateCheckInfo>,
    /// The newest plugin-load reading, from the last check or a later job.
    pub load: Option<UpdateLoadHealth>,
    /// Whether the demo-upload block is intact in the MatchZy config file;
    /// null = unknown.
    pub demo_upload_wired: Option<bool>,
    /// Whether the portal-managed overrides are in that file; null = none
    /// set on the host, or unknown.
    pub overrides_applied: Option<bool>,
    /// A repair changed the config and the running server has not re-read
    /// it yet (the agent waits for no match to be loaded).
    pub reload_pending: bool,
    pub agent_job: Option<UpdateAgentJob>,
    /// The job in flight, if any.
    pub active_job: Option<UpdateJobSummary>,
    /// The last ten jobs, newest first (the active one included).
    pub recent_jobs: Vec<UpdateJobSummary>,
    pub safety: UpdateSafety,
}

/// What an admin may start. `repair-config` has its own endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartUpdateOperation {
    /// LinuxGSM itself; does not restart CS2.
    Lgsm,
    /// The CS2 build; restarts CS2 if anything changed.
    Game,
    /// Metamod, CounterStrikeSharp and MatchZy; restarts CS2.
    Mods,
    /// All three, one restart.
    All,
}

impl From<StartUpdateOperation> for UpdateOperation {
    fn from(op: StartUpdateOperation) -> Self {
        match op {
            StartUpdateOperation::Lgsm => Self::Lgsm,
            StartUpdateOperation::Game => Self::Game,
            StartUpdateOperation::Mods => Self::Mods,
            StartUpdateOperation::All => Self::All,
        }
    }
}

/// Start an update.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartUpdateRequest {
    pub operation: StartUpdateOperation,
    /// Go ahead although players are connected or a reservation is live
    /// or imminent. Only matters for `game`, `mods` and `all`.
    #[serde(default)]
    pub force: bool,
}

/// A check was started; its result arrives in `check` of the overview.
#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateCheckStartedResponse {
    /// The agent's id for the check (not a job row).
    pub job_id: Option<String>,
    /// Always `check`.
    pub operation: String,
}

/// The portal-managed MatchZy overrides of a server.
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchzyOverridesResponse {
    /// `cvar → value` (string, number or boolean).
    #[schema(value_type = Object)]
    pub overrides: Map<String, Value>,
    /// Whether the host's MatchZy config has them (agent heartbeat); null
    /// = none set on the host, or unknown.
    pub applied: Option<bool>,
    /// Saved but not yet accepted by the agent; pushed on its next connect.
    pub pending: bool,
}

/// Replace the overrides. An empty map clears them.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetMatchzyOverridesRequest {
    /// At most 64 entries. Names match `^matchzy_[a-z0-9_]+$` (at most 128
    /// bytes) and are not `matchzy_demo_upload_*`, `matchzy_remote_*`,
    /// `matchzy_loadmatch*` or `matchzy_loadbackup_url`. Values are a
    /// string, number or boolean that renders to at most 256 bytes of
    /// printable ASCII without `"`, `;` or `\`.
    #[schema(value_type = Object)]
    pub overrides: Map<String, Value>,
}

/// What happened to a saved override set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverridesDelivery {
    /// The agent wrote them and started `repair-config` to apply them now.
    Applying,
    /// The agent wrote them, but another job is running: the next `mods`,
    /// `all` or `repair-config` applies them.
    Written,
    /// Not delivered (agent away, too old, or it refused): pushed on the
    /// agent's next connect.
    Pending,
}

/// Result of saving the overrides.
#[derive(Debug, Serialize, ToSchema)]
pub struct SetMatchzyOverridesResponse {
    /// The set as stored (strings trimmed).
    #[schema(value_type = Object)]
    pub overrides: Map<String, Value>,
    pub delivery: OverridesDelivery,
    /// The agent's `repair-config` job (trigger `overrides`) when
    /// `delivery` is `applying`.
    pub job_id: Option<String>,
    /// Why it was not applied now, for `written` and `pending`.
    pub detail: Option<String>,
}

// =============================================================================
// Reading the agent's JSON
// =============================================================================

/// A version field: a string, or a number some tool wrote unquoted.
fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn flag(v: Option<&Value>) -> Option<bool> {
    v?.as_bool()
}

fn versions(v: Option<&Value>) -> Option<UpdateVersions> {
    let v = v.filter(|v| v.is_object())?;
    Some(UpdateVersions {
        lgsm: text(v.get("lgsm")),
        cs2_build: text(v.get("cs2_build")),
        metamod: text(v.get("metamod")),
        cssharp: text(v.get("cssharp")),
        matchzy: text(v.get("matchzy")),
    })
}

fn load_health(v: Option<&Value>) -> Option<UpdateLoadHealth> {
    let v = v.filter(|v| v.is_object())?;
    Some(UpdateLoadHealth {
        metamod: flag(v.get("metamod")),
        cssharp: flag(v.get("cssharp")),
        matchzy: flag(v.get("matchzy")),
        error: text(v.get("error")),
    })
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| text(Some(x))).collect())
        .unwrap_or_default()
}

fn config_report(v: Option<&Value>) -> Option<UpdateConfigReport> {
    let v = v.filter(|v| v.is_object())?;
    Some(UpdateConfigReport {
        cvars_changed: v
            .get("cvars_changed")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        Some(UpdateCvarChange {
                            name: text(c.get("name"))?,
                            before: text(c.get("before")),
                            after: text(c.get("after")),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        cvars_removed: strings(v.get("cvars_removed")),
        cvars_added: strings(v.get("cvars_added")),
        files_changed: strings(v.get("files_changed")),
    })
}

/// Whether one component has an update, against what is installed now.
///
/// While the installed version is still the one the check saw, the agent's
/// own verdict stands (it knows how to compare its sources). Once a job has
/// moved the version on, the check's verdict is stale: compare with the
/// check's `latest` instead, and say "unknown" when either side is.
fn component_available(
    installed_now: Option<&str>,
    installed_at_check: Option<&str>,
    latest: Option<&str>,
    agent_verdict: Option<bool>,
) -> Option<bool> {
    if installed_now == installed_at_check {
        return agent_verdict;
    }
    match (installed_now, latest) {
        (Some(now), Some(latest)) => Some(now != latest),
        _ => None,
    }
}

fn availability(check: &Value, installed_now: &UpdateVersions) -> UpdateAvailability {
    let at_check = versions(check.get("installed")).unwrap_or_default();
    let latest = versions(check.get("latest")).unwrap_or_default();
    let verdicts = check.get("update_available");
    let verdict = |key: &str| flag(verdicts.and_then(|v| v.get(key)));
    let one = |now: &Option<String>, was: &Option<String>, new: &Option<String>, key: &str| {
        component_available(now.as_deref(), was.as_deref(), new.as_deref(), verdict(key))
    };

    let metamod = one(
        &installed_now.metamod,
        &at_check.metamod,
        &latest.metamod,
        "metamod",
    );
    let cssharp = one(
        &installed_now.cssharp,
        &at_check.cssharp,
        &latest.cssharp,
        "cssharp",
    );
    let matchzy = one(
        &installed_now.matchzy,
        &at_check.matchzy,
        &latest.matchzy,
        "matchzy",
    );
    let plugins = [metamod, cssharp, matchzy];
    let mods = if plugins.contains(&Some(true)) {
        Some(true)
    } else if plugins.iter().all(Option::is_none) {
        // Nothing moved since the check: its own roll-up stands.
        if installed_now.metamod == at_check.metamod
            && installed_now.cssharp == at_check.cssharp
            && installed_now.matchzy == at_check.matchzy
        {
            verdict("mods")
        } else {
            None
        }
    } else {
        Some(false)
    };

    UpdateAvailability {
        lgsm: one(&installed_now.lgsm, &at_check.lgsm, &latest.lgsm, "lgsm"),
        game: one(
            &installed_now.cs2_build,
            &at_check.cs2_build,
            &latest.cs2_build,
            "game",
        ),
        metamod,
        cssharp,
        matchzy,
        mods,
    }
}

// =============================================================================
// Shared pieces
// =============================================================================

fn job_summary(job: &GameServerUpdateJob, username: Option<String>) -> UpdateJobSummary {
    UpdateJobSummary {
        id: job.id.to_string(),
        server_id: job.server_id.to_string(),
        operation: job.operation,
        status: job.status,
        trigger: job.trigger,
        requested_by: job.requested_by.map(|u| u.to_string()),
        requested_by_username: username,
        forced: job.forced,
        phase: job.phase.clone(),
        failed_phase: job.failed_phase.clone(),
        error: job.error.clone(),
        restarted: job.restarted,
        agent_synthesized: job.agent_synthesized,
        blocks_allocation: job.blocks_allocation(),
        created_at: job.created_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        updated_at: job.updated_at,
    }
}

fn job_response(job: &GameServerUpdateJob, username: Option<String>) -> UpdateJobResponse {
    UpdateJobResponse {
        summary: job_summary(job, username),
        before: versions(job.before_versions.as_ref()),
        after: versions(job.after_versions.as_ref()),
        load: load_health(job.load.as_ref()),
        config_report: config_report(job.config_report.as_ref()),
        log_tail: job.log_tail.clone(),
    }
}

/// Usernames of the admins behind jobs, looked up once each.
#[derive(Default)]
struct Usernames(HashMap<UserId, Option<String>>);

impl Usernames {
    async fn of(&mut self, state: &AppState, job: &GameServerUpdateJob) -> Option<String> {
        let user_id = job.requested_by?;
        if let Some(name) = self.0.get(&user_id) {
            return name.clone();
        }
        let name = state
            .user_service
            .get_user(user_id)
            .await
            .ok()
            .map(|u| u.username);
        self.0.insert(user_id, name.clone());
        name
    }
}

/// Which of the gate's lookups failed. A failed lookup is a reason in
/// itself: the gate fails closed.
#[derive(Debug, Clone, Copy, Default)]
struct LookupFailures {
    reservation: bool,
    bookings: bool,
}

/// What a restart would interrupt: humans on the server (from the
/// heartbeat's `status`, when it is fresh), a match the server or MatchZy
/// reports, a live reservation, and bookings that are active or start
/// within 30 minutes.
///
/// It fails closed: what cannot be known — who is connected, whether a
/// match is loaded, whether a reservation or booking exists — is a reason
/// too, which only `force` overrides.
fn assess_safety(
    server: &GameServer,
    agent_connected: bool,
    reservation: Option<&ServerReservation>,
    bookings: &[ServerBooking],
    failed: LookupFailures,
    now: DateTime<Utc>,
) -> UpdateSafety {
    let fresh = agent_connected
        && server
            .last_status_at
            .is_some_and(|at| now - at <= Duration::seconds(STATUS_FRESH_SECS));
    let status = server
        .last_status_output
        .as_deref()
        .filter(|_| fresh)
        .map(console::parse_status);
    let players: Vec<String> = status
        .as_ref()
        .map(|s| {
            s.players
                .iter()
                .filter(|p| !p.bot)
                .map(|p| p.name.clone())
                .collect()
        })
        .unwrap_or_default();
    // The `players :` line when the server printed one, else the rows.
    let humans_connected = status.as_ref().map(|s| {
        s.humans
            .unwrap_or_else(|| u32::try_from(players.len()).unwrap_or(u32::MAX))
    });

    let horizon = now + Duration::minutes(IMMINENT_MINUTES);
    let near: Vec<&ServerBooking> = bookings
        .iter()
        .filter(|b| b.starts_at <= horizon && b.ends_at > now)
        .collect();

    let mut reasons = Vec::new();
    match humans_connected {
        Some(0) => {}
        Some(n) => reasons.push(format!(
            "{n} player{} connected",
            if n == 1 { " is" } else { "s are" }
        )),
        None => reasons.push(
            "player count unknown (no fresh CS2 status from the agent in the last 2 minutes)"
                .to_string(),
        ),
    }
    match server.status {
        GameServerStatus::InMatch => {
            reasons.push("the server is hosting a portal match (status in_match)".into());
        }
        GameServerStatus::BusyExternal => reasons.push(
            "the server is running a match the portal did not set up (status busy_external)".into(),
        ),
        // With a reservation found, its own reason says it better.
        GameServerStatus::Reserved | GameServerStatus::Configuring if reservation.is_none() => {
            reasons.push(format!("the server is {} for a match", server.status));
        }
        _ => {}
    }
    match server.last_gamestate {
        Some(g) if g.is_idle() => {}
        Some(g) => reasons.push(format!("MatchZy has a match loaded (gamestate {g})")),
        None => reasons.push("match state unknown (no MatchZy gamestate from the agent)".into()),
    }
    if let Some(r) = reservation {
        reasons.push(format!(
            "a {} reservation is live on this server (match {}, {})",
            r.kind, r.match_id, r.status
        ));
    } else if failed.reservation {
        reasons.push("whether a reservation is live could not be checked".into());
    }
    if failed.bookings {
        reasons.push("the server's bookings could not be checked".into());
    }
    for b in &near {
        let what = b.reason.as_deref().unwrap_or("no reason given");
        if b.covers(now) {
            reasons.push(format!(
                "a booking is active until {} ({what})",
                b.ends_at.to_rfc3339()
            ));
        } else {
            reasons.push(format!(
                "a booking starts at {} ({what})",
                b.starts_at.to_rfc3339()
            ));
        }
    }

    UpdateSafety {
        humans_connected,
        players,
        reservation: reservation.map(|r| UpdateSafetyReservation {
            id: r.id.to_string(),
            match_id: r.match_id.to_string(),
            kind: r.kind,
            status: r.status,
        }),
        bookings: near
            .iter()
            .map(|b| UpdateSafetyBooking {
                id: b.id.to_string(),
                reason: b.reason.clone(),
                tournament_id: b.tournament_id.map(|t| t.to_string()),
                starts_at: b.starts_at,
                ends_at: b.ends_at,
            })
            .collect(),
        restart_needs_force: !reasons.is_empty(),
        reasons,
    }
}

async fn safety_for(state: &AppState, server: &GameServer, connected: bool) -> UpdateSafety {
    // A failed lookup is logged and becomes a reason of its own: the gate
    // fails closed rather than reading an error as "nothing found".
    let mut failed = LookupFailures::default();
    let reservation = match state
        .server_reservation_repo
        .find_live_by_server(server.id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(server_id = %server.id, error = %e, "live-reservation lookup failed");
            failed.reservation = true;
            None
        }
    };
    let bookings = match state.game_server_registry.list_bookings(server.id).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(server_id = %server.id, error = %e, "booking lookup failed");
            failed.bookings = true;
            Vec::new()
        }
    };
    assess_safety(
        server,
        connected,
        reservation.as_ref(),
        &bookings,
        failed,
        Utc::now(),
    )
}

/// The gates every command to the helper shares: an agent is connected and
/// it advertises `lgsm_update`.
fn require_update_capable(
    state: &AppState,
    server_id: GameServerId,
    update_state: &GameServerUpdateState,
) -> Result<(), ApiError> {
    if !state.agent_manager.is_connected(server_id) {
        return Err(ApiError::conflict("no agent connected for this server"));
    }
    if !update_state.can_update() {
        return Err(ApiError::conflict(
            "this server's agent cannot run updates: it does not advertise the lgsm_update \
             capability (agent older than 0.3.0, or no LinuxGSM update helper configured on \
             the host)",
        ));
    }
    Ok(())
}

/// An agent's refusal is the admin's conflict to resolve (`refused: …`,
/// e.g. another job is running); anything else it reports is the host
/// failing to do what it was asked.
fn agent_error(error: &str) -> ApiError {
    if error.starts_with("refused:") {
        ApiError::conflict(format!("the agent refused: {error}"))
    } else {
        bad_gateway(format!("the agent could not start the operation: {error}"))
    }
}

fn send_error(e: SendError) -> ApiError {
    if e.not_delivered() {
        ApiError::conflict(e.to_string())
    } else {
        bad_gateway(e.to_string())
    }
}

/// Hand a freshly created job to the agent, and settle the row if the
/// agent certainly did not take it.
async fn start_job(
    state: &AppState,
    auth: &AuthenticatedUser,
    server: &GameServer,
    job: GameServerUpdateJob,
) -> Result<GameServerUpdateJob, ApiError> {
    let updates = &state.game_server_updates;
    let operation = job.operation;
    let force = job.forced;

    let command = AgentCommand::UpdateStart {
        operation: operation.as_str().to_string(),
        job_id: job.id.to_string(),
    };
    let sent = state
        .agent_manager
        .try_send_command(server.id, command)
        .await;
    let audit_row = |ok: bool, output: String| CreateAdminServerCommand {
        server_id: server.id,
        reservation_id: None,
        admin_user_id: auth.user_id,
        kind: format!("update:{operation}"),
        command: format!("update_start {operation} {}", job.id),
        output: Some(output),
        ok,
        force,
    };

    let failure = match sent {
        Ok(outcome) if outcome.ok => {
            updates
                .audit(audit_row(true, outcome.output.unwrap_or_default()))
                .await;
            tracing::info!(server_id = %server.id, job_id = %job.id, operation = %operation,
                force, admin = %auth.username, admin_user_id = %auth.user_id,
                "game-server update started");
            // Progress may already have moved it on.
            return Ok(updates
                .service
                .find_job(server.id, job.id)
                .await
                .ok()
                .flatten()
                .unwrap_or(job));
        }
        // The agent answered no: nothing was started.
        Ok(outcome) => {
            let error = outcome
                .error
                .unwrap_or_else(|| "the agent refused without a reason".into());
            updates
                .service
                .fail_job(job.id, &error)
                .await
                .map_err(ApiError::from)?;
            updates.audit(audit_row(false, error.clone())).await;
            agent_error(&error)
        }
        // The agent never saw it: nothing was started.
        Err(e) if e.not_delivered() => {
            updates
                .service
                .fail_job(job.id, &e.to_string())
                .await
                .map_err(ApiError::from)?;
            updates.audit(audit_row(false, e.to_string())).await;
            send_error(e)
        }
        // The frame went out and no answer came back: the helper may be
        // running. The job stays `queued` — for a restarting operation
        // that keeps the server out of allocation, as it must while CS2
        // may be about to restart — and is settled by what the agent says
        // next: progress or a heartbeat naming it moves it on, a result
        // ends it, and the lost-job pass asks the agent (`update_status`)
        // after 20 minutes of silence.
        Err(e) => {
            let mut reason = format!(
                "{e}: whether the update started is unknown. Job {} stays queued until the \
                 agent reports on it, or is settled by the lost-job pass after {} minutes \
                 without word",
                job.id, UPDATE_JOB_STALL_MINUTES
            );
            if job.blocks_allocation() {
                reason.push_str("; meanwhile the server is not allocated to matches");
            }
            updates.audit(audit_row(false, reason.clone())).await;
            tracing::warn!(server_id = %server.id, job_id = %job.id, operation = %operation,
                error = %e, "game-server update start unanswered; outcome unknown");
            bad_gateway(reason)
        }
    };
    Err(failure)
}

// =============================================================================
// Handlers
// =============================================================================

/// Update state of a server: what is installed, the last check, health of
/// the plugin stack and the MatchZy config, jobs, and what a restart would
/// interrupt.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers/{server_id}/updates",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 200, description = "Update overview", body = DataResponse<GameServerUpdatesResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn get_updates(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<Json<DataResponse<GameServerUpdatesResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let server = state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;
    let updates = &state.game_server_updates.service;
    let update_state = updates.state(id).await.map_err(ApiError::from)?;
    let jobs = updates
        .recent_jobs(id, RECENT_JOBS)
        .await
        .map_err(ApiError::from)?;
    let connected = state.agent_manager.is_connected(id);

    let installed = versions(update_state.installed_versions.as_ref());
    let nothing_installed = UpdateVersions::default();
    let installed_now = installed.as_ref().unwrap_or(&nothing_installed);
    let check = update_state.update_check.as_ref().map(|c| UpdateCheckInfo {
        checked_at: update_state.update_checked_at,
        latest: versions(c.get("latest")).unwrap_or_default(),
        update_available: availability(c, installed_now),
        demo_upload_block_present: flag(c.get("demo_upload_block_present")),
    });

    // The newest reading of the plugin stack: a job that restarted the
    // server after the last check knows better than the check.
    let check_load = update_state
        .update_check
        .as_ref()
        .and_then(|c| load_health(c.get("load")));
    let job_load = jobs
        .iter()
        .filter(|j| j.load.is_some())
        .find_map(|j| Some((j.finished_at?, load_health(j.load.as_ref())?)));
    let load = match (job_load, update_state.update_checked_at) {
        (Some((finished, load)), Some(checked)) if finished > checked => Some(load),
        (Some((_, load)), None) => Some(load),
        _ => check_load,
    };

    let mut names = Usernames::default();
    let mut recent_jobs = Vec::with_capacity(jobs.len());
    for job in &jobs {
        let name = names.of(&state, job).await;
        recent_jobs.push(job_summary(job, name));
    }
    // Normally among the last ten; looked up only if a burst of
    // agent-side jobs pushed it out.
    let active = match jobs.iter().find(|j| !j.status.is_terminal()) {
        Some(job) => Some(job.clone()),
        None if jobs.len() < usize::try_from(RECENT_JOBS).unwrap_or(usize::MAX) => None,
        None => updates.active_job(id).await.map_err(ApiError::from)?,
    };
    let active_job = match &active {
        Some(job) => Some(job_summary(job, names.of(&state, job).await)),
        None => None,
    };

    let agent_job = update_state
        .agent_update_job
        .as_ref()
        .filter(|_| connected)
        .map(|j| UpdateAgentJob {
            job_id: text(j.get("job_id")),
            operation: text(j.get("operation")),
            phase: text(j.get("phase")),
            status: text(j.get("status")),
        });

    Ok(Json(DataResponse::new(
        GameServerUpdatesResponse {
            server_id: server.id.to_string(),
            agent_connected: connected,
            agent_version: server.agent_version.clone(),
            supported: update_state.can_update(),
            capabilities: update_state.capabilities.clone(),
            installed,
            check,
            load,
            demo_upload_wired: update_state.demo_upload_wired,
            overrides_applied: update_state.overrides_applied,
            reload_pending: update_state.reload_pending,
            agent_job,
            active_job,
            recent_jobs,
            safety: safety_for(&state, &server, connected).await,
        },
        request_id,
    )))
}

/// Ask the host to check installed against latest versions. Answers as
/// soon as the agent has started the check; the result lands in the
/// overview's `check` a few seconds later.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/updates/check",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 202, description = "Check started", body = DataResponse<UpdateCheckStartedResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
        (status = 409, description = "No agent connected, no lgsm_update capability, or the agent refused (a job is running)", body = ApiError),
        (status = 502, description = "The agent did not answer or could not start the check", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn check_updates(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<(StatusCode, Json<DataResponse<UpdateCheckStartedResponse>>)> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;
    let updates = &state.game_server_updates;
    let update_state = updates.service.state(id).await.map_err(ApiError::from)?;
    require_update_capable(&state, id, &update_state)?;

    let sent = state
        .agent_manager
        .try_send_command(id, AgentCommand::UpdateCheck)
        .await;
    let (result, ok, output) = match sent {
        Ok(outcome) if outcome.ok => {
            let output = outcome.output.unwrap_or_default();
            let job_id = parse_output(Some(&output)).and_then(|v| text(v.get("job_id")));
            (Ok(job_id), true, output)
        }
        Ok(outcome) => {
            let error = outcome
                .error
                .unwrap_or_else(|| "the agent refused without a reason".into());
            (Err(agent_error(&error)), false, error)
        }
        Err(e) => (Err(send_error(e)), false, e.to_string()),
    };
    updates
        .audit(CreateAdminServerCommand {
            server_id: id,
            reservation_id: None,
            admin_user_id: auth.user_id,
            kind: "update:check".into(),
            command: "update_check".into(),
            output: Some(output),
            ok,
            force: false,
        })
        .await;

    let job_id = result?;
    Ok((
        StatusCode::ACCEPTED,
        Json(DataResponse::new(
            UpdateCheckStartedResponse {
                job_id,
                operation: "check".into(),
            },
            request_id,
        )),
    ))
}

/// Start an update. Answers as soon as the agent has started the helper;
/// poll the job for progress and the result.
///
/// `game`, `mods` and `all` restart CS2 and are refused (409) while humans
/// are connected, the server or MatchZy reports a match, a reservation is
/// live, or a booking is active or starts within 30 minutes — and when any
/// of that cannot be known (no fresh status, a failed lookup) — unless
/// `force` is set. While such a job is in
/// flight the server is not allocated to matches.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/updates",
    params(("server_id" = String, Path, description = "Game server ID")),
    request_body = StartUpdateRequest,
    responses(
        (status = 202, description = "Update started", body = DataResponse<UpdateJobResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
        (status = 409, description = "No agent connected; no lgsm_update capability; another job is running; players connected or a reservation/booking is live or imminent (without force); or the agent refused. `detail` names the reason", body = ApiError),
        (status = 502, description = "The agent could not start the helper (the job ends `failed`), or it did not answer: then whether the update started is unknown, `detail` says so and names the job, which stays `queued` until the agent reports on it or the lost-job pass settles it", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn start_update(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<StartUpdateRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<UpdateJobResponse>>)> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let operation = UpdateOperation::from(body.operation);
    let job = gated_start(&state, &auth, id, operation, body.force).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(DataResponse::new(
            job_response(&job, Some(auth.username.clone())),
            request_id,
        )),
    ))
}

/// Re-assert the managed blocks of the MatchZy config (demo upload and the
/// portal's overrides). Restarts nothing, so there is no players gate.
#[utoipa::path(
    post,
    path = "/v1/admin/game-servers/{server_id}/updates/repair-config",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 202, description = "Repair started", body = DataResponse<UpdateJobResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
        (status = 409, description = "No agent connected; no lgsm_update capability; another job is running; or the agent refused", body = ApiError),
        (status = 502, description = "The agent could not start the helper, or it did not answer (outcome unknown: the job stays `queued`, see the start endpoint)", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn repair_config(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<(StatusCode, Json<DataResponse<UpdateJobResponse>>)> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let job = gated_start(&state, &auth, id, UpdateOperation::RepairConfig, false).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(DataResponse::new(
            job_response(&job, Some(auth.username.clone())),
            request_id,
        )),
    ))
}

/// The gates, in the order an admin can act on them, then the start.
async fn gated_start(
    state: &AppState,
    auth: &AuthenticatedUser,
    id: GameServerId,
    operation: UpdateOperation,
    force: bool,
) -> Result<GameServerUpdateJob, ApiError> {
    let server = state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;
    let updates = &state.game_server_updates.service;
    let update_state = updates.state(id).await.map_err(ApiError::from)?;
    require_update_capable(state, id, &update_state)?;

    if let Some(active) = updates.active_job(id).await.map_err(ApiError::from)? {
        return Err(ApiError::conflict(format!(
            "update job {} ({}) is already {} on this server",
            active.id, active.operation, active.status
        )));
    }

    // The job row goes in first, and the restart gate reads only after
    // it is committed. Inserting it locks the server row against
    // allocation (see `PgServerReservationRepository::allocate`), so a
    // match is either already reserved on the server — and the gate sees
    // it — or kept off it by the job. Gating first would leave a window in
    // which a match lands on a server about to restart.
    let job = updates
        .create_job(id, operation, auth.user_id, force)
        .await
        .map_err(ApiError::from)?;

    if operation.restarts_server() && !force {
        // Re-read: status and gamestate as of now, not as of the request.
        let server = match state.game_server_registry.get(id).await {
            Ok(server) => server,
            Err(e) => {
                end_gated_job(state, &job, "the server could not be re-read").await;
                return Err(ApiError::from(e));
            }
        };
        let connected = state.agent_manager.is_connected(id);
        let safety = safety_for(state, &server, connected).await;
        if safety.restart_needs_force {
            let reasons = safety.reasons.join("; ");
            end_gated_job(
                state,
                &job,
                &format!("refused by the safety gate: {reasons}"),
            )
            .await;
            return Err(ApiError::conflict(format!(
                "`{operation}` restarts CS2 and would interrupt: {reasons}. Send force: true \
                 to go ahead anyway"
            )));
        }
        return start_job(state, auth, &server, job).await;
    }

    start_job(state, auth, &server, job).await
}

/// A job the gate turned down never reached the agent: it ends `failed`
/// with the reason, so the server is back in allocation at once.
async fn end_gated_job(state: &AppState, job: &GameServerUpdateJob, reason: &str) {
    if let Err(e) = state
        .game_server_updates
        .service
        .fail_job(job.id, reason)
        .await
    {
        tracing::error!(server_id = %job.server_id, job_id = %job.id, error = %e,
            "gated update job not closed; the lost-job pass will settle it");
    }
}

/// One update job in full: progress while it runs, then versions before
/// and after, the config report and the log tail.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers/{server_id}/updates/{job_id}",
    params(
        ("server_id" = String, Path, description = "Game server ID"),
        ("job_id" = String, Path, description = "Update job ID"),
    ),
    responses(
        (status = 200, description = "The job", body = DataResponse<UpdateJobResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server or job not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn get_update_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((server_id, job_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<UpdateJobResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let job_id: GameServerUpdateJobId = job_id
        .parse()
        .map_err(|_| ApiError::bad_request(format!("invalid update job id: {job_id}")))?;
    state
        .game_server_registry
        .get(id)
        .await
        .map_err(ApiError::from)?;
    let job = state
        .game_server_updates
        .service
        .find_job(id, job_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found(format!("Update job not found: {job_id}")))?;
    let name = Usernames::default().of(&state, &job).await;
    Ok(Json(DataResponse::new(
        job_response(&job, name),
        request_id,
    )))
}

/// The portal-managed MatchZy overrides of a server.
#[utoipa::path(
    get,
    path = "/v1/admin/game-servers/{server_id}/matchzy-overrides",
    params(("server_id" = String, Path, description = "Game server ID")),
    responses(
        (status = 200, description = "The overrides", body = DataResponse<MatchzyOverridesResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn get_matchzy_overrides(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> ApiResult<Json<DataResponse<MatchzyOverridesResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let update_state = state
        .game_server_updates
        .service
        .state(id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(DataResponse::new(
        MatchzyOverridesResponse {
            overrides: update_state.matchzy_overrides,
            applied: update_state.overrides_applied,
            pending: update_state.matchzy_overrides_pending,
        },
        request_id,
    )))
}

/// Replace the portal-managed MatchZy overrides. They are stored whatever
/// the agent's state; when it is connected and capable they are sent at
/// once, otherwise on its next connect. An empty map clears them.
#[utoipa::path(
    put,
    path = "/v1/admin/game-servers/{server_id}/matchzy-overrides",
    params(("server_id" = String, Path, description = "Game server ID")),
    request_body = SetMatchzyOverridesRequest,
    responses(
        (status = 200, description = "Stored; `delivery` says whether the host has them", body = DataResponse<SetMatchzyOverridesResponse>),
        (status = 400, description = "An override breaks the rules; `detail` names it. Nothing was stored", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Server not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "game_servers"
)]
pub async fn set_matchzy_overrides(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<SetMatchzyOverridesRequest>,
) -> ApiResult<Json<DataResponse<SetMatchzyOverridesResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let id = parse_server_id(&server_id)?;
    let overrides = validate_matchzy_overrides(&body.overrides)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let updates = &state.game_server_updates;
    // 404 before anything is written.
    let update_state = updates.service.state(id).await.map_err(ApiError::from)?;
    updates
        .service
        .set_matchzy_overrides(id, &overrides)
        .await
        .map_err(ApiError::from)?;

    let push = updates
        .push_overrides(id, &update_state, overrides.clone())
        .await;
    let (delivery, job_id, detail) = match push {
        OverridesPush::Applying { job_id } => (OverridesDelivery::Applying, job_id, None),
        OverridesPush::Written { reason } => (OverridesDelivery::Written, None, Some(reason)),
        OverridesPush::Pending { reason } => (OverridesDelivery::Pending, None, Some(reason)),
    };

    let names: Vec<&str> = overrides.keys().map(String::as_str).collect();
    updates
        .audit(CreateAdminServerCommand {
            server_id: id,
            reservation_id: None,
            admin_user_id: auth.user_id,
            kind: "matchzy_overrides".into(),
            // Names only: values are settings, not secrets, but the row is
            // a trail of who changed what, not a second copy of the config.
            command: format!(
                "set_matchzy_overrides ({}) {}",
                names.len(),
                names.join(" ")
            ),
            output: Some(match &detail {
                Some(d) => format!("{}: {d}", delivery_name(delivery)),
                None => delivery_name(delivery).to_string(),
            }),
            ok: delivery != OverridesDelivery::Pending,
            force: false,
        })
        .await;
    tracing::info!(server_id = %id, entries = overrides.len(), delivery = ?delivery,
        admin = %auth.username, admin_user_id = %auth.user_id,
        "MatchZy overrides saved");

    Ok(Json(DataResponse::new(
        SetMatchzyOverridesResponse {
            overrides,
            delivery,
            job_id,
            detail,
        },
        request_id,
    )))
}

const fn delivery_name(d: OverridesDelivery) -> &'static str {
    match d {
        OverridesDelivery::Applying => "applying",
        OverridesDelivery::Written => "written",
        OverridesDelivery::Pending => "pending",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v(lgsm: &str, build: &str, mm: Option<&str>, css: &str, mz: &str) -> UpdateVersions {
        UpdateVersions {
            lgsm: Some(lgsm.into()),
            cs2_build: Some(build.into()),
            metamod: mm.map(Into::into),
            cssharp: Some(css.into()),
            matchzy: Some(mz.into()),
        }
    }

    fn check() -> Value {
        json!({
            "installed": {"lgsm": "v26.6.0", "cs2_build": "20012345", "metamod": null,
                          "cssharp": "1.0.370", "matchzy": "0.9.2"},
            "latest": {"lgsm": "v26.7.0", "cs2_build": "20012399",
                       "metamod": "mmsource-2.0.0-git1491-linux.tar.gz",
                       "cssharp": "1.0.380", "matchzy": "0.9.3"},
            "update_available": {"lgsm": true, "game": true, "metamod": null,
                                 "cssharp": true, "matchzy": true, "mods": true},
        })
    }

    #[test]
    fn the_agents_verdict_stands_while_nothing_has_moved() {
        let now = v("v26.6.0", "20012345", None, "1.0.370", "0.9.2");
        let a = availability(&check(), &now);
        assert_eq!(a.lgsm, Some(true));
        assert_eq!(a.game, Some(true));
        assert_eq!(a.metamod, None);
        assert_eq!(a.cssharp, Some(true));
        assert_eq!(a.matchzy, Some(true));
        assert_eq!(a.mods, Some(true));
    }

    #[test]
    fn a_component_updated_since_the_check_is_no_longer_outdated() {
        // A `mods` job brought the plugins to the check's latest.
        let now = v(
            "v26.6.0",
            "20012345",
            Some("mmsource-2.0.0-git1491-linux.tar.gz"),
            "1.0.380",
            "0.9.3",
        );
        let a = availability(&check(), &now);
        assert_eq!(a.metamod, Some(false));
        assert_eq!(a.cssharp, Some(false));
        assert_eq!(a.matchzy, Some(false));
        assert_eq!(a.mods, Some(false));
        // Untouched components keep the agent's verdict.
        assert_eq!(a.lgsm, Some(true));
        assert_eq!(a.game, Some(true));
    }

    #[test]
    fn a_moved_component_without_a_latest_is_unknown() {
        let mut c = check();
        c["latest"]["matchzy"] = json!(null);
        c["update_available"]["matchzy"] = json!(null);
        let now = v("v26.6.0", "20012345", None, "1.0.370", "0.9.4");
        let a = availability(&c, &now);
        assert_eq!(a.matchzy, None);
        // cssharp is still outdated, so the roll-up is too.
        assert_eq!(a.mods, Some(true));
    }

    #[test]
    fn versions_read_numbers_and_ignore_other_shapes() {
        let got = versions(Some(
            &json!({"lgsm": "v1", "cs2_build": 20012345, "metamod": [],
            "cssharp": "", "recorded_at": "t"}),
        ))
        .unwrap();
        assert_eq!(got.lgsm.as_deref(), Some("v1"));
        assert_eq!(got.cs2_build.as_deref(), Some("20012345"));
        assert_eq!(got.metamod, None);
        assert_eq!(got.cssharp, None);
        assert!(versions(Some(&json!(null))).is_none());
        assert!(versions(None).is_none());
    }

    #[test]
    fn config_report_reads_the_agents_shape() {
        let r = config_report(Some(&json!({
            "cvars_changed": [{"name": "matchzy_kick_when_no_match_loaded",
                               "before": "false", "after": "true"}, {"before": "x"}],
            "cvars_removed": ["matchzy_max_tech_pauses_allowed"],
            "cvars_added": ["matchzy_max_tech_pauses"],
            "files_changed": ["config.cfg", "live.cfg"],
        })))
        .unwrap();
        assert_eq!(r.cvars_changed.len(), 1);
        assert_eq!(r.cvars_changed[0].after.as_deref(), Some("true"));
        assert_eq!(r.cvars_removed, vec!["matchzy_max_tech_pauses_allowed"]);
        assert_eq!(r.files_changed.len(), 2);
        assert!(config_report(Some(&json!(null))).is_none());
    }

    const EMPTY: &str = "hostname: x\nmap     : de_mirage\n\
        players : 0 humans, 0 bots (12 max) (not hibernating) (unreserved)\n\n\
          id     time ping loss      state   rate adr name\n#end\n";

    /// An idle, available server whose agent sent an empty `status` just now.
    fn idle_server(now: DateTime<Utc>) -> GameServer {
        GameServer {
            id: GameServerId::new(),
            name: "box".into(),
            game_id: portal_core::ids::GameId::new(),
            ip_address: "203.0.113.10".parse().unwrap(),
            port: 27015,
            gotv_port: None,
            region: "eu-west".into(),
            enabled: true,
            allow_pugs: true,
            status: GameServerStatus::Available,
            current_match_id: None,
            agent_cert_serial: None,
            agent_cert_expires_at: None,
            agent_version: Some("0.3.0".into()),
            last_heartbeat_at: Some(now),
            last_gamestate: Some(portal_core::types::AgentGamestate::None),
            last_map: None,
            last_player_count: Some(0),
            last_status_output: Some(EMPTY.into()),
            last_status_at: Some(now),
            enrollment_token_hash: None,
            enrollment_token_expires_at: None,
            has_demo_token: false,
            created_at: now,
            updated_at: now,
        }
    }

    fn safety(server: &GameServer, connected: bool, failed: LookupFailures) -> UpdateSafety {
        assess_safety(server, connected, None, &[], failed, Utc::now())
    }

    #[test]
    fn an_idle_server_needs_no_force() {
        let s = safety(&idle_server(Utc::now()), true, LookupFailures::default());
        assert_eq!(s.humans_connected, Some(0));
        assert!(!s.restart_needs_force, "{:?}", s.reasons);
        assert!(s.reasons.is_empty());
    }

    #[test]
    fn an_unknown_player_count_needs_force() {
        let now = Utc::now();
        let unknown = "player count unknown (no fresh CS2 status from the agent in the last 2 \
                       minutes)";
        // No status at all (agent sends none, or RCON is down).
        let mut server = idle_server(now);
        server.last_status_output = None;
        server.last_status_at = None;
        let s = safety(&server, true, LookupFailures::default());
        assert_eq!(s.humans_connected, None);
        assert!(s.restart_needs_force);
        assert_eq!(s.reasons, vec![unknown]);
        // A status, but a stale one.
        let mut server = idle_server(now);
        server.last_status_at = Some(now - Duration::minutes(5));
        assert_eq!(
            safety(&server, true, LookupFailures::default()).reasons,
            vec![unknown]
        );
        // The agent is away: its last status says nothing about now.
        assert_eq!(
            safety(&idle_server(now), false, LookupFailures::default()).reasons,
            vec![unknown]
        );
    }

    #[test]
    fn a_busy_server_or_a_loaded_match_needs_force() {
        let now = Utc::now();
        let mut server = idle_server(now);
        server.status = GameServerStatus::BusyExternal;
        server.last_gamestate = Some(portal_core::types::AgentGamestate::Live);
        let s = safety(&server, true, LookupFailures::default());
        assert!(s.restart_needs_force);
        assert_eq!(
            s.reasons,
            vec![
                "the server is running a match the portal did not set up (status \
                 busy_external)",
                "MatchZy has a match loaded (gamestate live)",
            ]
        );

        let mut server = idle_server(now);
        server.status = GameServerStatus::InMatch;
        assert_eq!(
            safety(&server, true, LookupFailures::default()).reasons,
            vec!["the server is hosting a portal match (status in_match)"]
        );

        // Reserved with no live reservation found: the status alone says so.
        let mut server = idle_server(now);
        server.status = GameServerStatus::Reserved;
        assert_eq!(
            safety(&server, true, LookupFailures::default()).reasons,
            vec!["the server is reserved for a match"]
        );

        let mut server = idle_server(now);
        server.last_gamestate = Some(portal_core::types::AgentGamestate::Warmup);
        assert_eq!(
            safety(&server, true, LookupFailures::default()).reasons,
            vec!["MatchZy has a match loaded (gamestate warmup)"]
        );
        server.last_gamestate = None;
        assert_eq!(
            safety(&server, true, LookupFailures::default()).reasons,
            vec!["match state unknown (no MatchZy gamestate from the agent)"]
        );
    }

    #[test]
    fn a_failed_lookup_needs_force() {
        let server = idle_server(Utc::now());
        let s = safety(
            &server,
            true,
            LookupFailures {
                reservation: true,
                bookings: false,
            },
        );
        assert!(s.restart_needs_force);
        assert_eq!(
            s.reasons,
            vec!["whether a reservation is live could not be checked"]
        );
        let s = safety(
            &server,
            true,
            LookupFailures {
                reservation: false,
                bookings: true,
            },
        );
        assert_eq!(
            s.reasons,
            vec!["the server's bookings could not be checked"]
        );
    }

    #[test]
    fn agent_refusals_are_conflicts_and_other_errors_are_the_hosts() {
        assert_eq!(
            agent_error("refused: update job x (mods) is already running").status,
            409
        );
        assert_eq!(
            agent_error("could not start portal-gs-update@mods.service: denied").status,
            502
        );
        assert_eq!(send_error(SendError::NotConnected).status, 409);
        assert_eq!(send_error(SendError::NotSent).status, 409);
        assert_eq!(send_error(SendError::TimedOut).status, 502);
        assert_eq!(send_error(SendError::Disconnected).status, 502);
    }
}
