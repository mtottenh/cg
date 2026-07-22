//! Background lifecycle automation.
//!
//! Until this module existed, the only background work in the server was the
//! veto timeout-warning loop — every other lifecycle step (opening check-in
//! windows, creating veto sessions, processing no-shows, sweeping evidence)
//! required a human with admin credentials to drive the state machine by
//! hand. The loop here makes tournaments run themselves:
//!
//! 1. **Check-in windows** — matches in `scheduled` whose `scheduled_at`
//!    falls inside the lead window transition to `checking_in` and get a
//!    check-in deadline stamped.
//! 2. **Veto sessions** — when the tournament configures a default map-veto
//!    format, a session is created, started, and coin-flipped as the window
//!    opens, so the match is veto-gated before anyone checks in.
//! 3. **No-shows** — matches still in `checking_in` past their deadline are
//!    forfeited against the absent side (or double-forfeited when nobody
//!    showed up).
//! 4. **Result auto-confirmation** — pending result claims whose
//!    `auto_confirm_at` deadline has passed are confirmed and their
//!    match-completion saga is executed, exactly as a manual opponent
//!    confirm would, so ignored claims can no longer stall a bracket.
//! 5. **Stuck completion sagas** — `match_completion` sagas left `failed`
//!    (or `running` past a deadline, e.g. the process died mid-flight) are
//!    re-driven, so a saga that blew up *after* the claim was already
//!    confirmed no longer strands the winner outside the next round.
//! 6. **Evidence hygiene** — expired evidence is processed and stale pending
//!    uploads are cleaned, on a slower cadence.
//!
//! `run_lifecycle_pass` is a single, side-effect-complete pass so
//! integration tests can drive it deterministically; `spawn_lifecycle_task`
//! wraps it in the interval/shutdown loop pattern shared with the veto
//! timeout task.

use crate::handlers::veto::{resolve_side_selection_mode, resolve_veto_format};
use crate::state::{AppState, VetoState};
use axum::extract::FromRef;
use chrono::{Duration as ChronoDuration, Utc};
use portal_core::DomainError;
use portal_core::types::TournamentMatchStatus;
use portal_db::adapters::PgSagaExecutionRepository;
use portal_domain::entities::forfeit::ForfeitTrigger;
use portal_domain::entities::match_lifecycle::TransitionTrigger;
use portal_domain::entities::result_claim::ResultClaim;
use portal_domain::entities::saga::StepStatus;
use portal_domain::entities::tournament::TournamentMatch;
use portal_domain::repositories::tournament::{
    TournamentMapPoolRepository as _, TournamentMatchRepository as _,
};
use portal_domain::services::tournament::{MatchCompletionInput, SagaCoordinator};
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{error, info, warn};

/// Tuning for the lifecycle automation loop. All values overridable via
/// `PORTAL_LIFECYCLE_*` environment variables.
#[derive(Debug, Clone)]
pub struct LifecycleConfig {
    /// How often the loop wakes up.
    pub tick_interval: Duration,
    /// How long before `scheduled_at` the check-in window opens.
    pub check_in_lead: ChronoDuration,
    /// How long after the window opens before absentees are no-showed.
    pub check_in_grace: ChronoDuration,
    /// Age after which pending (never-completed) evidence uploads are
    /// cleaned up.
    pub evidence_stale_max_age: ChronoDuration,
    /// Run the evidence sweep every N ticks (it is cheap but there is no
    /// reason to run it every few seconds).
    pub evidence_sweep_every: u32,
    /// How long a `running` saga may stay running before the re-drive pass
    /// assumes its owner died and takes it over.
    pub saga_stuck_after: ChronoDuration,
    /// Max rows pulled per query per tick — backpressure bound.
    pub batch_limit: i64,
}

