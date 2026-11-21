//! Post-write settlement for results recorded outside the claim flow.
//!
//! Forfeits (admin, captain, no-show, withdrawal, disqualification), admin
//! score overrides and dispute resolutions write the match row directly. On
//! their own they used to stop there — no standings recompute, no winner
//! advancement, no groups → playoffs transition. Each of those handlers now
//! calls [`settle`] after its write commits, which delegates to
//! `MatchCompletionSaga::settle_result` — the same machinery a confirmed
//! claim runs.
//!
//! Settlement is best-effort *for the response*, exactly like the claim
//! confirm handler: the result write has already committed, so a failure here
//! is logged and left to the lifecycle saga re-drive rather than turned into
//! an error for a request whose primary effect succeeded.

use portal_core::{TournamentBracketId, TournamentMatchId};
use portal_domain::services::tournament::SettleOutcome;
use tracing::warn;

use crate::state::AppMatchCompletionSaga;

/// Settle one match. Returns the outcome, or `None` if settlement failed
/// (already logged).
pub async fn settle(
    saga: &AppMatchCompletionSaga,
    match_id: TournamentMatchId,
) -> Option<SettleOutcome> {
    let result = saga.settle_result(match_id).await;
    log_outcome(match_id, result)
}

/// Re-derive standings / stage advancement for a corrected result without
/// moving participants (see `MatchCompletionSaga::rederive_bracket`).
pub async fn rederive(
    saga: &AppMatchCompletionSaga,
    match_id: TournamentMatchId,
    bracket_id: TournamentBracketId,
) -> Option<SettleOutcome> {
    let result = saga.rederive_bracket(bracket_id).await;
    log_outcome(match_id, result)
}

/// After a manual progression of `source_match_id`: open the matches it fed
/// and walk over any of them — or of `newly_ready` — that is playable
/// against a participant who has left the tournament.
///
/// The saga does this itself for the matches a completion feeds; the manual
/// progression endpoints move participants without it (see
/// `MatchCompletionSaga::settle_fed_matches`). Best-effort: the progression
/// itself has already been applied.
pub async fn settle_after_manual_progression(
    saga: &AppMatchCompletionSaga,
    source_match_id: TournamentMatchId,
    newly_ready: &[TournamentMatchId],
) {
    if let Err(e) = saga.settle_fed_matches(source_match_id).await {
        warn!(
            match_id = %source_match_id,
            error = %e,
            "Opening the matches fed by a manual progression failed"
        );
    }
    for &match_id in newly_ready {
        if let Err(e) = saga.settle_departed(match_id).await {
            warn!(
                %match_id,
                error = %e,
                "Forfeiting a match for a departed participant failed"
            );
        }
    }
}

fn log_outcome(
    match_id: TournamentMatchId,
    result: Result<SettleOutcome, portal_core::DomainError>,
) -> Option<SettleOutcome> {
    crate::observability::record_saga("match-settlement", &result);
    match result {
        Ok(outcome) => Some(outcome),
        Err(e) => {
            warn!(
                %match_id,
                error = %e,
                "Result settlement failed after the result was recorded; \
                 left for the lifecycle saga re-drive"
            );
            None
        }
    }
}
