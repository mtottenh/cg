//! Match scheduling service.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{TournamentMatchId, UserId};
use portal_core::types::{ProposalStatus, TournamentMatchStatus};

use crate::entities::{
    AcceptProposalCommand, CounterProposeCommand, CreateScheduleProposalCommand,
    RejectProposalCommand, ScheduleProposal, TournamentMatch,
};
use crate::repositories::{
    ScheduleProposalRepository, TournamentMatchRepository, TournamentRegistrationRepository,
    UpdateTournamentMatch,
};

/// Default time-to-live for schedule proposals (48 hours).
const DEFAULT_PROPOSAL_TTL_HOURS: i64 = 48;

/// Service for managing match scheduling through proposals.
#[derive(Clone)]
pub struct SchedulingService<SPR, TMR, TRR>
where
    SPR: ScheduleProposalRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    proposal_repo: Arc<SPR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    proposal_ttl: Duration,
}

impl<SPR, TMR, TRR> SchedulingService<SPR, TMR, TRR>
where
    SPR: ScheduleProposalRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new scheduling service.
    pub fn new(
        proposal_repo: Arc<SPR>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
    ) -> Self {
        Self {
            proposal_repo,
            match_repo,
            registration_repo,
            proposal_ttl: Duration::hours(DEFAULT_PROPOSAL_TTL_HOURS),
        }
    }

    /// Create a new scheduling service with custom proposal TTL.
    pub fn with_proposal_ttl(mut self, ttl: Duration) -> Self {
        self.proposal_ttl = ttl;
        self
    }

    /// Create a new schedule proposal.
    ///
    /// # Errors
    /// - `MatchNotFound` if match doesn't exist
    /// - `NotAuthorized` if user is not part of the match
    /// - `InvalidState` if match cannot be scheduled
    /// - `InvalidState` if proposed times are invalid
    pub async fn propose_schedule(
        &self,
        match_id: TournamentMatchId,
        proposed_times: Vec<DateTime<Utc>>,
        proposed_by: UserId,
    ) -> Result<ScheduleProposal, DomainError> {
        // Validate match exists and can be scheduled
        let tournament_match = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id.to_string()))?;

        if !tournament_match.status.can_schedule() {
            return Err(DomainError::InvalidState(format!(
                "Match {} cannot be scheduled in status {:?}",
                match_id, tournament_match.status
            )));
        }

        // Validate proposed times
        if proposed_times.is_empty() || proposed_times.len() > 5 {
            return Err(DomainError::InvalidState(
                "Must propose 1-5 time slots".to_string(),
            ));
        }

        let now = Utc::now();
        for time in &proposed_times {
            if *time <= now {
                return Err(DomainError::InvalidState(
                    "Proposed times must be in the future".to_string(),
                ));
            }
        }

        // Check if there's already a pending proposal
        if let Some(existing) = self.proposal_repo.find_pending_by_match_id(match_id).await? {
            return Err(DomainError::Conflict(format!(
                "Match {} already has a pending proposal: {}",
                match_id, existing.id
            )));
        }

        // Find the registration for this user in this match
        let registration_id = self
            .find_user_registration_in_match(proposed_by, &tournament_match)
            .await?;

        let command = CreateScheduleProposalCommand {
            match_id,
            proposed_by_registration_id: registration_id,
            proposed_by_user_id: proposed_by,
            proposed_times,
            expires_at: now + self.proposal_ttl,
            notes: None,
        };

        self.proposal_repo.create(command).await
    }

    /// Accept a schedule proposal, selecting one of the proposed times.
    ///
    /// This updates the match to Scheduled status.
    pub async fn accept_proposal(
        &self,
        command: AcceptProposalCommand,
    ) -> Result<(ScheduleProposal, TournamentMatch), DomainError> {
        let mut proposal = self
            .proposal_repo
            .find_by_id(command.proposal_id)
            .await?
            .ok_or_else(|| {
                DomainError::not_found("ScheduleProposal", command.proposal_id.to_string())
            })?;

        // Check proposal can be responded to
        if !proposal.can_respond() {
            return Err(DomainError::InvalidState(format!(
                "Proposal {} cannot be responded to (status: {:?}, expired: {})",
                proposal.id,
                proposal.status,
                proposal.is_expired()
            )));
        }

        // Validate selected time is one of the proposed times
        if !proposal.contains_time(&command.selected_time) {
            return Err(DomainError::InvalidState(format!(
                "Selected time {:?} is not one of the proposed times",
                command.selected_time
            )));
        }

        // Validate responder is the opponent
        let tournament_match = self
            .match_repo
            .find_by_id(proposal.match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(proposal.match_id.to_string()))?;

        self.validate_responder_is_opponent(command.accepted_by_user_id, &proposal, &tournament_match)
            .await?;

        // Update proposal
        proposal.status = ProposalStatus::Accepted;
        proposal.selected_time = Some(command.selected_time);
        proposal.responded_at = Some(Utc::now());
        proposal.responded_by_user_id = Some(command.accepted_by_user_id);

        let updated_proposal = self.proposal_repo.update(&proposal).await?;

        // Update match: first set scheduled_at, then update status
        let update = UpdateTournamentMatch {
            scheduled_at: Some(command.selected_time),
            ..Default::default()
        };
        self.match_repo.update(proposal.match_id, update).await?;

        // Update status to Scheduled
        let updated_match = self
            .match_repo
            .update_status(proposal.match_id, TournamentMatchStatus::Scheduled)
            .await?;

        Ok((updated_proposal, updated_match))
    }

    /// Reject a schedule proposal.
    pub async fn reject_proposal(
        &self,
        command: RejectProposalCommand,
    ) -> Result<ScheduleProposal, DomainError> {
        let mut proposal = self
            .proposal_repo
            .find_by_id(command.proposal_id)
            .await?
            .ok_or_else(|| {
                DomainError::not_found("ScheduleProposal", command.proposal_id.to_string())
            })?;

        if !proposal.can_respond() {
            return Err(DomainError::InvalidState(format!(
                "Proposal {} cannot be rejected",
                proposal.id
            )));
        }

        // Validate responder is the opponent
        let tournament_match = self
            .match_repo
            .find_by_id(proposal.match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(proposal.match_id.to_string()))?;

        self.validate_responder_is_opponent(command.rejected_by_user_id, &proposal, &tournament_match)
            .await?;

        proposal.status = ProposalStatus::Rejected;
        proposal.responded_at = Some(Utc::now());
        proposal.responded_by_user_id = Some(command.rejected_by_user_id);

        self.proposal_repo.update(&proposal).await
    }

    /// Counter-propose with new times.
    ///
    /// Creates a new proposal and links it to the original.
    pub async fn counter_propose(
        &self,
        command: CounterProposeCommand,
    ) -> Result<ScheduleProposal, DomainError> {
        let mut original_proposal = self
            .proposal_repo
            .find_by_id(command.original_proposal_id)
            .await?
            .ok_or_else(|| {
                DomainError::not_found(
                    "ScheduleProposal",
                    command.original_proposal_id.to_string(),
                )
            })?;

        if !original_proposal.can_respond() {
            return Err(DomainError::InvalidState(format!(
                "Proposal {} cannot be counter-proposed",
                original_proposal.id
            )));
        }

        // Validate proposed times
        if command.proposed_times.is_empty() || command.proposed_times.len() > 5 {
            return Err(DomainError::InvalidState(
                "Must propose 1-5 time slots".to_string(),
            ));
        }

        let now = Utc::now();
        for time in &command.proposed_times {
            if *time <= now {
                return Err(DomainError::InvalidState(
                    "Proposed times must be in the future".to_string(),
                ));
            }
        }

        // Validate responder is the opponent
        let tournament_match = self
            .match_repo
            .find_by_id(command.match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(command.match_id.to_string()))?;

        self.validate_responder_is_opponent(
            command.proposed_by_user_id,
            &original_proposal,
            &tournament_match,
        )
        .await?;

        // Create counter-proposal
        let new_proposal = self
            .proposal_repo
            .create(CreateScheduleProposalCommand {
                match_id: command.match_id,
                proposed_by_registration_id: command.proposed_by_registration_id,
                proposed_by_user_id: command.proposed_by_user_id,
                proposed_times: command.proposed_times,
                expires_at: command.expires_at,
                notes: None,
            })
            .await?;

        // Mark original as counter-proposed
        original_proposal.status = ProposalStatus::CounterProposed;
        original_proposal.counter_proposal_id = Some(new_proposal.id);
        original_proposal.responded_at = Some(Utc::now());
        original_proposal.responded_by_user_id = Some(command.proposed_by_user_id);
        self.proposal_repo.update(&original_proposal).await?;

        Ok(new_proposal)
    }

    /// Admin directly schedules a match.
    ///
    /// Bypasses the proposal workflow entirely.
    pub async fn admin_schedule(
        &self,
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
        _admin_id: UserId,
    ) -> Result<TournamentMatch, DomainError> {
        let tournament_match = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id.to_string()))?;

        if !tournament_match.status.can_schedule() {
            return Err(DomainError::InvalidState(format!(
                "Match {} cannot be scheduled in status {:?}",
                match_id, tournament_match.status
            )));
        }

        // Cancel any pending proposals
        if let Some(pending) = self.proposal_repo.find_pending_by_match_id(match_id).await? {
            let mut cancelled = pending;
            cancelled.status = ProposalStatus::Cancelled;
            self.proposal_repo.update(&cancelled).await?;
        }

        // Update scheduled_at first
        let update = UpdateTournamentMatch {
            scheduled_at: Some(scheduled_at),
            ..Default::default()
        };
        self.match_repo.update(match_id, update).await?;

        // Then update status
        self.match_repo
            .update_status(match_id, TournamentMatchStatus::Scheduled)
            .await
    }

    /// Expire pending proposals that have passed their deadline.
    ///
    /// Called by background job.
    pub async fn expire_proposals(&self) -> Result<Vec<ScheduleProposal>, DomainError> {
        let now = Utc::now();
        let expired = self.proposal_repo.find_expired(now).await?;

        let mut results = Vec::new();
        for proposal in expired {
            let updated = self.proposal_repo.mark_expired(proposal.id).await?;
            results.push(updated);
        }

        Ok(results)
    }

    /// Get active proposal for a match.
    pub async fn get_active_proposal(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ScheduleProposal>, DomainError> {
        self.proposal_repo.find_pending_by_match_id(match_id).await
    }

    /// Get proposal history for a match.
    pub async fn get_proposal_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ScheduleProposal>, DomainError> {
        self.proposal_repo.find_by_match_id(match_id).await
    }

    /// Get a match by ID.
    pub async fn get_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<TournamentMatch>, DomainError> {
        self.match_repo.find_by_id(match_id).await
    }

    /// Find the user's registration in a match.
    async fn find_user_registration_in_match(
        &self,
        user_id: UserId,
        tournament_match: &TournamentMatch,
    ) -> Result<portal_core::ids::TournamentRegistrationId, DomainError> {
        // Check participant 1
        if let Some(reg_id) = tournament_match.participant1_registration_id {
            if let Some(reg) = self.registration_repo.find_by_id(reg_id).await? {
                if reg.registered_by == user_id {
                    return Ok(reg_id);
                }
            }
        }

        // Check participant 2
        if let Some(reg_id) = tournament_match.participant2_registration_id {
            if let Some(reg) = self.registration_repo.find_by_id(reg_id).await? {
                if reg.registered_by == user_id {
                    return Ok(reg_id);
                }
            }
        }

        Err(DomainError::NotAuthorized(format!(
            "User {} is not a participant in match {}",
            user_id, tournament_match.id
        )))
    }

    /// Validate that the responder is the opponent (not the proposer).
    async fn validate_responder_is_opponent(
        &self,
        responder_id: UserId,
        proposal: &ScheduleProposal,
        tournament_match: &TournamentMatch,
    ) -> Result<(), DomainError> {
        // Responder cannot be the proposer
        if responder_id == proposal.proposed_by_user_id {
            return Err(DomainError::NotAuthorized(
                "Cannot respond to your own proposal".to_string(),
            ));
        }

        // Responder must be a participant
        self.find_user_registration_in_match(responder_id, tournament_match)
            .await?;

        Ok(())
    }
}
