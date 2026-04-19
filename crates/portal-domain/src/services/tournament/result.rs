//! Result submission service.
//!
//! Handles match result submission and confirmation workflow.
//! One team submits a result claim, the opponent confirms, disputes, or
//! the claim auto-confirms after a timeout.

use std::sync::Arc;

use chrono::{Duration, Utc};
use portal_core::{
    DemoMatchLinkId, DomainError, EvidenceId, ResultClaimId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use tracing::{info, instrument, warn};

use crate::entities::result_claim::{
    ClaimStatus, GameResult, GameResultInput, ResultClaim, ResultValidationError,
};
use crate::entities::tournament::TournamentMatch;
use crate::repositories::demo::DemoMatchLinkRepository;
use crate::repositories::tournament::{
    CreateResultClaim, ResultClaimRepository, TournamentMatchRepository,
    TournamentRegistrationRepository,
};

/// Service for managing match result submissions.
#[derive(Clone)]
pub struct ResultService<RCR, TMR, TRR, DMLR>
where
    RCR: ResultClaimRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    DMLR: DemoMatchLinkRepository,
{
    claim_repo: Arc<RCR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    demo_link_repo: Arc<DMLR>,
    auto_confirm_timeout_seconds: i64,
}

impl<RCR, TMR, TRR, DMLR> ResultService<RCR, TMR, TRR, DMLR>
where
    RCR: ResultClaimRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    DMLR: DemoMatchLinkRepository,
{
    /// Create a new result service with default 15-minute auto-confirm timeout.
    pub fn new(
        claim_repo: Arc<RCR>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
        demo_link_repo: Arc<DMLR>,
    ) -> Self {
        Self {
            claim_repo,
            match_repo,
            registration_repo,
            demo_link_repo,
            auto_confirm_timeout_seconds: 15 * 60, // 15 minutes
        }
    }

    /// Create a new result service with custom auto-confirm timeout.
    pub fn with_timeout(mut self, timeout_seconds: i64) -> Self {
        self.auto_confirm_timeout_seconds = timeout_seconds;
        self
    }

    /// Submit a result claim for a match.
    #[instrument(skip(self, game_results, evidence_ids, demo_link_ids))]
    pub async fn submit_claim(
        &self,
        match_id: TournamentMatchId,
        claimed_winner: TournamentRegistrationId,
        participant1_score: i32,
        participant2_score: i32,
        game_results: Vec<GameResultInput>,
        evidence_ids: Vec<EvidenceId>,
        demo_link_ids: Vec<DemoMatchLinkId>,
        notes: Option<String>,
        submitted_by_user: UserId,
    ) -> Result<ResultClaim, DomainError> {
        // Get the match
        let match_ = self.get_match(match_id).await?;

        // Verify match is in a state where results can be submitted
        if !match_.can_submit_result() {
            return Err(DomainError::InvalidState(format!(
                "Cannot submit result for match in {} status",
                match_.status
            )));
        }

        // Determine which participant the submitter is acting for
        let submitter_registration = self
            .find_user_registration(&match_, submitted_by_user)
            .await?;

        // Validate the claim
        self.validate_claim(
            &match_,
            claimed_winner,
            participant1_score,
            participant2_score,
            &game_results,
        )?;

        // Validate demo_link_ids belong to this match
        if !demo_link_ids.is_empty() {
            let links = self.demo_link_repo.find_by_ids(&demo_link_ids).await?;

            // Verify all requested IDs were found
            let found_ids: Vec<_> = links.iter().map(|l| l.id).collect();
            for id in &demo_link_ids {
                if !found_ids.contains(id) {
                    return Err(DomainError::DemoMatchLinkNotFound(*id));
                }
            }

            // Verify all links belong to this match
            for link in &links {
                if link.match_id != match_id {
                    return Err(DomainError::DemoNotLinkedToMatch(
                        link.id.to_string(),
                        match_id.to_string(),
                    ));
                }
            }
        }

        // Convert game result inputs to domain type
        let game_results: Vec<GameResult> = game_results
            .into_iter()
            .map(|g| self.convert_game_result(&match_, g, claimed_winner, participant1_score, participant2_score))
            .collect::<Result<Vec<_>, _>>()?;

        // Calculate auto-confirm time
        let auto_confirm_at = Utc::now() + Duration::seconds(self.auto_confirm_timeout_seconds);

        // Atomic: supersede any existing pending claim for the match
        // and insert the new one in one transaction. The previous
        // two-call version (`update_status(Superseded) + create`) left
        // the match claim-less on partial failure — confirmation and
        // auto-confirm both treated it as unclaimed until resubmission.
        // See audit I5.
        let claim = self
            .claim_repo
            .create_and_supersede_pending(CreateResultClaim {
                match_id,
                submitted_by_registration_id: submitter_registration,
                submitted_by_user_id: submitted_by_user,
                claimed_winner_registration_id: claimed_winner,
                participant1_score,
                participant2_score,
                game_results,
                auto_confirm_at,
                evidence_ids,
                demo_link_ids,
                notes,
            })
            .await?;

        info!(
            claim_id = %claim.id,
            match_id = %match_id,
            winner = %claimed_winner,
            score = format!("{}-{}", participant1_score, participant2_score),
            "Result claim submitted"
        );

        Ok(claim)
    }

    /// Confirm a result claim (by opponent).
    #[instrument(skip(self))]
    pub async fn confirm_claim(
        &self,
        claim_id: ResultClaimId,
        confirmed_by_user: UserId,
    ) -> Result<ResultClaim, DomainError> {
        let claim = self.get_claim(claim_id).await?;

        if !claim.is_pending() {
            return Err(DomainError::InvalidState(format!(
                "Cannot confirm claim in {} status",
                claim.status
            )));
        }

        // Get the match
        let match_ = self.get_match(claim.match_id).await?;

        // Determine which participant the confirmer is acting for
        let confirmer_registration = self
            .find_user_registration(&match_, confirmed_by_user)
            .await?;

        // Verify confirmer is not the submitter
        if confirmer_registration == claim.submitted_by_registration_id {
            return Err(DomainError::NotAuthorized(
                "Cannot confirm your own result claim".to_string(),
            ));
        }

        // Atomic: claim Confirmed + match result submitted commit
        // together. See audit I5. Loser derived here instead of inside
        // `apply_result_to_match` because the match row will be mutated
        // as part of the same tx.
        let loser = if match_.participant1_registration_id
            == Some(claim.claimed_winner_registration_id)
        {
            match_.participant2_registration_id
        } else {
            match_.participant1_registration_id
        }
        .ok_or_else(|| DomainError::InvalidState("Loser participant not found".to_string()))?;

        let claim = self
            .claim_repo
            .confirm_and_apply_to_match(
                claim_id,
                confirmer_registration,
                confirmed_by_user,
                false,
                match_.id,
                claim.claimed_winner_registration_id,
                loser,
                claim.claimed_participant1_score,
                claim.claimed_participant2_score,
            )
            .await?;

        info!(
            claim_id = %claim_id,
            match_id = %claim.match_id,
            confirmed_by = %confirmed_by_user,
            "Result claim confirmed"
        );

        Ok(claim)
    }

    /// Dispute a result claim.
    #[instrument(skip(self))]
    pub async fn dispute_claim(
        &self,
        claim_id: ResultClaimId,
        disputed_by_user: UserId,
        reason: &str,
    ) -> Result<ResultClaim, DomainError> {
        let claim = self.get_claim(claim_id).await?;

        if !claim.is_pending() {
            return Err(DomainError::InvalidState(format!(
                "Cannot dispute claim in {} status",
                claim.status
            )));
        }

        // Get the match
        let match_ = self.get_match(claim.match_id).await?;

        // Determine which participant the disputer is acting for
        let disputer_registration = self
            .find_user_registration(&match_, disputed_by_user)
            .await?;

        // Verify disputer is not the submitter
        if disputer_registration == claim.submitted_by_registration_id {
            return Err(DomainError::NotAuthorized(
                "Cannot dispute your own result claim".to_string(),
            ));
        }

        // Dispute the claim
        let claim = self
            .claim_repo
            .update_status(claim_id, ClaimStatus::Disputed)
            .await?;

        // Mark match as disputed
        self.match_repo
            .file_dispute(claim.match_id, reason.to_string())
            .await?;

        warn!(
            claim_id = %claim_id,
            match_id = %claim.match_id,
            reason = reason,
            "Result claim disputed"
        );

        Ok(claim)
    }

    /// Cancel a result claim (by submitter).
    #[instrument(skip(self))]
    pub async fn cancel_claim(
        &self,
        claim_id: ResultClaimId,
        cancelled_by_user: UserId,
    ) -> Result<ResultClaim, DomainError> {
        let claim = self.get_claim(claim_id).await?;

        if !claim.is_pending() {
            return Err(DomainError::InvalidState(format!(
                "Cannot cancel claim in {} status",
                claim.status
            )));
        }

        // Verify canceller is the submitter
        if claim.submitted_by_user_id != cancelled_by_user {
            return Err(DomainError::NotAuthorized(
                "Only the submitter can cancel their own claim".to_string(),
            ));
        }

        let claim = self
            .claim_repo
            .update_status(claim_id, ClaimStatus::Cancelled)
            .await?;

        info!(
            claim_id = %claim_id,
            match_id = %claim.match_id,
            "Result claim cancelled"
        );

        Ok(claim)
    }

    /// Process auto-confirmations for claims past their deadline.
    #[instrument(skip(self))]
    pub async fn process_auto_confirmations(&self) -> Result<Vec<ResultClaim>, DomainError> {
        let claims = self.claim_repo.find_ready_for_auto_confirm().await?;
        let mut confirmed = Vec::new();

        for claim in claims {
            match self.auto_confirm_claim(&claim).await {
                Ok(c) => confirmed.push(c),
                Err(e) => {
                    warn!(
                        claim_id = %claim.id,
                        error = %e,
                        "Failed to auto-confirm claim"
                    );
                }
            }
        }

        Ok(confirmed)
    }

    /// Get the pending claim for a match, if any.
    pub async fn get_pending_claim(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultClaim>, DomainError> {
        self.claim_repo.find_pending_by_match(match_id).await
    }

    /// Get a specific result claim by ID.
    pub async fn get_claim_by_id(
        &self,
        claim_id: ResultClaimId,
    ) -> Result<ResultClaim, DomainError> {
        self.get_claim(claim_id).await
    }

    /// Get claim history for a match.
    pub async fn get_claim_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ResultClaim>, DomainError> {
        self.claim_repo.list_by_match(match_id).await
    }

    // =========================================================================
    // INTERNAL HELPERS
    // =========================================================================

    async fn get_match(&self, id: TournamentMatchId) -> Result<TournamentMatch, DomainError> {
        self.match_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(id))
    }

    async fn get_claim(&self, id: ResultClaimId) -> Result<ResultClaim, DomainError> {
        self.claim_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal(format!("Result claim {id} not found")))
    }

    async fn find_user_registration(
        &self,
        match_: &TournamentMatch,
        user_id: UserId,
    ) -> Result<TournamentRegistrationId, DomainError> {
        // Check participant 1
        if let Some(reg_id) = match_.participant1_registration_id {
            let reg = self.registration_repo.find_by_id(reg_id).await?;
            if let Some(r) = reg {
                if r.registered_by == user_id {
                    return Ok(reg_id);
                }
            }
        }

        // Check participant 2
        if let Some(reg_id) = match_.participant2_registration_id {
            let reg = self.registration_repo.find_by_id(reg_id).await?;
            if let Some(r) = reg {
                if r.registered_by == user_id {
                    return Ok(reg_id);
                }
            }
        }

        Err(DomainError::NotAuthorized(
            "User is not authorized to act for any participant in this match".to_string(),
        ))
    }

    fn validate_claim(
        &self,
        match_: &TournamentMatch,
        claimed_winner: TournamentRegistrationId,
        participant1_score: i32,
        participant2_score: i32,
        game_results: &[GameResultInput],
    ) -> Result<(), DomainError> {
        // Validate scores are non-negative
        if participant1_score < 0 || participant2_score < 0 {
            return Err(DomainError::InvalidMatchResult(
                ResultValidationError::NegativeScore.to_string(),
            ));
        }

        // Validate winner is a participant
        let is_participant = match_.participant1_registration_id == Some(claimed_winner)
            || match_.participant2_registration_id == Some(claimed_winner);

        if !is_participant {
            return Err(DomainError::InvalidMatchResult(
                ResultValidationError::InvalidWinner.to_string(),
            ));
        }

        // Validate scores match winner
        let p1_wins = match_.participant1_registration_id == Some(claimed_winner);
        if p1_wins && participant1_score <= participant2_score {
            return Err(DomainError::InvalidMatchResult(
                ResultValidationError::ScoreWinnerMismatch.to_string(),
            ));
        }
        if !p1_wins && participant2_score <= participant1_score {
            return Err(DomainError::InvalidMatchResult(
                ResultValidationError::ScoreWinnerMismatch.to_string(),
            ));
        }

        // Validate game count matches format
        let wins_required = match_.match_format.wins_required();
        let expected_games = participant1_score + participant2_score;
        let min_games = wins_required;
        let max_games = wins_required * 2 - 1;

        if expected_games < min_games || expected_games > max_games {
            return Err(DomainError::InvalidMatchResult(
                ResultValidationError::InsufficientGames {
                    required: wins_required as u32,
                    provided: expected_games as u32,
                }
                .to_string(),
            ));
        }

        // Validate game results if provided
        if !game_results.is_empty() {
            // Game count should match series score
            if game_results.len() != expected_games as usize {
                return Err(DomainError::InvalidMatchResult(
                    ResultValidationError::GameScoresMismatch.to_string(),
                ));
            }

            // Validate game numbers are sequential
            for (i, game) in game_results.iter().enumerate() {
                if game.game_number != (i + 1) as i32 {
                    return Err(DomainError::InvalidMatchResult(
                        ResultValidationError::NonSequentialGameNumber(game.game_number).to_string(),
                    ));
                }

                // No ties allowed
                if game.participant1_score == game.participant2_score {
                    return Err(DomainError::InvalidMatchResult(
                        ResultValidationError::TiedGame.to_string(),
                    ));
                }
            }

            // Sum of game winners should match series score
            let p1_game_wins = game_results
                .iter()
                .filter(|g| g.participant1_score > g.participant2_score)
                .count() as i32;
            let p2_game_wins = game_results.len() as i32 - p1_game_wins;

            if p1_game_wins != participant1_score || p2_game_wins != participant2_score {
                return Err(DomainError::InvalidMatchResult(
                    ResultValidationError::GameScoresMismatch.to_string(),
                ));
            }
        }

        Ok(())
    }

    fn convert_game_result(
        &self,
        match_: &TournamentMatch,
        input: GameResultInput,
        _series_winner: TournamentRegistrationId,
        _p1_series_score: i32,
        _p2_series_score: i32,
    ) -> Result<GameResult, DomainError> {
        let p1_won = input.participant1_score > input.participant2_score;
        let game_winner = if p1_won {
            match_.participant1_registration_id
        } else {
            match_.participant2_registration_id
        }
        .ok_or_else(|| DomainError::InvalidState("Participant not found".to_string()))?;

        Ok(GameResult {
            game_number: input.game_number,
            map_id: input.map_id,
            participant1_score: input.participant1_score,
            participant2_score: input.participant2_score,
            winner_registration_id: game_winner,
            started_at: None,
            completed_at: None,
            duration_seconds: input.duration_seconds,
            evidence_ids: input.evidence_ids,
            demo_link_id: input.demo_link_id,
        })
    }

    async fn auto_confirm_claim(&self, claim: &ResultClaim) -> Result<ResultClaim, DomainError> {
        // Get the match
        let match_ = self.get_match(claim.match_id).await?;

        // Find opponent registration
        let opponent = if match_.participant1_registration_id == Some(claim.submitted_by_registration_id) {
            match_.participant2_registration_id
        } else {
            match_.participant1_registration_id
        }
        .ok_or_else(|| DomainError::InvalidState("Opponent not found".to_string()))?;

        // Atomic auto-confirm + result application. See audit I5.
        let loser = if match_.participant1_registration_id
            == Some(claim.claimed_winner_registration_id)
        {
            match_.participant2_registration_id
        } else {
            match_.participant1_registration_id
        }
        .ok_or_else(|| DomainError::InvalidState("Loser participant not found".to_string()))?;

        let claim = self
            .claim_repo
            .confirm_and_apply_to_match(
                claim.id,
                opponent,
                claim.submitted_by_user_id, // Use submitter's user ID as placeholder
                true,                        // was_auto
                match_.id,
                claim.claimed_winner_registration_id,
                loser,
                claim.claimed_participant1_score,
                claim.claimed_participant2_score,
            )
            .await?;

        info!(
            claim_id = %claim.id,
            match_id = %claim.match_id,
            "Result claim auto-confirmed"
        );

        Ok(claim)
    }
}
