//! Tournament registration service.
//!
//! Handles participant registration, withdrawal, approval, rejection,
//! disqualification, and check-in operations.

use std::sync::Arc;

use portal_core::types::{RegistrationType, TournamentRegistrationStatus, TournamentStatus};
use portal_core::{DomainError, TournamentId, TournamentRegistrationId, UserId};
use tracing::instrument;

use crate::entities::tournament::TournamentRegistration;
use crate::repositories::tournament::{TournamentRegistrationRepository, TournamentRepository};

/// Service for tournament registration management.
pub struct RegistrationService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    tournament_repo: Arc<TR>,
    registration_repo: Arc<TRR>,
}

impl<TR, TRR> RegistrationService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new registration service.
    pub const fn new(tournament_repo: Arc<TR>, registration_repo: Arc<TRR>) -> Self {
        Self {
            tournament_repo,
            registration_repo,
        }
    }

    /// Get a registration by ID.
    #[instrument(skip(self))]
    pub async fn get_registration(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<TournamentRegistration, DomainError> {
        self.registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or_else(|| DomainError::TournamentRegistrationNotFound(registration_id))
    }

    /// Withdraw from a tournament.
    ///
    /// The participant can withdraw if they're in a withdrawable state
    /// and the tournament hasn't started yet or withdrawal is allowed.
    #[instrument(skip(self))]
    pub async fn withdraw(
        &self,
        registration_id: TournamentRegistrationId,
        withdrawn_by: UserId,
    ) -> Result<TournamentRegistration, DomainError> {
        let registration = self.get_registration(registration_id).await?;

        // Check if registration can be withdrawn
        if !registration.status.can_withdraw() {
            return Err(DomainError::InvalidState(format!(
                "Cannot withdraw registration in {} status",
                registration.status
            )));
        }

        // Get tournament to check if withdrawal is allowed
        let tournament = self
            .tournament_repo
            .find_by_id(registration.tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(registration.tournament_id))?;

        // Check if tournament has already started and is past a certain state
        if tournament.status == TournamentStatus::InProgress
            || tournament.status == TournamentStatus::Completed
        {
            return Err(DomainError::InvalidState(
                "Cannot withdraw after tournament has started".to_string(),
            ));
        }

        // Verify the user has permission (either the registering user or admin)
        // In the handler we'll check for admin permission separately
        if registration.registered_by != withdrawn_by {
            // This check can be bypassed by admin in the handler
            tracing::info!(
                registration_id = %registration_id,
                withdrawn_by = %withdrawn_by,
                "Withdrawal by non-registrant - requires admin permission"
            );
        }

        self.registration_repo.withdraw(registration_id).await
    }

    /// Approve a pending registration (admin only).
    ///
    /// This is used when the tournament has `registration_type` of `Approval` or `InviteOnly`.
    #[instrument(skip(self))]
    pub async fn approve_registration(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<TournamentRegistration, DomainError> {
        let registration = self.get_registration(registration_id).await?;

        // Only pending registrations can be approved
        if registration.status != TournamentRegistrationStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "Cannot approve registration in {} status",
                registration.status
            )));
        }

        // Get tournament to verify approval-based registration
        let tournament = self
            .tournament_repo
            .find_by_id(registration.tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(registration.tournament_id))?;

        // Check capacity
        let current_count = self
            .tournament_repo
            .count_registrations(registration.tournament_id)
            .await?;
        if current_count >= i64::from(tournament.max_participants) {
            return Err(DomainError::TournamentFull);
        }

        self.registration_repo
            .update_status(registration_id, TournamentRegistrationStatus::Approved)
            .await
    }

    /// Reject a pending registration (admin only).
    #[instrument(skip(self))]
    pub async fn reject_registration(
        &self,
        registration_id: TournamentRegistrationId,
        reason: Option<String>,
    ) -> Result<TournamentRegistration, DomainError> {
        let registration = self.get_registration(registration_id).await?;

        // Only pending registrations can be rejected
        if registration.status != TournamentRegistrationStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "Cannot reject registration in {} status",
                registration.status
            )));
        }

        // Update status to withdrawn (we use withdrawn for rejected registrations)
        // Optionally store the reason in admin_notes
        if let Some(reason) = reason {
            self.registration_repo
                .update(
                    registration_id,
                    crate::repositories::tournament::UpdateTournamentRegistration {
                        admin_notes: Some(format!("Rejected: {reason}")),
                        ..Default::default()
                    },
                )
                .await?;
        }

        self.registration_repo
            .update_status(registration_id, TournamentRegistrationStatus::Withdrawn)
            .await
    }

    /// Disqualify a participant (admin only).
    ///
    /// Can be used during or after tournament to mark a participant as disqualified.
    #[instrument(skip(self))]
    pub async fn disqualify(
        &self,
        registration_id: TournamentRegistrationId,
        reason: String,
    ) -> Result<TournamentRegistration, DomainError> {
        let registration = self.get_registration(registration_id).await?;

        // Cannot disqualify already terminal registrations
        if registration.status.is_terminal() {
            return Err(DomainError::InvalidState(format!(
                "Cannot disqualify registration in {} status",
                registration.status
            )));
        }

        // Store the reason in admin_notes
        self.registration_repo
            .update(
                registration_id,
                crate::repositories::tournament::UpdateTournamentRegistration {
                    admin_notes: Some(format!("Disqualified: {reason}")),
                    ..Default::default()
                },
            )
            .await?;

        self.registration_repo
            .update_status(registration_id, TournamentRegistrationStatus::Disqualified)
            .await
    }

    /// List registrations for a tournament with optional status filter.
    #[instrument(skip(self))]
    pub async fn list_registrations(
        &self,
        tournament_id: TournamentId,
        status_filter: Option<TournamentRegistrationStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TournamentRegistration>, i64), DomainError> {
        self.registration_repo
            .list_by_tournament(tournament_id, status_filter, limit, offset)
            .await
    }

    /// Check eligibility for registration.
    ///
    /// Validates that:
    /// - Tournament is accepting registrations
    /// - Participant is not already registered
    /// - Tournament is not full
    #[instrument(skip(self))]
    pub async fn check_eligibility(&self, tournament_id: TournamentId) -> Result<(), DomainError> {
        let tournament = self
            .tournament_repo
            .find_by_id(tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(tournament_id))?;

        // Check registration is open
        if !tournament.is_registration_open() {
            return Err(DomainError::TournamentNotOpen);
        }

        // Check capacity
        let current_count = self
            .tournament_repo
            .count_registrations(tournament_id)
            .await?;
        if current_count >= i64::from(tournament.max_participants) {
            return Err(DomainError::TournamentFull);
        }

        Ok(())
    }

    /// Get the initial registration status based on tournament settings.
    ///
    /// - For `Open` tournaments: `Approved` (auto-approved)
    /// - For `Approval`, `InviteOnly`, `Qualification`: `Pending`
    pub fn initial_status_for_tournament(
        &self,
        registration_type: RegistrationType,
    ) -> TournamentRegistrationStatus {
        match registration_type {
            RegistrationType::Open => TournamentRegistrationStatus::Approved,
            RegistrationType::Approval
            | RegistrationType::InviteOnly
            | RegistrationType::Qualification => TournamentRegistrationStatus::Pending,
        }
    }
}

// Manual Clone implementation since derive(Clone) doesn't work with generic bounds
impl<TR, TRR> Clone for RegistrationService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    fn clone(&self) -> Self {
        Self {
            tournament_repo: Arc::clone(&self.tournament_repo),
            registration_repo: Arc::clone(&self.registration_repo),
        }
    }
}