impl LifecycleConfig {
    /// Build from environment, falling back to defaults.
    #[must_use]
    pub fn from_env() -> Self {
        fn env_u64(key: &str, default: u64) -> u64 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        }
        Self {
            tick_interval: Duration::from_secs(env_u64("PORTAL_LIFECYCLE_INTERVAL_SECS", 30)),
            check_in_lead: ChronoDuration::minutes(
                i64::try_from(env_u64("PORTAL_CHECKIN_LEAD_MINUTES", 15)).unwrap_or(15),
            ),
            check_in_grace: ChronoDuration::minutes(
                i64::try_from(env_u64("PORTAL_CHECKIN_GRACE_MINUTES", 10)).unwrap_or(10),
            ),
            evidence_stale_max_age: ChronoDuration::hours(
                i64::try_from(env_u64("PORTAL_EVIDENCE_STALE_HOURS", 24)).unwrap_or(24),
            ),
            evidence_sweep_every: 20,
            saga_stuck_after: ChronoDuration::minutes(
                i64::try_from(env_u64("PORTAL_SAGA_STUCK_MINUTES", 10)).unwrap_or(10),
            ),
            batch_limit: 100,
        }
    }
}

/// What a single pass did — returned for observability and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LifecyclePassSummary {
    /// Matches transitioned `scheduled` → `checking_in`.
    pub check_in_windows_opened: u32,
    /// Veto sessions auto-created (created + started + coin-flipped).
    pub veto_sessions_created: u32,
    /// Matches forfeited (single no-show).
    pub no_shows_forfeited: u32,
    /// Matches double-forfeited (nobody showed).
    pub double_forfeits: u32,
    /// Partially-opened check-in windows repaired (`checking_in` with a
    /// NULL `check_in_deadline`).
    pub check_in_deadlines_repaired: u32,
    /// Result claims auto-confirmed past their `auto_confirm_at` deadline.
    pub claims_auto_confirmed: u32,
    /// Failed/stuck `match_completion` sagas successfully re-driven.
    pub sagas_redriven: u32,
    /// Evidence records expired.
    pub evidence_expired: u32,
    /// Stale pending evidence records cleaned.
    pub evidence_stale_cleaned: u32,
    /// Errors encountered (each already logged; the pass continues).
    pub errors: u32,
}

/// Run one pass of the automation. `sweep_evidence` gates the slower
/// evidence sweep so the loop can run it on its own cadence.
pub async fn run_lifecycle_pass(
    state: &AppState,
    cfg: &LifecycleConfig,
    sweep_evidence: bool,
) -> LifecyclePassSummary {
    let mut summary = LifecyclePassSummary::default();
    let now = Utc::now();

    // ------------------------------------------------------------------
    // 1 + 2: open check-in windows, auto-create veto sessions
    // ------------------------------------------------------------------
    match state
        .tournament_match_repo
        .list_scheduled_due(now + cfg.check_in_lead, cfg.batch_limit)
        .await
    {
        Ok(due) => {
            for match_ in due {
                open_check_in_window(state, cfg, &match_, &mut summary).await;
            }
        }
        Err(e) => {
            error!(error = %e, "lifecycle: list_scheduled_due failed");
            summary.errors += 1;
        }
    }

    // ------------------------------------------------------------------
    // 2b: repair partially-opened check-in windows
    //
    // A match left in `checking_in` with a NULL check_in_deadline (the
    // status write landed, the deadline write did not) is invisible to
    // list_scheduled_due AND list_checkin_expired. Stamp the missing
    // deadline so the no-show pass can see it again.
    // ------------------------------------------------------------------
    match state
        .tournament_match_repo
        .list_checkin_missing_deadline(cfg.batch_limit)
        .await
    {
        Ok(stranded) => {
            for match_ in stranded {
                repair_check_in_deadline(state, cfg, &match_, &mut summary).await;
            }
        }
        Err(e) => {
            error!(error = %e, "lifecycle: list_checkin_missing_deadline failed");
            summary.errors += 1;
        }
    }

    // ------------------------------------------------------------------
    // 3: process check-in timeouts (no-shows)
    // ------------------------------------------------------------------
    match state
        .tournament_match_repo
        .list_checkin_expired(now, cfg.batch_limit)
        .await
    {
        Ok(expired) => {
            for match_ in expired {
                process_check_in_timeout(state, &match_, &mut summary).await;
            }
        }
        Err(e) => {
            error!(error = %e, "lifecycle: list_checkin_expired failed");
            summary.errors += 1;
        }
    }

    // ------------------------------------------------------------------
    // 4: auto-confirm overdue result claims + run their completion sagas
    // ------------------------------------------------------------------
    process_overdue_result_claims(state, &mut summary).await;

    // ------------------------------------------------------------------
    // 5: re-drive failed / stuck match-completion sagas
    // ------------------------------------------------------------------
    redrive_stuck_completion_sagas(state, cfg, now, &mut summary).await;

    // ------------------------------------------------------------------
    // 6: evidence hygiene (slower cadence)
    // ------------------------------------------------------------------
    if sweep_evidence {
        match state.evidence_service.process_expired().await {
            Ok(expired) => {
                summary.evidence_expired = u32::try_from(expired.len()).unwrap_or(u32::MAX);
            }
            Err(e) => {
                error!(error = %e, "lifecycle: evidence expiry sweep failed");
                summary.errors += 1;
            }
        }
        match state
            .evidence_service
            .cleanup_stale_pending(cfg.evidence_stale_max_age)
            .await
        {
            Ok(cleaned) => {
                summary.evidence_stale_cleaned = u32::try_from(cleaned.len()).unwrap_or(u32::MAX);
            }
            Err(e) => {
                error!(error = %e, "lifecycle: stale-pending evidence sweep failed");
                summary.errors += 1;
            }
        }
    }

    summary
}

