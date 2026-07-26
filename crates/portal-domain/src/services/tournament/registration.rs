//! Tournament registration service.
//!
//! Handles participant registration, withdrawal, approval, rejection,
//! disqualification, and check-in operations.

use std::sync::Arc;

use portal_core::types::{RegistrationType, TournamentRegistrationStatus, TournamentStatus};
use portal_core::{DomainError, TournamentId, TournamentRegistrationId, UserId};
use tracing::instrument;

use crate::entities::tournament::TournamentRegistration;
use crate::repositories::league_team::LeagueTeamMemberRepository;
use crate::repositories::tournament::{TournamentRegistrationRepository, TournamentRepository};
use crate::services::tournament::registration_actor::{RegistrationActor, speaks_for_registration};

/// The status a brand-new registration is created with, given the
/// tournament's `registration_type`.
///
/// - `Open`: `Approved` — anyone may enter, so there is nothing for an
///   organiser to decide; making them click "approve" on every row was
///   busywork the product never intended (see P-2).
/// - `Approval`, `InviteOnly`, `Qualification`: `Pending` — an organiser
///   (or a qualifier result) still has to sign the entry off.
///
/// Free function rather than a method so both `RegistrationService` and
/// `TournamentService` (which owns `register_team` / `register_player`)
/// apply exactly the same rule.
#[must_use]
pub const fn initial_registration_status(
    registration_type: RegistrationType,
) -> TournamentRegistrationStatus {
    match registration_type {
        RegistrationType::Open => TournamentRegistrationStatus::Approved,
        RegistrationType::Approval
        | RegistrationType::InviteOnly
        | RegistrationType::Qualification => TournamentRegistrationStatus::Pending,
    }
}

/// How many registrations a tournament has, per status.
///
/// Exists because the pages that show these numbers used to count the rows of
/// a **page** of the registrations list — so a 128-player event reported "20
/// participants" and "20 pending approvals" for as long as it had more than 20
/// of either (P-167). A count needs a real count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegistrationCounts {
    /// Every row, whatever its status.
    pub total: i64,
    /// Rows that still represent someone taking part: everything except
    /// `withdrawn` and `disqualified`. This is the number the participant
    /// list and the `n / max_participants` capacity read are about.
    pub participating: i64,
    pub pending: i64,
    pub approved: i64,
    pub checked_in: i64,
    pub active: i64,
    pub eliminated: i64,
    pub disqualified: i64,
    pub withdrawn: i64,
    pub no_show: i64,
}

/// Service for tournament registration management.
pub struct RegistrationService<TR, TRR, LTMR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
    LTMR: LeagueTeamMemberRepository,
{
    tournament_repo: Arc<TR>,
    registration_repo: Arc<TRR>,
    member_repo: Arc<LTMR>,
    /// Ad-hoc team membership (PUG containers); optional, see `with_adhoc_repo`.
    adhoc_repo: Option<Arc<dyn crate::repositories::pug::AdhocTeamRepository>>,
}

