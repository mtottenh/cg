//! Dispute service.
//!
//! Handles match result disputes and admin resolution workflow.

use std::sync::Arc;

use portal_core::types::TournamentMatchStatus;
use portal_core::{
    DisputeId, DomainError, EvidenceId, ResultClaimId, TournamentId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use tracing::{info, instrument};

use crate::entities::dispute::{
    AuthorType, Dispute, DisputeMessage, DisputePriority, DisputeReason, DisputeResolution,
    DisputeResolutionResult, DisputeStatus, DisputeWithThread, ProgressionChanges, ResolutionType,
};
use crate::entities::tournament::TournamentMatch;
use crate::repositories::dispute::{
    CreateDispute, CreateDisputeMessage, DisputeMessageRepository, DisputeRepository, UpdateDispute,
};
use crate::repositories::tournament::{ResultClaimRepository, TournamentMatchRepository};

/// Service for handling disputes.
#[derive(Clone)]
pub struct DisputeService<DR, DMR, TMR, RCR> {
    dispute_repo: Arc<DR>,
    message_repo: Arc<DMR>,
    match_repo: Arc<TMR>,
    claim_repo: Arc<RCR>,
}

impl<DR, DMR, TMR, RCR> DisputeService<DR, DMR, TMR, RCR>
where
    DR: DisputeRepository,
    DMR: DisputeMessageRepository,
    TMR: TournamentMatchRepository,
    RCR: ResultClaimRepository,
{
    /// Create a new dispute service.
    pub fn new(
        dispute_repo: Arc<DR>,
        message_repo: Arc<DMR>,
        match_repo: Arc<TMR>,
        claim_repo: Arc<RCR>,
    ) -> Self {
        Self {
            dispute_repo,
            message_repo,
            match_repo,
            claim_repo,
        }
    }

    /// Raise a dispute against a match result.
    #[instrument(skip(self, description, evidence_ids))]
    pub async fn raise_dispute(
        &self,
        match_id: TournamentMatchId,
        result_claim_id: Option<ResultClaimId>,
        reason: DisputeReason,
        description: String,
        evidence_ids: Vec<EvidenceId>,
        disputed_by_registration_id: TournamentRegistrationId,
        disputed_by_user_id: UserId,
    ) -> Result<Dispute, DomainError> {
        // Get the match
        let match_ = self.get_match(match_id).await?;

        // Validate the match can be disputed
        self.validate_can_dispute(&match_, disputed_by_registration_id)?;

        // Check if there's already a pending dispute
        if self.dispute_repo.exists_pending_for_match(match_id).await? {
            return Err(DomainError::InvalidState(format!(
                "Match {match_id} already has a pending dispute"
            )));
        }

        // If disputing a specific claim, verify it's not the submitter's own claim
        if let Some(claim_id) = result_claim_id
            && let Some(claim) = self.claim_repo.find_by_id(claim_id).await?
            && claim.submitted_by_registration_id == disputed_by_registration_id
        {
            return Err(DomainError::InvalidState(
                "Cannot dispute your own result claim".to_string(),
            ));
        }

        // Determine priority based on reason
        let priority = match reason {
            DisputeReason::Cheating => DisputePriority::Urgent,
            DisputeReason::RuleViolation | DisputeReason::PlayerMisconduct => DisputePriority::High,
            _ => DisputePriority::Normal,
        };

        // Compute the system-message summary snippet before moving
        // `description` into the create command (byte-safe truncation
        // via `.chars().take(100)` — the old slice form could panic on
        // a multi-byte boundary).
        let summary_snippet: String = description.chars().take(100).collect();

        // Atomic: create dispute + flip match to Disputed + append the
        // initial system message. Previously these three writes ran
        // sequentially without a transaction; partial failure could
        // leave a dispute without a Disputed match (allowing concurrent
        // result submissions), a Disputed match with no dispute row
        // (invisible to the admin queue), or a dispute + status with
        // no opening message. See audit I5. The adapter rewrites
        // `initial_message.dispute_id` with the just-inserted id before
        // the message INSERT runs, so the placeholder nil id below is
        // only a type-level filler.
        let dispute = self
            .dispute_repo
            .raise_atomic(
                CreateDispute {
                    match_id,
                    result_claim_id,
                    disputed_by_registration_id,
                    disputed_by_user_id,
                    reason,
                    description,
                    evidence_ids,
                    original_winner_registration_id: match_.winner_registration_id,
                    original_participant1_score: Some(match_.participant1_score),
                    original_participant2_score: Some(match_.participant2_score),
                    priority,
                },
                CreateDisputeMessage {
                    dispute_id: DisputeId::from(uuid::Uuid::nil()),
                    author_user_id: disputed_by_user_id,
                    author_type: AuthorType::System,
                    message: format!("Dispute raised: {reason} - {summary_snippet}"),
                    evidence_ids: Vec::new(),
                    is_internal: false,
                },
            )
            .await?;

        info!(
            dispute_id = %dispute.id,
            match_id = %match_id,
            reason = %reason,
            "Dispute raised"
        );

        Ok(dispute)
    }

    /// Add a message to a dispute thread.
    #[instrument(skip(self, message, evidence_ids))]
    pub async fn add_message(
        &self,
        dispute_id: DisputeId,
        message: String,
        evidence_ids: Vec<EvidenceId>,
        author_user_id: UserId,
        author_type: AuthorType,
        is_internal: bool,
    ) -> Result<DisputeMessage, DomainError> {
        // Verify dispute exists and is not resolved
        let dispute = self.get_dispute(dispute_id).await?;
        if dispute.is_terminal() {
            return Err(DomainError::InvalidState(format!(
                "Dispute {} is already {}",
                dispute_id, dispute.status
            )));
        }

        let dispute_message = self
            .message_repo
            .create(CreateDisputeMessage {
                dispute_id,
                author_user_id,
                author_type,
                message,
                evidence_ids,
                is_internal,
            })
            .await?;

        info!(
            dispute_id = %dispute_id,
            message_id = %dispute_message.id,
            is_internal = is_internal,
            "Added dispute message"
        );

        Ok(dispute_message)
    }

    /// Assign a dispute for review (admin takes ownership).
    #[instrument(skip(self))]
    pub async fn assign_for_review(
        &self,
        dispute_id: DisputeId,
        assigned_by: UserId,
    ) -> Result<Dispute, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;

        if !dispute.can_assign() {
            return Err(DomainError::InvalidState(format!(
                "Dispute {} cannot be assigned (status: {})",
                dispute_id, dispute.status
            )));
        }

        let updated = self
            .dispute_repo
            .update(
                dispute_id,
                UpdateDispute {
                    status: Some(DisputeStatus::UnderReview),
                    ..Default::default()
                },
            )
            .await?;

        // Add system message
        self.message_repo
            .create(CreateDisputeMessage {
                dispute_id,
                author_user_id: assigned_by,
                author_type: AuthorType::System,
                message: "Dispute assigned for review".to_string(),
                evidence_ids: Vec::new(),
                is_internal: true,
            })
            .await?;

        info!(
            dispute_id = %dispute_id,
            assigned_by = %assigned_by,
            "Dispute assigned for review"
        );

        Ok(updated)
    }

    /// Resolve dispute with uphold (original result stands).
    #[instrument(skip(self, notes))]
    pub async fn resolve_uphold(
        &self,
        dispute_id: DisputeId,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;
        self.validate_can_resolve(&dispute)?;

        let resolution = DisputeResolution {
            resolution_type: ResolutionType::Upheld,
            notes: notes.clone(),
            new_winner_registration_id: None,
            new_participant1_score: None,
            new_participant2_score: None,
        };

        // Atomic: resolve dispute + restore match to Completed +
        // append resolution message. See audit I5.
        let resolved = self
            .dispute_repo
            .resolve_with_status_change(
                dispute_id,
                resolved_by,
                resolution,
                dispute.match_id,
                TournamentMatchStatus::Completed,
                CreateDisputeMessage {
                    dispute_id,
                    author_user_id: resolved_by,
                    author_type: AuthorType::Admin,
                    message: format!("Dispute upheld: {notes}"),
                    evidence_ids: Vec::new(),
                    is_internal: false,
                },
            )
            .await?;

        info!(
            dispute_id = %dispute_id,
            resolution = "upheld",
            "Dispute resolved"
        );

        Ok(DisputeResolutionResult {
            dispute: resolved,
            progression_changes: None,
        })
    }

    /// Resolve dispute by overturning result.
    #[instrument(skip(self, notes))]
    pub async fn resolve_overturn(
        &self,
        dispute_id: DisputeId,
        new_winner_registration_id: TournamentRegistrationId,
        new_participant1_score: i32,
        new_participant2_score: i32,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;
        self.validate_can_resolve(&dispute)?;

        // Resolve the loser *before* the atomic call since it requires
        // a fresh read of the match row. The actual match mutation
        // happens inside `resolve_with_overturn`'s transaction.
        let match_ = self.get_match(dispute.match_id).await?;
        let loser_id = if match_.participant1_registration_id == Some(new_winner_registration_id) {
            match_.participant2_registration_id
        } else {
            match_.participant1_registration_id
        }
        .ok_or_else(|| DomainError::InvalidState("Cannot determine loser".to_string()))?;

        let resolution = DisputeResolution {
            resolution_type: ResolutionType::Overturned,
            notes: notes.clone(),
            new_winner_registration_id: Some(new_winner_registration_id),
            new_participant1_score: Some(new_participant1_score),
            new_participant2_score: Some(new_participant2_score),
        };

        // Atomic: flip dispute to Resolved + overwrite match result +
        // append admin resolution message. The old four-call chain
        // (resolve → submit_result → update_status → message_create)
        // could leave an overturned dispute whose match still showed
        // the disputed result, and the bracket progression saga would
        // advance the wrong winner. See audit I5.
        let resolved = self
            .dispute_repo
            .resolve_with_overturn(
                dispute_id,
                resolved_by,
                resolution,
                dispute.match_id,
                new_winner_registration_id,
                loser_id,
                new_participant1_score,
                new_participant2_score,
                CreateDisputeMessage {
                    dispute_id,
                    author_user_id: resolved_by,
                    author_type: AuthorType::Admin,
                    message: format!(
                        "Dispute overturned. New scores: {new_participant1_score}-{new_participant2_score}. {notes}"
                    ),
                    evidence_ids: Vec::new(),
                    is_internal: false,
                },
            )
            .await?;

        info!(
            dispute_id = %dispute_id,
            resolution = "overturned",
            new_winner = %new_winner_registration_id,
            "Dispute resolved with overturn"
        );

        // Note: In a full implementation, we would handle progression reversal here
        Ok(DisputeResolutionResult {
            dispute: resolved,
            progression_changes: Some(ProgressionChanges {
                reverted_matches: Vec::new(),
                updated_matches: vec![dispute.match_id],
                new_winner_path: Vec::new(),
            }),
        })
    }

    /// Resolve dispute by ordering a rematch.
    #[instrument(skip(self, notes))]
    pub async fn resolve_rematch(
        &self,
        dispute_id: DisputeId,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;
        self.validate_can_resolve(&dispute)?;

        let resolution = DisputeResolution {
            resolution_type: ResolutionType::Rematch,
            notes: notes.clone(),
            new_winner_registration_id: None,
            new_participant1_score: None,
            new_participant2_score: None,
        };

        // Atomic: resolve dispute + reset match to Ready + append
        // resolution message. See audit I5.
        let resolved = self
            .dispute_repo
            .resolve_with_status_change(
                dispute_id,
                resolved_by,
                resolution,
                dispute.match_id,
                TournamentMatchStatus::Ready,
                CreateDisputeMessage {
                    dispute_id,
                    author_user_id: resolved_by,
                    author_type: AuthorType::Admin,
                    message: format!("Rematch ordered: {notes}"),
                    evidence_ids: Vec::new(),
                    is_internal: false,
                },
            )
            .await?;

        info!(
            dispute_id = %dispute_id,
            resolution = "rematch",
            "Dispute resolved with rematch"
        );

        Ok(DisputeResolutionResult {
            dispute: resolved,
            progression_changes: None,
        })
    }

    /// Resolve dispute with adjusted scores.
    #[instrument(skip(self, notes))]
    pub async fn resolve_adjusted(
        &self,
        dispute_id: DisputeId,
        new_participant1_score: i32,
        new_participant2_score: i32,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;
        self.validate_can_resolve(&dispute)?;

        let match_ = self.get_match(dispute.match_id).await?;

        // Determine winner based on new scores
        let new_winner_id = if new_participant1_score > new_participant2_score {
            match_.participant1_registration_id
        } else {
            match_.participant2_registration_id
        }
        .ok_or_else(|| DomainError::InvalidState("Cannot determine winner".to_string()))?;

        let loser_id = if match_.participant1_registration_id == Some(new_winner_id) {
            match_.participant2_registration_id
        } else {
            match_.participant1_registration_id
        }
        .ok_or_else(|| DomainError::InvalidState("Cannot determine loser".to_string()))?;

        let resolution = DisputeResolution {
            resolution_type: ResolutionType::Adjusted,
            notes: notes.clone(),
            new_winner_registration_id: Some(new_winner_id),
            new_participant1_score: Some(new_participant1_score),
            new_participant2_score: Some(new_participant2_score),
        };

        // Atomic: resolve dispute + overwrite match result + append
        // message. Reuses the same repo path as `resolve_overturn`
        // because the writes are structurally identical — only the
        // `resolution.resolution_type` differs. The trailing
        // `update_status(Completed)` that used to follow was redundant
        // (submit_result already sets status=completed); dropped as
        // part of the same audit I5 cleanup.
        let resolved = self
            .dispute_repo
            .resolve_with_overturn(
                dispute_id,
                resolved_by,
                resolution,
                dispute.match_id,
                new_winner_id,
                loser_id,
                new_participant1_score,
                new_participant2_score,
                CreateDisputeMessage {
                    dispute_id,
                    author_user_id: resolved_by,
                    author_type: AuthorType::Admin,
                    message: format!("Scores adjusted to {new_participant1_score}-{new_participant2_score}. {notes}"),
                    evidence_ids: Vec::new(),
                    is_internal: false,
                },
            )
            .await?;

        info!(
            dispute_id = %dispute_id,
            resolution = "adjusted",
            "Dispute resolved with adjusted scores"
        );

        Ok(DisputeResolutionResult {
            dispute: resolved,
            progression_changes: None,
        })
    }

    /// Resolve dispute by disqualifying both teams.
    #[instrument(skip(self, notes))]
    pub async fn resolve_double_dq(
        &self,
        dispute_id: DisputeId,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;
        self.validate_can_resolve(&dispute)?;

        let resolution = DisputeResolution {
            resolution_type: ResolutionType::DoubleDq,
            notes: notes.clone(),
            new_winner_registration_id: None,
            new_participant1_score: None,
            new_participant2_score: None,
        };

        // Atomic: resolve dispute + cancel match + append resolution
        // message. See audit I5.
        let resolved = self
            .dispute_repo
            .resolve_with_status_change(
                dispute_id,
                resolved_by,
                resolution,
                dispute.match_id,
                TournamentMatchStatus::Cancelled,
                CreateDisputeMessage {
                    dispute_id,
                    author_user_id: resolved_by,
                    author_type: AuthorType::Admin,
                    message: format!("Both teams disqualified: {notes}"),
                    evidence_ids: Vec::new(),
                    is_internal: false,
                },
            )
            .await?;

        info!(
            dispute_id = %dispute_id,
            resolution = "double_dq",
            "Dispute resolved with double DQ"
        );

        Ok(DisputeResolutionResult {
            dispute: resolved,
            progression_changes: None,
        })
    }

    /// Get the active dispute for a match (if any).
    ///
    /// Returns the most recent non-resolved dispute for the given match,
    /// or None if no active dispute exists.
    #[instrument(skip(self))]
    pub async fn get_match_dispute(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<Dispute>, DomainError> {
        self.dispute_repo.find_pending_by_match(match_id).await
    }

    /// Get pending disputes (admin queue).
    #[instrument(skip(self))]
    pub async fn get_pending_disputes(
        &self,
        tournament_id: Option<TournamentId>,
        priority: Option<DisputePriority>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Dispute>, i64), DomainError> {
        self.dispute_repo
            .find_pending(tournament_id, priority, limit, offset)
            .await
    }

    /// List disputes with optional filters; `status: None` means all
    /// statuses, so resolved and cancelled disputes remain reachable from
    /// the admin queue.
    #[instrument(skip(self))]
    pub async fn list_disputes(
        &self,
        status: Option<DisputeStatus>,
        tournament_id: Option<TournamentId>,
        match_id: Option<TournamentMatchId>,
        priority: Option<DisputePriority>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Dispute>, i64), DomainError> {
        self.dispute_repo
            .list_filtered(status, tournament_id, match_id, priority, limit, offset)
            .await
    }

    /// Get dispute with full thread.
    #[instrument(skip(self))]
    pub async fn get_dispute_with_thread(
        &self,
        dispute_id: DisputeId,
        include_internal: bool,
    ) -> Result<DisputeWithThread, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;
        let messages = self
            .message_repo
            .find_by_dispute(dispute_id, include_internal)
            .await?;

        Ok(DisputeWithThread { dispute, messages })
    }

    /// Cancel a dispute.
    #[instrument(skip(self))]
    pub async fn cancel_dispute(
        &self,
        dispute_id: DisputeId,
        cancelled_by: UserId,
    ) -> Result<Dispute, DomainError> {
        let dispute = self.get_dispute(dispute_id).await?;

        if dispute.is_terminal() {
            return Err(DomainError::InvalidState(format!(
                "Dispute {} is already {}",
                dispute_id, dispute.status
            )));
        }

        // Atomic: cancel dispute + restore match to Completed +
        // append cancellation message. See audit I5.
        let cancelled = self
            .dispute_repo
            .cancel_with_match_restore(
                dispute_id,
                dispute.match_id,
                CreateDisputeMessage {
                    dispute_id,
                    author_user_id: cancelled_by,
                    author_type: AuthorType::System,
                    message: "Dispute cancelled".to_string(),
                    evidence_ids: Vec::new(),
                    is_internal: false,
                },
            )
            .await?;

        info!(
            dispute_id = %dispute_id,
            "Dispute cancelled"
        );

        Ok(cancelled)
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    async fn get_match(&self, match_id: TournamentMatchId) -> Result<TournamentMatch, DomainError> {
        self.match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(match_id))
    }

    /// Fetch a dispute by ID or return a typed `DisputeNotFound` error.
    ///
    /// Pub because authorization logic in handlers needs to load the dispute
    /// metadata (match id, disputed-by user) before deciding whether to
    /// expose the thread to the caller.
    pub async fn get_dispute(&self, dispute_id: DisputeId) -> Result<Dispute, DomainError> {
        self.dispute_repo
            .find_by_id(dispute_id)
            .await?
            .ok_or(DomainError::DisputeNotFound(dispute_id))
    }

    fn validate_can_dispute(
        &self,
        match_: &TournamentMatch,
        disputed_by_registration_id: TournamentRegistrationId,
    ) -> Result<(), DomainError> {
        // Check if match can be disputed
        if !matches!(
            match_.status,
            TournamentMatchStatus::Completed | TournamentMatchStatus::Disputed
        ) {
            return Err(DomainError::InvalidState(format!(
                "Match in {} status cannot be disputed",
                match_.status
            )));
        }

        // Check if the disputer is a participant
        let is_participant = match_.participant1_registration_id
            == Some(disputed_by_registration_id)
            || match_.participant2_registration_id == Some(disputed_by_registration_id);

        if !is_participant {
            return Err(DomainError::NotAuthorized(
                "Only match participants can raise a dispute".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_can_resolve(&self, dispute: &Dispute) -> Result<(), DomainError> {
        if !dispute.can_resolve() {
            return Err(DomainError::InvalidState(format!(
                "Dispute {} cannot be resolved (status: {})",
                dispute.id, dispute.status
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispute_priority_from_reason() {
        // This tests the logic used in raise_dispute
        let cheating_priority = match DisputeReason::Cheating {
            DisputeReason::Cheating => DisputePriority::Urgent,
            DisputeReason::RuleViolation | DisputeReason::PlayerMisconduct => DisputePriority::High,
            _ => DisputePriority::Normal,
        };
        assert_eq!(cheating_priority, DisputePriority::Urgent);

        let wrong_score_priority = match DisputeReason::WrongScore {
            DisputeReason::Cheating => DisputePriority::Urgent,
            DisputeReason::RuleViolation | DisputeReason::PlayerMisconduct => DisputePriority::High,
            _ => DisputePriority::Normal,
        };
        assert_eq!(wrong_score_priority, DisputePriority::Normal);
    }
}
