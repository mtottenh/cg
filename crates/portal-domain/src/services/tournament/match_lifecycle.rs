//! Match lifecycle service.
//!
//! Handles match state machine transitions and lifecycle management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use portal_core::types::TournamentMatchStatus;
use portal_core::{DomainError, TournamentMatchId, TournamentRegistrationId, UserId};
use tracing::{info, instrument, warn};

use crate::entities::MatchStatusLog;
use crate::entities::match_lifecycle::TransitionTrigger;
use crate::entities::tournament::TournamentMatch;
use crate::repositories::match_lifecycle::{CreateMatchStatusLog, MatchStatusLogRepository};
use crate::repositories::tournament::{
    ParticipantSlot, TournamentMatchRepository, TournamentRegistrationRepository,
};

/// Service for managing match lifecycle and state transitions.
pub struct MatchLifecycleService<TMR, TRR, MSLR>
where
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    MSLR: MatchStatusLogRepository,
{
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    log_repo: Arc<MSLR>,
}

impl<TMR, TRR, MSLR> MatchLifecycleService<TMR, TRR, MSLR>
where
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    MSLR: MatchStatusLogRepository,
{
    /// Create a new match lifecycle service.
    pub const fn new(
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
        log_repo: Arc<MSLR>,
    ) -> Self {
        Self {
            match_repo,
            registration_repo,
            log_repo,
        }
    }

    /// Transition a match to a new status with validation.
    ///
    /// This validates that the transition is allowed according to the state machine
    /// and logs the transition for audit purposes.
    #[instrument(skip(self))]
    pub async fn transition(
        &self,
        match_id: TournamentMatchId,
        to_status: TournamentMatchStatus,
        triggered_by: TransitionTrigger,
        reason: Option<String>,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // Validate the transition
        if !match_.status.can_transition_to(to_status) {
            return Err(DomainError::InvalidState(format!(
                "Cannot transition match from {} to {}",
                match_.status, to_status
            )));
        }

        // Perform the transition
        let updated = self.match_repo.update_status(match_id, to_status).await?;

        // Log the transition
        self.log_transition(match_id, match_.status, to_status, &triggered_by, reason)
            .await?;

        info!(
            match_id = %match_id,
            from = %match_.status,
            to = %to_status,
            "Match transitioned"
        );

        Ok(updated)
    }

    /// Mark a match as ready when both participants are assigned.
    ///
    /// This transitions from Pending to Ready once both participant slots are filled.
    #[instrument(skip(self))]
    pub async fn mark_ready(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // Check current status
        if match_.status != TournamentMatchStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "Match is already in {} status, cannot mark as ready",
                match_.status
            )));
        }

        // Verify both participants are assigned
        if !match_.has_both_participants() {
            return Err(DomainError::InvalidState(
                "Cannot mark match as ready without both participants".to_string(),
            ));
        }

        self.transition(
            match_id,
            TournamentMatchStatus::Ready,
            TransitionTrigger::System {
                job_name: "participant_assignment".to_string(),
            },
            Some("Both participants assigned".to_string()),
        )
        .await
    }

    /// Schedule a match for a specific time.
    ///
    /// This sets the scheduled time and transitions to Scheduled status.
    #[instrument(skip(self))]
    pub async fn schedule(
        &self,
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
        scheduled_by: UserId,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // Check if match can be scheduled
        if !match_.status.can_schedule() {
            return Err(DomainError::InvalidState(format!(
                "Match in {} status cannot be scheduled",
                match_.status
            )));
        }

        // Validate scheduled time is in the future
        if scheduled_at <= Utc::now() {
            return Err(DomainError::InvalidState(
                "Scheduled time must be in the future".to_string(),
            ));
        }

        // Update the scheduled time
        self.match_repo.schedule(match_id, scheduled_at).await?;

        // Transition to Scheduled
        self.transition(
            match_id,
            TournamentMatchStatus::Scheduled,
            TransitionTrigger::User(scheduled_by),
            Some(format!("Scheduled for {scheduled_at}")),
        )
        .await
    }

    /// Record participant check-in for a match.
    ///
    /// Both participants must check in before the match can proceed.
    #[instrument(skip(self))]
    pub async fn check_in(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // Check if match is in checking_in status (or transition to it)
        if match_.status != TournamentMatchStatus::CheckingIn {
            if match_.status == TournamentMatchStatus::Scheduled {
                // Auto-transition to CheckingIn
                let _ = self
                    .transition(
                        match_id,
                        TournamentMatchStatus::CheckingIn,
                        TransitionTrigger::System {
                            job_name: "match_check_in".to_string(),
                        },
                        Some("Check-in window opened".to_string()),
                    )
                    .await?;
            } else {
                return Err(DomainError::InvalidState(format!(
                    "Match in {} status cannot accept check-ins",
                    match_.status
                )));
            }
        }

        // Determine which participant is checking in
        let is_participant1 = match_.participant1_registration_id == Some(registration_id);
        let is_participant2 = match_.participant2_registration_id == Some(registration_id);

        if !is_participant1 && !is_participant2 {
            return Err(DomainError::NotAuthorized(
                "Registration is not a participant in this match".to_string(),
            ));
        }

        let slot = if is_participant1 {
            ParticipantSlot::One
        } else {
            ParticipantSlot::Two
        };

        // Re-fetch match to get current check-in state (may have been transitioned above)
        let current = self.get_match(match_id).await?;

        // Guard against double check-in
        let already_checked_in = if is_participant1 {
            current.participant1_checked_in_at.is_some()
        } else {
            current.participant2_checked_in_at.is_some()
        };
        if already_checked_in {
            return Ok(current);
        }

        // Persist check-in
        let updated = self
            .match_repo
            .check_in_participant(match_id, slot, checked_in_by)
            .await?;

        info!(
            match_id = %match_id,
            registration_id = %registration_id,
            checked_in_by = %checked_in_by,
            participant = if is_participant1 { "1" } else { "2" },
            "Participant checked in for match"
        );

        // If both participants have now checked in, auto-advance
        if updated.both_checked_in() {
            let next_status = if updated.veto_required {
                TournamentMatchStatus::PickBan
            } else {
                TournamentMatchStatus::InProgress
            };
            return self
                .transition(
                    match_id,
                    next_status,
                    TransitionTrigger::System {
                        job_name: "both_checked_in".to_string(),
                    },
                    Some("Both participants checked in".to_string()),
                )
                .await;
        }

        Ok(updated)
    }

    /// Start a match that is ready to play.
    ///
    /// Transitions from Scheduled/CheckingIn/PickBan to InProgress.
    #[instrument(skip(self))]
    pub async fn start_match(
        &self,
        match_id: TournamentMatchId,
        started_by: UserId,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        if !match_.status.can_start() {
            return Err(DomainError::InvalidState(format!(
                "Match in {} status cannot be started",
                match_.status
            )));
        }

        // Transition to InProgress and set started_at
        self.match_repo.start(match_id).await?;

        self.log_transition(
            match_id,
            match_.status,
            TournamentMatchStatus::InProgress,
            &TransitionTrigger::User(started_by),
            Some("Match started".to_string()),
        )
        .await?;

        self.get_match(match_id).await
    }

    /// Admin force transition with override reason.
    ///
    /// Allows admins to force any valid transition with a documented reason.
    #[instrument(skip(self))]
    pub async fn admin_transition(
        &self,
        match_id: TournamentMatchId,
        to_status: TournamentMatchStatus,
        admin_id: UserId,
        override_reason: String,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // For admin transitions, we still validate the transition is technically valid
        if !match_.status.can_transition_to(to_status) {
            warn!(
                match_id = %match_id,
                from = %match_.status,
                to = %to_status,
                admin_id = %admin_id,
                reason = %override_reason,
                "Admin attempting invalid transition"
            );
            return Err(DomainError::InvalidState(format!(
                "Cannot transition match from {} to {} (even with admin override)",
                match_.status, to_status
            )));
        }

        self.transition(
            match_id,
            to_status,
            TransitionTrigger::Admin {
                user_id: admin_id,
                override_reason: override_reason.clone(),
            },
            Some(format!("Admin override: {override_reason}")),
        )
        .await
    }

    /// Forfeit a match.
    ///
    /// Records a forfeit by one participant, awarding the win to the opponent.
    #[instrument(skip(self))]
    pub async fn forfeit(
        &self,
        match_id: TournamentMatchId,
        forfeiting_registration_id: TournamentRegistrationId,
        forfeited_by: UserId,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        if !match_.status.can_forfeit() {
            return Err(DomainError::InvalidState(format!(
                "Match in {} status cannot be forfeited",
                match_.status
            )));
        }

        // Determine winner and loser
        let (winner_id, loser_id) =
            if match_.participant1_registration_id == Some(forfeiting_registration_id) {
                (
                    match_.participant2_registration_id.ok_or_else(|| {
                        DomainError::InvalidState("No opponent to award forfeit win".to_string())
                    })?,
                    forfeiting_registration_id,
                )
            } else if match_.participant2_registration_id == Some(forfeiting_registration_id) {
                (
                    match_.participant1_registration_id.ok_or_else(|| {
                        DomainError::InvalidState("No opponent to award forfeit win".to_string())
                    })?,
                    forfeiting_registration_id,
                )
            } else {
                return Err(DomainError::NotAuthorized(
                    "Registration is not a participant in this match".to_string(),
                ));
            };

        // Record the forfeit
        self.match_repo
            .forfeit(match_id, winner_id, loser_id)
            .await?;

        self.log_transition(
            match_id,
            match_.status,
            TournamentMatchStatus::Forfeit,
            &TransitionTrigger::User(forfeited_by),
            Some(format!(
                "Forfeited by registration {forfeiting_registration_id}"
            )),
        )
        .await?;

        self.get_match(match_id).await
    }

    /// Get status history for a match.
    #[instrument(skip(self))]
    pub async fn get_status_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchStatusLog>, DomainError> {
        // Verify match exists
        let _ = self.get_match(match_id).await?;

        self.log_repo.find_by_match_id(match_id).await
    }

    /// Get match status details including recent history.
    #[instrument(skip(self))]
    pub async fn get_match_status(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<MatchStatusDetails, DomainError> {
        let match_ = self.get_match(match_id).await?;
        let history = self.log_repo.find_by_match_id(match_id).await?;
        let latest_log = history.last().cloned();

        Ok(MatchStatusDetails {
            match_id,
            current_status: match_.status,
            allowed_transitions: match_.status.allowed_transitions(),
            is_terminal: match_.status.is_terminal(),
            is_active: match_.status.is_active(),
            scheduled_at: match_.scheduled_at,
            started_at: match_.started_at,
            completed_at: match_.completed_at,
            transition_count: history.len(),
            latest_transition: latest_log,
        })
    }

    // ==========================================================================
    // HELPER METHODS
    // ==========================================================================

    /// Get a match by ID or return error.
    async fn get_match(&self, match_id: TournamentMatchId) -> Result<TournamentMatch, DomainError> {
        self.match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id))
    }

    /// Log a status transition.
    async fn log_transition(
        &self,
        match_id: TournamentMatchId,
        from_status: TournamentMatchStatus,
        to_status: TournamentMatchStatus,
        triggered_by: &TransitionTrigger,
        reason: Option<String>,
    ) -> Result<MatchStatusLog, DomainError> {
        let (user_id, is_system, metadata) = triggered_by.to_db_fields();

        self.log_repo
            .create(CreateMatchStatusLog {
                match_id,
                from_status,
                to_status,
                transition_reason: reason,
                triggered_by_user_id: user_id,
                triggered_by_system: is_system,
                metadata,
            })
            .await
    }
}

/// Detailed match status information.
#[derive(Debug, Clone)]
pub struct MatchStatusDetails {
    /// Match ID.
    pub match_id: TournamentMatchId,
    /// Current status.
    pub current_status: TournamentMatchStatus,
    /// Allowed transitions from current status.
    pub allowed_transitions: Vec<TournamentMatchStatus>,
    /// Whether match is in terminal state.
    pub is_terminal: bool,
    /// Whether match is actively in progress.
    pub is_active: bool,
    /// Scheduled time (if any).
    pub scheduled_at: Option<DateTime<Utc>>,
    /// When match started (if started).
    pub started_at: Option<DateTime<Utc>>,
    /// When match completed (if completed).
    pub completed_at: Option<DateTime<Utc>>,
    /// Number of status transitions.
    pub transition_count: usize,
    /// Latest transition log entry.
    pub latest_transition: Option<MatchStatusLog>,
}

// Manual Clone implementation
impl<TMR, TRR, MSLR> Clone for MatchLifecycleService<TMR, TRR, MSLR>
where
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    MSLR: MatchStatusLogRepository,
{
    fn clone(&self) -> Self {
        Self {
            match_repo: Arc::clone(&self.match_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            log_repo: Arc::clone(&self.log_repo),
        }
    }
}