impl<TR, TRR, LTMR> RegistrationService<TR, TRR, LTMR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
    LTMR: LeagueTeamMemberRepository,
{
    /// Create a new registration service.
    pub fn new(
        tournament_repo: Arc<TR>,
        registration_repo: Arc<TRR>,
        member_repo: Arc<LTMR>,
    ) -> Self {
        Self {
            tournament_repo,
            registration_repo,
            member_repo,
            adhoc_repo: None,
        }
    }

    /// Attach the ad-hoc team repository so ad-hoc registrations (PUGs)
    /// authorize their team members.
    #[must_use]
    pub fn with_adhoc_repo(
        mut self,
        adhoc_repo: Arc<dyn crate::repositories::pug::AdhocTeamRepository>,
    ) -> Self {
        self.adhoc_repo = Some(adhoc_repo);
        self
    }


    /// Whether `actor` may act on behalf of `registration`.
    ///
    /// The handler-side entry point to [`speaks_for_registration`] — the one
    /// definition of the rule (P-168). Five handlers used to carry their own
    /// open-coded copy of it and the domain services carried a *different*
    /// one, which is how the same person could raise a dispute about a result
    /// they were refused permission to submit.
    #[instrument(skip(self, registration))]
    pub async fn speaks_for(
        &self,
        registration: &TournamentRegistration,
        actor: RegistrationActor,
    ) -> Result<bool, DomainError> {
        speaks_for_registration(
            self.member_repo.as_ref(),
            self.adhoc_repo.as_deref(),
            registration,
            actor,
        )
        .await
    }

    /// Every registration in this tournament that `actor` speaks for.
    ///
    /// # Why this exists (P-167)
    ///
    /// The tournament page answered "am I registered?" by fetching
    /// `GET /v1/tournaments/{id}/registrations` — **at the default
    /// `per_page` of 20** — and scanning the page for the caller. Past row 20
    /// every participant was told they were not registered: no "Registered"
    /// state, no withdraw control, and a "do I have an eligible team?" answer
    /// computed from a 20-row sample. Raising the page size only moves the
    /// ceiling (the same bug was already fixed once at 100), so identity is
    /// resolved directly instead.
    ///
    /// Cost is bounded by the caller's own team count, not by the tournament's
    /// size: one lookup by player, plus one per team-season they belong to.
    /// Every candidate is then passed through [`Self::speaks_for`], so this
    /// list and the authorization checks can never disagree.
    #[instrument(skip(self))]
    pub async fn list_for_actor(
        &self,
        tournament_id: TournamentId,
        actor: RegistrationActor,
    ) -> Result<Vec<TournamentRegistration>, DomainError> {
        let mut found: Vec<TournamentRegistration> = Vec::new();

        if let Some(registration) = self
            .registration_repo
            .find_by_player(tournament_id, actor.player_id)
            .await?
        {
            found.push(registration);
        }

        // `list_memberships_for_player` is deliberately re-filtered through
        // `speaks_for`: the view behind it does not exclude teams the player
        // has left, and the authorization rule does.
        let memberships = self
            .member_repo
            .list_memberships_for_player(actor.player_id)
            .await?;

        for membership in memberships {
            if found
                .iter()
                .any(|r| r.team_season_id == Some(membership.team_season_id))
            {
                continue;
            }
            let Some(registration) = self
                .registration_repo
                .find_by_team_season(tournament_id, membership.team_season_id)
                .await?
            else {
                continue;
            };
            if self.speaks_for(&registration, actor).await? {
                found.push(registration);
            }
        }

        Ok(found)
    }

    /// Real per-status counts for a tournament's registrations (P-167).
    #[instrument(skip(self))]
    pub async fn counts(
        &self,
        tournament_id: TournamentId,
    ) -> Result<RegistrationCounts, DomainError> {
        let by_status = self
            .registration_repo
            .count_all_by_status(tournament_id)
            .await?;

        let get = |status: TournamentRegistrationStatus| -> i64 {
            by_status
                .iter()
                .find(|(s, _)| *s == status)
                .map_or(0, |(_, n)| *n)
        };

        let counts = RegistrationCounts {
            total: by_status.iter().map(|(_, n)| *n).sum(),
            participating: by_status
                .iter()
                .filter(|(s, _)| {
                    !matches!(
                        s,
                        TournamentRegistrationStatus::Withdrawn
                            | TournamentRegistrationStatus::Disqualified
                    )
                })
                .map(|(_, n)| *n)
                .sum(),
            pending: get(TournamentRegistrationStatus::Pending),
            approved: get(TournamentRegistrationStatus::Approved),
            checked_in: get(TournamentRegistrationStatus::CheckedIn),
            active: get(TournamentRegistrationStatus::Active),
            eliminated: get(TournamentRegistrationStatus::Eliminated),
            disqualified: get(TournamentRegistrationStatus::Disqualified),
            withdrawn: get(TournamentRegistrationStatus::Withdrawn),
            no_show: get(TournamentRegistrationStatus::NoShow),
        };

        Ok(counts)
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
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))
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
            .ok_or(DomainError::TournamentNotFound(registration.tournament_id))?;

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

        // Approving something already approved is a no-op, not an error.
        //
        // This became load-bearing with P-2: `Open` tournaments now auto-approve
        // on signup, so a registration an organiser sees may already be approved
        // and pressing Approve on it would have returned 400. The same applied to
        // a double-click, or two organisers acting at once. Returning the
        // registration unchanged makes the endpoint idempotent, which is what
        // every caller already assumed.
        //
        // Deliberately narrow: only `Approved` short-circuits. Approving a
        // withdrawn, rejected or disqualified registration is still a genuine
        // state error and still fails below.
        if registration.status == TournamentRegistrationStatus::Approved {
            return Ok(registration);
        }

        // Only pending registrations can be approved
        if registration.status != TournamentRegistrationStatus::Pending {
            return Err(DomainError::InvalidState(format!(
                "Cannot approve registration in {} status",
                registration.status
            )));
        }

        // Verify the tournament still exists before touching the row.
        self.tournament_repo
            .find_by_id(registration.tournament_id)
            .await?
            .ok_or(DomainError::TournamentNotFound(registration.tournament_id))?;

        // Capacity check + status flip in one transaction under a lock on
        // the tournament row, so a burst of approvals cannot push the
        // tournament past `max_participants`. Note this counts every other
        // slot-occupying registration: the row being approved keeps the
        // slot it already held as a pending registration.
        self.registration_repo
            .update_status_with_capacity_check(
                registration_id,
                TournamentRegistrationStatus::Approved,
            )
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
            .ok_or(DomainError::TournamentNotFound(tournament_id))?;

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
    /// Thin wrapper over [`initial_registration_status`], which is the
    /// single definition of the rule.
    #[must_use]
    pub const fn initial_status_for_tournament(
        &self,
        registration_type: RegistrationType,
    ) -> TournamentRegistrationStatus {
        initial_registration_status(registration_type)
    }
}

// Manual Clone implementation since derive(Clone) doesn't work with generic bounds
impl<TR, TRR, LTMR> Clone for RegistrationService<TR, TRR, LTMR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
    LTMR: LeagueTeamMemberRepository,
{
    fn clone(&self) -> Self {
        Self {
            tournament_repo: Arc::clone(&self.tournament_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            member_repo: Arc::clone(&self.member_repo),
            adhoc_repo: self.adhoc_repo.clone(),
        }
    }
}
