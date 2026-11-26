//! Repository trait for the admin match-rescue operations.
//!
//! The match state machine has terminal states (`forfeit`, `cancelled`) and
//! one-way steps (`ready → scheduled`, `checking_in → …`). When automation or
//! an operator puts a match somewhere it should not be — a no-show forfeit
//! while both teams were sitting in voice, a check-in window that expired,
//! a veto that has to be redone — an admin needs a way back that does not
//! involve editing the database by hand.
//!
//! These writes deliberately bypass `allowed_transitions`: they are explicit,
//! audited admin operations, each guarded on the status the caller saw, and
//! each leaves a `match_status_log` row naming the admin and their reason.

use crate::entities::tournament::TournamentMatch;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::types::TournamentMatchStatus;
use portal_core::{
    DisputeId, DomainError, ResultClaimId, ScheduleProposalId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};

/// Who is rescuing the match, and why. Recorded on the status log.
#[derive(Debug, Clone)]
pub struct RescueAudit {
    /// The acting admin.
    pub admin_id: UserId,
    /// Their stated reason.
    pub reason: String,
}

/// What a reset swept away, alongside the match as it now stands.
#[derive(Debug, Clone)]
pub struct MatchResetSummary {
    /// The match, now `ready`.
    pub match_: TournamentMatch,
    /// Pending result claims superseded by the reset.
    pub superseded_claims: u64,
    /// Open disputes cancelled by the reset.
    pub cancelled_disputes: u64,
    /// Pending schedule proposals cancelled by the reset.
    pub cancelled_proposals: u64,
    /// Whether a veto session (in any state) was discarded.
    pub veto_discarded: bool,
}

/// The forfeit record currently deciding a match.
#[derive(Debug, Clone)]
pub struct ActiveForfeitInfo {
    /// `no_show`, `withdrawal`, `disqualification`, `technical_default`.
    pub forfeit_type: String,
    /// Recorded reason, if any.
    pub reason: Option<String>,
    /// Whether automation (the check-in timeout) recorded it.
    pub triggered_by_system: bool,
}

/// A pending schedule proposal on a match.
#[derive(Debug, Clone)]
pub struct PendingProposalInfo {
    pub id: ScheduleProposalId,
    pub proposed_by_registration_id: TournamentRegistrationId,
    pub expires_at: DateTime<Utc>,
}

/// A pending result claim on a match.
#[derive(Debug, Clone)]
pub struct PendingClaimInfo {
    pub id: ResultClaimId,
    /// `participant`, `server` or `admin`.
    pub source: String,
    pub auto_confirm_at: Option<DateTime<Utc>>,
}

/// The newest `match_completion` saga of a match.
#[derive(Debug, Clone)]
pub struct CompletionSagaInfo {
    /// `pending`, `running`, `completed`, `failed`, `paused`, …
    pub status: String,
    /// The step the saga stopped on, when it did not complete.
    pub failed_step: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Everything about a match's side tables an admin needs to see to decide
/// what is wrong with it. Read in one go so the picture is consistent.
#[derive(Debug, Clone, Default)]
pub struct MatchRescueSnapshot {
    pub active_forfeit: Option<ActiveForfeitInfo>,
    pub pending_proposal: Option<PendingProposalInfo>,
    pub pending_claim: Option<PendingClaimInfo>,
    pub open_dispute_id: Option<DisputeId>,
    pub saga: Option<CompletionSagaInfo>,
}

/// Admin rescue writes on a match and the reads that support them.
#[async_trait]
pub trait MatchRescueRepository: Send + Sync {
    /// Put a match back to `ready`, as if it had never been scheduled.
    ///
    /// In one transaction: guards that the match is still in one of
    /// `allowed_from`; supersedes its active forfeit records and pending
    /// result claims; cancels its open disputes and pending schedule
    /// proposals; retires its unfinished completion sagas; discards its veto
    /// session and per-map game rows; unlocks its lineups; clears the time,
    /// the check-in window, the check-ins, the result and the dispute flag;
    /// and writes a status-log row `logged_from → ready`.
    ///
    /// `logged_from` is the status the admin acted on, which may differ from
    /// the row's status by the time this runs (reverting a forfeit's
    /// progression moves it to `awaiting_result` first).
    ///
    /// `DomainError::Conflict` when the match is no longer in `allowed_from`.
    async fn reset_to_ready(
        &self,
        id: TournamentMatchId,
        allowed_from: &[TournamentMatchStatus],
        logged_from: TournamentMatchStatus,
        audit: &RescueAudit,
    ) -> Result<MatchResetSummary, DomainError>;

    /// Open the check-in window now, or move its deadline.
    ///
    /// Guarded on `ready` / `scheduled` / `checking_in`. Sets the deadline,
    /// fills in `scheduled_at` with now when the match has no time, moves the
    /// match to `checking_in`, keeps any check-ins already made, and writes a
    /// status-log row. Returns the match and the status it was in before.
    ///
    /// `DomainError::Conflict` when the match has left those statuses.
    async fn set_check_in_window(
        &self,
        id: TournamentMatchId,
        deadline: DateTime<Utc>,
        audit: &RescueAudit,
    ) -> Result<(TournamentMatch, TournamentMatchStatus), DomainError>;

    /// Throw the match's veto away so it can be run again.
    ///
    /// Guarded on `checking_in` / `pick_ban` / `in_progress`. Deletes the
    /// veto session (and its actions) and the per-map game rows built from
    /// it, clears `veto_required` (creating the new session sets it again),
    /// moves the match to `pick_ban` and writes a status-log row. Returns
    /// the match and the status it was in before.
    ///
    /// `DomainError::Conflict` when the match has left those statuses.
    async fn discard_veto_for_restart(
        &self,
        id: TournamentMatchId,
        audit: &RescueAudit,
    ) -> Result<(TournamentMatch, TournamentMatchStatus), DomainError>;

    /// Whether the match has a result claim that is pending or confirmed.
    async fn has_live_result_claim(&self, id: TournamentMatchId) -> Result<bool, DomainError>;

    /// Record an admin action that changed data but not the status, as a
    /// status-log row with `from == to`.
    async fn log_admin_action(
        &self,
        id: TournamentMatchId,
        status: TournamentMatchStatus,
        action: &str,
        audit: &RescueAudit,
    ) -> Result<(), DomainError>;

    /// Read the side tables that explain a match's state.
    async fn snapshot(&self, id: TournamentMatchId) -> Result<MatchRescueSnapshot, DomainError>;
}