/// Transition one due match into its check-in window, stamp the deadline,
/// and set up the veto session when the tournament configures one.
async fn open_check_in_window(
    state: &AppState,
    cfg: &LifecycleConfig,
    match_: &TournamentMatch,
    summary: &mut LifecyclePassSummary,
) {
    let match_id = match_.id;

    // Order matters. The deadline is stamped BEFORE the status transition so
    // a crash (or a failure) between the two writes leaves the match in
    // `scheduled` — still visible to `list_scheduled_due`, so the next pass
    // simply redoes both writes. The old order left the match in
    // `checking_in` with a NULL deadline, which is invisible to
    // `list_scheduled_due` (status != 'scheduled') AND to
    // `list_checkin_expired` (deadline IS NULL): stranded forever. A
    // `scheduled` match carrying an early deadline is harmless — both
    // sweeps key off `status`, and the value is re-stamped on the retry.
    //
    // Deadline: grace period from whichever is later — the scheduled time
    // or now (a match scheduled in the past still gets a full grace window).
    let base = match_
        .scheduled_at
        .map_or_else(Utc::now, |s| s.max(Utc::now()));
    if let Err(e) = state
        .tournament_match_repo
        .set_check_in_deadline(match_id, base + cfg.check_in_grace)
        .await
    {
        error!(%match_id, error = %e, "lifecycle: failed to set check-in deadline");
        summary.errors += 1;
        return;
    }

    if let Err(e) = state
        .match_lifecycle_service
        .transition(
            match_id,
            TournamentMatchStatus::CheckingIn,
            TransitionTrigger::System {
                job_name: "open_check_in_window".to_string(),
            },
            Some("Check-in window opened".to_string()),
        )
        .await
    {
        error!(%match_id, error = %e, "lifecycle: failed to open check-in window");
        summary.errors += 1;
        return;
    }

    summary.check_in_windows_opened += 1;
    info!(%match_id, "lifecycle: check-in window opened");

    match ensure_veto_session(state, match_).await {
        Ok(true) => summary.veto_sessions_created += 1,
        Ok(false) => {}
        Err(e) => {
            error!(%match_id, error = %e, "lifecycle: veto session setup failed");
            summary.errors += 1;
        }
    }
}

/// Stamp a check-in deadline on a match stranded in `checking_in` without
/// one, so the no-show pass can pick it up again. Idempotent: once the
/// deadline is set the match no longer matches the repair query.
async fn repair_check_in_deadline(
    state: &AppState,
    cfg: &LifecycleConfig,
    match_: &TournamentMatch,
    summary: &mut LifecyclePassSummary,
) {
    let match_id = match_.id;
    let base = match_
        .scheduled_at
        .map_or_else(Utc::now, |s| s.max(Utc::now()));

    match state
        .tournament_match_repo
        .set_check_in_deadline(match_id, base + cfg.check_in_grace)
        .await
    {
        Ok(_) => {
            summary.check_in_deadlines_repaired += 1;
            warn!(%match_id, "lifecycle: repaired check-in window with missing deadline");
        }
        Err(e) => {
            error!(%match_id, error = %e, "lifecycle: failed to repair check-in deadline");
            summary.errors += 1;
        }
    }
}

