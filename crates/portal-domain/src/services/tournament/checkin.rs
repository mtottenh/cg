//! Tournament check-in service.
//!
//! Handles participant check-in operations before tournament start.

use std::sync::Arc;

use portal_core::types::TournamentRegistrationStatus;
use portal_core::{DomainError, TournamentId, TournamentRegistrationId, UserId};
use tracing::{info, instrument};

use crate::entities::tournament::TournamentRegistration;
use crate::repositories::tournament::{TournamentRegistrationRepository, TournamentRepository};

/// Service for tournament check-in management.
pub struct CheckInService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    tournament_repo: Arc<TR>,
    registration_repo: Arc<TRR>,
}

impl<TR, TRR> CheckInService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new check-in service.
    pub const fn new(tournament_repo: Arc<TR>, registration_repo: Arc<TRR>) -> Self {
        Self {
            tournament_repo,
            registration_repo,
        }
    }

    /// Check in a participant.
    ///
    /// The participant must be in `Approved` status and check-in must be open.
    #[instrument(skip(self))]
    pub async fn check_in(
        &self,
        registration_id: TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentRegistration, DomainError> {
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or_else(|| {
                DomainError::TournamentRegistrationNotFound(registration_id.to_string())
            })?;

        // Check if already checked in
        if registration.checked_in {
            return Err(DomainError::Conflict("Already checked in".to_string()));
        }

        // Check if registration can check in
        if !registration.status.can_check_in() {
            return Err(DomainError::InvalidState(format!(
                "Cannot check in registration in {} status",
                registration.status
            )));
        }

        // Get tournament to verify check-in window
        let tournament = self
            .tournament_repo
            .find_by_id(registration.tournament_id)
            .await?
            .ok_or_else(|| {
                DomainError::TournamentNotFound(registration.tournament_id.to_string())
            })?;

        // Check if check-in is open (or not required)
        if tournament.check_in_required && !tournament.is_check_in_open() {
            return Err(DomainError::InvalidState(
                "Check-in is not currently open".to_string(),
            ));
        }

        info!(
            registration_id = %registration_id,
            checked_in_by = %checked_in_by,
            "Participant checking in"
        );

        self.registration_repo
            .check_in(registration_id, checked_in_by)
            .await
    }

    /// Admin check-in (bypasses check-in window validation).
    ///
    /// Allows admins to manually check in participants regardless of the check-in window.
    #[instrument(skip(self))]
    pub async fn admin_check_in(
        &self,
        registration_id: TournamentRegistrationId,
        admin_user_id: UserId,
    ) -> Result<TournamentRegistration, DomainError> {
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or_else(|| {
                DomainError::TournamentRegistrationNotFound(registration_id.to_string())
            })?;

        // Check if already checked in
        if registration.checked_in {
            return Err(DomainError::Conflict("Already checked in".to_string()));
        }

        // For admin check-in, we allow checking in participants in Approved or Pending status
        if registration.status != TournamentRegistrationStatus::Approved
            && registration.status != TournamentRegistrationStatus::Pending
        {
            return Err(DomainError::InvalidState(format!(
                "Cannot check in registration in {} status",
                registration.status
            )));
        }

        info!(
            registration_id = %registration_id,
            admin_user_id = %admin_user_id,
            "Admin checking in participant"
        );

        // If the registration is pending, approve it first
        if registration.status == TournamentRegistrationStatus::Pending {
            self.registration_repo
                .update_status(registration_id, TournamentRegistrationStatus::Approved)
                .await?;
        }

        self.registration_repo
            .check_in(registration_id, admin_user_id)
            .await
    }

    /// Process no-shows - mark all participants who haven't checked in as no-shows.
    ///
    /// This should be called after check-in closes but before the tournament starts.
    #[instrument(skip(self))]
    pub async fn process_no_shows(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentRegistration>, DomainError> {
        // Verify tournament exists
        let tournament = self
            .tournament_repo
            .find_by_id(tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(tournament_id.to_string()))?;

        // Check-in must be required for this operation
        if !tournament.check_in_required {
            return Err(DomainError::InvalidState(
                "Tournament does not require check-in".to_string(),
            ));
        }

        // Get all approved (but not checked-in) registrations
        let (registrations, _) = self
            .registration_repo
            .list_by_tournament(
                tournament_id,
                Some(TournamentRegistrationStatus::Approved),
                1000, // Get all (reasonable limit)
                0,
            )
            .await?;

        let mut no_shows = Vec::new();

        for registration in registrations {
            // Skip if already checked in (this shouldn't happen with status filter, but be safe)
            if registration.checked_in {
                continue;
            }

            info!(
                registration_id = %registration.id,
                participant_name = %registration.participant_name,
                "Marking participant as no-show"
            );

            let updated = self
                .registration_repo
                .update_status(registration.id, TournamentRegistrationStatus::NoShow)
                .await?;

            no_shows.push(updated);
        }

        info!(
            tournament_id = %tournament_id,
            no_show_count = no_shows.len(),
            "Processed no-shows"
        );

        Ok(no_shows)
    }

    /// Get check-in status for a tournament.
    ///
    /// Returns counts of checked-in vs total approved participants.
    #[instrument(skip(self))]
    pub async fn get_check_in_status(
        &self,
        tournament_id: TournamentId,
    ) -> Result<CheckInStatus, DomainError> {
        // Verify tournament exists
        let tournament = self
            .tournament_repo
            .find_by_id(tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(tournament_id.to_string()))?;

        let checked_in_count = self
            .registration_repo
            .count_by_status(tournament_id, TournamentRegistrationStatus::CheckedIn)
            .await?;

        let approved_count = self
            .registration_repo
            .count_by_status(tournament_id, TournamentRegistrationStatus::Approved)
            .await?;

        let total_eligible = checked_in_count + approved_count;

        Ok(CheckInStatus {
            tournament_id,
            check_in_required: tournament.check_in_required,
            check_in_open: tournament.is_check_in_open(),
            checked_in_count,
            total_eligible,
        })
    }
}

/// Check-in status summary for a tournament.
#[derive(Debug, Clone)]
pub struct CheckInStatus {
    /// Tournament ID.
    pub tournament_id: TournamentId,
    /// Whether check-in is required.
    pub check_in_required: bool,
    /// Whether check-in window is currently open.
    pub check_in_open: bool,
    /// Number of participants who have checked in.
    pub checked_in_count: i64,
    /// Total eligible participants (approved + checked_in).
    pub total_eligible: i64,
}

// Manual Clone implementation
impl<TR, TRR> Clone for CheckInService<TR, TRR>
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
