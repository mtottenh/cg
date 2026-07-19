//! Background task for sending timeout warnings to veto lobbies.
//!
//! This task runs periodically and checks for sessions with approaching deadlines,
//! broadcasting timeout warnings to connected WebSocket clients.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use portal_core::TournamentMatchId;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{error, info, trace};

use crate::state::AppState;
use crate::websocket::{LobbyBroadcast, TimeoutWarningBroadcast, VetoLobbyManager};

/// Thresholds (in seconds) at which to send warnings.
const WARNING_THRESHOLDS: &[u32] = &[10, 5, 3, 2, 1];

/// How often to check for timeout warnings (in milliseconds).
const CHECK_INTERVAL_MS: u64 = 500;

/// Tracks which warnings have been sent to avoid duplicates.
struct WarningTracker {
    /// Set of (match_id, seconds_remaining) that have been sent.
    sent_warnings: HashSet<(String, u32)>,
}

impl WarningTracker {
    fn new() -> Self {
        Self {
            sent_warnings: HashSet::new(),
        }
    }

    /// Check if we should send a warning for this match at this threshold.
    fn should_send(&mut self, match_id: &TournamentMatchId, seconds: u32) -> bool {
        let key = (match_id.to_string(), seconds);
        if self.sent_warnings.contains(&key) {
            false
        } else {
            self.sent_warnings.insert(key);
            true
        }
    }

    /// Clear warnings for a match (e.g., when deadline changes).
    fn clear_match(&mut self, match_id: &TournamentMatchId) {
        let match_str = match_id.to_string();
        self.sent_warnings.retain(|(m, _)| m != &match_str);
    }
}

/// Start the timeout warning background task.
///
/// Spawns a Tokio task that polls for sessions with approaching deadlines
/// and broadcasts warnings. Returns the [`JoinHandle`] so the caller can
/// `await` it during shutdown — previously the handle was dropped, so a
/// panic in the loop was swallowed silently and the task could outlive the
/// process's intent to exit.
///
/// `shutdown` is signalled (via `Notify::notify_waiters`) by the main
/// shutdown handler; the loop exits at the next iteration.
pub fn spawn_timeout_warning_task(state: AppState, shutdown: Arc<Notify>) -> JoinHandle<()> {
    tokio::spawn(async move {
        run_timeout_warning_loop(state, shutdown).await;
    })
}

async fn run_timeout_warning_loop(state: AppState, shutdown: Arc<Notify>) {
    let mut check_interval = interval(Duration::from_millis(CHECK_INTERVAL_MS));
    let mut tracker = WarningTracker::new();

    loop {
        tokio::select! {
            _ = check_interval.tick() => {
                if let Err(e) = check_and_send_warnings(&state, &mut tracker).await {
                    error!(error = %e, "timeout warning task iteration failed");
                }
            }
            () = shutdown.notified() => {
                info!("timeout warning task: shutdown signal received, exiting");
                return;
            }
        }
    }
}

async fn check_and_send_warnings(
    state: &AppState,
    tracker: &mut WarningTracker,
) -> Result<(), String> {
    let now = Utc::now();

    // Get all active lobbies
    let lobby_manager = &state.veto_lobby_manager;

    // For each active lobby, check if there's a session with an approaching deadline
    for match_id in get_active_match_ids(lobby_manager) {
        // Try to get the session state for this match.
        // On error there is no session (or it can't be read) - skip this match.
        let Ok(session_state) = state.veto_service.get_session_state(match_id).await else {
            continue;
        };

        let session = &session_state.session;

        // Check if session is in progress with a deadline
        let Some(deadline) = session.action_deadline else {
            // No deadline set, clear any tracked warnings
            tracker.clear_match(&match_id);
            continue;
        };

        // Calculate seconds remaining
        let duration_until = deadline.signed_duration_since(now);
        let seconds_remaining = duration_until.num_seconds();

        // If already past deadline, skip (timeout handling is done elsewhere)
        if seconds_remaining < 0 {
            tracker.clear_match(&match_id);
            continue;
        }

        let seconds_remaining = seconds_remaining as u32;

        // Check each threshold
        for &threshold in WARNING_THRESHOLDS {
            if seconds_remaining <= threshold && tracker.should_send(&match_id, threshold) {
                // Get team info for the warning
                let Some(current_team_reg_id) = session.current_team_turn else {
                    continue;
                };

                // Look up team name (best effort)
                let team_name = get_team_name_for_registration(state, current_team_reg_id);

                // Send the warning
                if let Some(lobby) = lobby_manager.get_lobby(&match_id) {
                    trace!(
                        "Sending timeout warning for match {} - {} seconds remaining",
                        match_id, threshold
                    );

                    let () =
                        lobby.broadcast(LobbyBroadcast::TimeoutWarning(TimeoutWarningBroadcast {
                            seconds_remaining: threshold,
                            current_team_registration_id: current_team_reg_id,
                            current_team_name: team_name,
                        }));
                }

                // Only send the lowest applicable threshold
                break;
            }
        }
    }

    Ok(())
}

/// Get all active match IDs from the lobby manager.
fn get_active_match_ids(manager: &Arc<VetoLobbyManager>) -> Vec<TournamentMatchId> {
    manager.active_match_ids()
}

/// Get team name for a registration ID.
///
/// For now, returns a placeholder since clients have the registration ID
/// and can look up the team name themselves. In the future, this could
/// be enhanced to cache team names from session state.
fn get_team_name_for_registration(
    _state: &AppState,
    _registration_id: portal_core::TournamentRegistrationId,
) -> String {
    // The client has the registration ID and can look up the team name
    // from the session state which they already have
    "Current Team".to_string()
}