/// Create + start + coin-flip a veto session for the match if (a) the
/// tournament configures a default map-veto format and (b) no session exists
/// yet. Returns whether a session was created.
async fn ensure_veto_session(
    state: &AppState,
    match_: &TournamentMatch,
) -> Result<bool, DomainError> {
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;

    let Some(format_id) = tournament.default_map_veto_format.as_deref() else {
        return Ok(false);
    };
    let (Some(p1), Some(p2)) = (
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ) else {
        return Ok(false);
    };

    let veto_state = VetoState::from_ref(state);
    let format = resolve_veto_format(format_id, &veto_state).map_err(|e| {
        DomainError::Internal(format!("unresolvable veto format {format_id}: {e:?}"))
    })?;

    // Map pool: tournament/stage-effective pool, else the game default.
    let map_pool = if let Ok(Some(pool)) = state
        .tournament_map_pool_repo
        .get_effective(match_.tournament_id, Some(match_.stage_id))
        .await
    {
        pool.maps
    } else if let Ok(Some(game)) = state
        .game_repo
        .find_by_id(tournament.game_id.as_uuid())
        .await
    {
        crate::handlers::games::extract_map_pool(&game)
    } else {
        Vec::new()
    };

    let plugin_id = state
        .game_repo
        .find_by_id(tournament.game_id.as_uuid())
        .await
        .ok()
        .flatten()
        .map(|g| g.plugin_id)
        .unwrap_or_default();
    let side_mode = resolve_side_selection_mode(&tournament, &plugin_id, &state.plugin_manager);

    let session = match state
        .veto_service
        .create_session(match_.id, &format, map_pool, None, side_mode)
        .await
    {
        Ok(session) => session,
        // Someone (admin, participant) already created one — that's fine.
        Err(DomainError::Conflict(_)) => return Ok(false),
        Err(e) => return Err(e),
    };

    state.veto_service.start_session(session.id).await?;

    // Automated coin flip: fair 50/50, winner picks first (the standard
    // convention the WS auto-flip also uses).
    let winner = if rand::rng().random_bool(0.5) { p1 } else { p2 };
    state
        .veto_service
        .record_coin_flip(session.id, winner, true)
        .await?;

    info!(match_id = %match_.id, session_id = %session.id, "lifecycle: veto session auto-created");
    Ok(true)
}

/// Auto-confirm every pending result claim whose `auto_confirm_at`
/// deadline has passed, then execute the match-completion saga for each,
/// mirroring what the opponent-confirm endpoint does. Without this the
/// deadline stored on every claim (and returned to clients) was never
/// enforced and an ignored claim stalled its bracket forever.
async fn process_overdue_result_claims(state: &AppState, summary: &mut LifecyclePassSummary) {
    let confirmed = match state.result_service.process_auto_confirmations().await {
        Ok(confirmed) => confirmed,
        Err(e) => {
            error!(error = %e, "lifecycle: auto-confirm sweep failed");
            summary.errors += 1;
            return;
        }
    };

    for claim in confirmed {
        summary.claims_auto_confirmed += 1;
        info!(
            claim_id = %claim.id,
            match_id = %claim.match_id,
            "lifecycle: result claim auto-confirmed"
        );
        // The claim + match are already committed as confirmed/completed;
        // the saga advances the bracket (progression, standings, stats).
        // A failure here is retried implicitly: the match completion can
        // still be driven via the admin progression endpoints.
        if let Err(e) = run_completion_saga_for_claim(state, &claim).await {
            error!(
                claim_id = %claim.id,
                match_id = %claim.match_id,
                error = %e,
                "lifecycle: completion saga failed after auto-confirm"
            );
            summary.errors += 1;
        }
    }
}

/// Saga type re-driven by [`redrive_stuck_completion_sagas`].
const MATCH_COMPLETION_SAGA: &str = "match_completion";

/// Step whose completion means the (non-idempotent) standings deltas have
/// already been applied for this saga.
const STANDINGS_STEP: &str = "update_standings";

/// Re-drive `match_completion` sagas that are `failed`, or `running` past
/// `saga_stuck_after` (their owning process died mid-flight).
///
/// Why this is needed: the confirm handler executes the completion saga
/// *after* the claim-confirm transaction has committed. If the saga then
/// fails, the match is already `completed` but the winner was never
/// advanced — and the handler still returns 200 (`bracket_advanced:
/// false`). Nothing retried that, so the bracket stalled until an admin
/// noticed and drove the progression endpoints by hand.
///
/// Safety: the steps are re-runnable — `complete_match` skips an
/// already-completed match and `assign_participant` writes the same
/// participant into the same slot. The one non-idempotent step is
/// `update_standings` (`points = points + delta`), so any saga whose
/// history already records that step completing is skipped rather than
/// re-run. `retry_count` bounds the attempts at `max_retries`.
async fn redrive_stuck_completion_sagas(
    state: &AppState,
    cfg: &LifecycleConfig,
    now: chrono::DateTime<Utc>,
    summary: &mut LifecyclePassSummary,
) {
    let coordinator = SagaCoordinator::new(Arc::new(PgSagaExecutionRepository::new(
        state.db_pool.clone(),
    )));

    let candidates = match coordinator
        .find_retryable(
            MATCH_COMPLETION_SAGA,
            now - cfg.saga_stuck_after,
            cfg.batch_limit,
        )
        .await
    {
        Ok(candidates) => candidates,
        Err(e) => {
            error!(error = %e, "lifecycle: find_retryable sagas failed");
            summary.errors += 1;
            return;
        }
    };

    for mut execution in candidates {
        let saga_id = execution.id;

        // Never re-run a saga that already applied its standings deltas.
        // Burn its remaining retries so it stops being a candidate, and
        // leave it for the admin progression endpoints. Note the step also
        // "completes" as `not_applicable` on elimination brackets — that
        // wrote nothing, so only `standings_updated: true` blocks a
        // re-drive.
        if execution.step_history.iter().any(|s| {
            s.name == STANDINGS_STEP
                && s.status == StepStatus::Completed
                && s.output
                    .as_ref()
                    .and_then(|o| o.get("standings_updated"))
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        }) {
            warn!(
                %saga_id,
                "lifecycle: skipping saga re-drive — standings already applied"
            );
            if let Err(e) = coordinator.record_retry(&mut execution, true).await {
                error!(%saga_id, error = %e, "lifecycle: failed to retire saga");
                summary.errors += 1;
            }
            continue;
        }

        let input =
            match serde_json::from_value::<MatchCompletionInput>(execution.input_data.clone()) {
                Ok(input) => MatchCompletionInput {
                    // A fresh execution record tracks the re-drive; the old
                    // row stays as the audit trail of the failure.
                    saga_id: None,
                    ..input
                },
                Err(e) => {
                    error!(%saga_id, error = %e, "lifecycle: unreadable saga input, retiring");
                    if let Err(e) = coordinator.record_retry(&mut execution, true).await {
                        error!(%saga_id, error = %e, "lifecycle: failed to retire saga");
                    }
                    summary.errors += 1;
                    continue;
                }
            };

        let match_id = input.match_id;
        match state.match_completion_saga.execute_completion(input).await {
            Ok(result) if result.is_paused() => {
                // Waiting on a result review — not our problem to retry.
                info!(%saga_id, %match_id, "lifecycle: saga re-drive paused for review");
                if let Err(e) = coordinator.record_retry(&mut execution, true).await {
                    error!(%saga_id, error = %e, "lifecycle: failed to retire saga");
                    summary.errors += 1;
                }
            }
            Ok(_) => {
                summary.sagas_redriven += 1;
                info!(%saga_id, %match_id, "lifecycle: completion saga re-driven");
                if let Err(e) = coordinator.record_retry(&mut execution, true).await {
                    error!(%saga_id, error = %e, "lifecycle: failed to retire saga");
                    summary.errors += 1;
                }
            }
            Err(e) => {
                warn!(%saga_id, %match_id, error = %e, "lifecycle: saga re-drive failed");
                summary.errors += 1;
                if let Err(e) = coordinator.record_retry(&mut execution, false).await {
                    error!(%saga_id, error = %e, "lifecycle: failed to bump saga retry count");
                }
            }
        }
    }
}

/// Build the completion-saga input for a confirmed claim and execute it —
/// the same derivation the confirm handler performs.
async fn run_completion_saga_for_claim(
    state: &AppState,
    claim: &ResultClaim,
) -> Result<(), DomainError> {
    let match_ = state
        .tournament_match_repo
        .find_by_id(claim.match_id)
        .await?
        .ok_or(DomainError::TournamentMatchNotFound(claim.match_id))?;

    let winner = claim.claimed_winner_registration_id;
    let loser = if match_.participant1_registration_id == Some(winner) {
        match_.participant2_registration_id
    } else {
        match_.participant1_registration_id
    }
    .ok_or_else(|| DomainError::InvalidState("Loser participant not found on match".to_string()))?;

    let (winner_score, loser_score) = if match_.participant1_registration_id == Some(winner) {
        (
            claim.claimed_participant1_score,
            claim.claimed_participant2_score,
        )
    } else {
        (
            claim.claimed_participant2_score,
            claim.claimed_participant1_score,
        )
    };

    state
        .match_completion_saga
        .execute_completion(MatchCompletionInput {
            match_id: claim.match_id,
            winner_registration_id: winner,
            loser_registration_id: loser,
            winner_score,
            loser_score,
            is_forfeit: false,
            saga_id: None,
            result_claim_id: Some(claim.id),
        })
        .await
        .map(|_| ())
}

/// Forfeit whichever side failed to check in before the deadline.
async fn process_check_in_timeout(
    state: &AppState,
    match_: &TournamentMatch,
    summary: &mut LifecyclePassSummary,
) {
    let match_id = match_.id;
    let p1_in = match_.participant1_checked_in_at.is_some();
    let p2_in = match_.participant2_checked_in_at.is_some();

    let result = match (p1_in, p2_in) {
        (true, true) => {
            // Both checked in but the match is still in checking_in — the
            // auto-advance handles this on check-in, so reaching here means
            // something interrupted it. Log for visibility, don't punish.
            warn!(%match_id, "lifecycle: both checked in but match still in checking_in");
            return;
        }
        (false, false) => state
            .forfeit_service
            .process_double_forfeit(
                match_id,
                Some("Neither participant checked in before the deadline".to_string()),
                ForfeitTrigger::System {
                    reason: "check_in_timeout".to_string(),
                },
            )
            .await
            .map(|_| {
                summary.double_forfeits += 1;
            }),
        (checked_1, _) => {
            let no_show = if checked_1 {
                match_.participant2_registration_id
            } else {
                match_.participant1_registration_id
            };
            match no_show {
                Some(reg) => state
                    .forfeit_service
                    .process_no_show(match_id, reg)
                    .await
                    .map(|_| {
                        summary.no_shows_forfeited += 1;
                    }),
                None => return,
            }
        }
    };

    match result {
        Ok(()) => info!(%match_id, "lifecycle: check-in timeout processed"),
        Err(e) => {
            error!(%match_id, error = %e, "lifecycle: check-in timeout processing failed");
            summary.errors += 1;
        }
    }
}

/// Spawn the automation loop. Mirrors `spawn_timeout_warning_task`: interval
/// ticks do the work, `shutdown.notified()` ends the loop, per-tick errors
/// are logged and swallowed so the loop survives.
pub fn spawn_lifecycle_task(state: AppState, shutdown: Arc<Notify>) -> JoinHandle<()> {
    let cfg = LifecycleConfig::from_env();
    tokio::spawn(async move {
        info!(
            interval_secs = cfg.tick_interval.as_secs(),
            "lifecycle automation task started"
        );
        let mut tick = interval(cfg.tick_interval);
        let mut tick_count: u32 = 0;
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    tick_count = tick_count.wrapping_add(1);
                    let sweep = tick_count.is_multiple_of(cfg.evidence_sweep_every);
                    let summary = run_lifecycle_pass(&state, &cfg, sweep).await;
                    if summary != LifecyclePassSummary::default() {
                        info!(?summary, "lifecycle pass");
                    }
                }
                () = shutdown.notified() => {
                    info!("lifecycle automation task shutting down");
                    return;
                }
            }
        }
    })
}
