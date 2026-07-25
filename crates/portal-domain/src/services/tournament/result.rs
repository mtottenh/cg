//! Result submission service.
//!
//! Handles match result submission and confirmation workflow.
//! One team submits a result claim, the opponent confirms, disputes, or
//! the claim auto-confirms after a timeout.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use portal_core::{
    DemoMatchLinkId, DomainError, EvidenceId, ResultClaimId, TournamentId, TournamentMatchId,
    TournamentRegistrationId, TournamentStageId, UserId,
};
use tracing::{info, instrument, warn};

use crate::entities::match_lifecycle::TransitionTrigger;
use crate::entities::result_claim::{
    ClaimStatus, GameResult, GameResultInput, ResultClaim, ResultValidationError,
};
use crate::entities::tournament::TournamentMatch;
use crate::entities::veto::VetoStatus;
use crate::repositories::demo::DemoMatchLinkRepository;
use crate::repositories::league_team::LeagueTeamMemberRepository;
use crate::repositories::tournament::{
    CreateResultClaim, ResultClaimRepository, TournamentMatchRepository,
    TournamentRegistrationRepository, VetoSessionRepository,
};
use crate::services::tournament::match_lifecycle::MatchStatusTransitioner;
use crate::services::tournament::registration_actor::{RegistrationActor, find_actor_registration};

// =============================================================================
// PROVIDER TRAITS
// =============================================================================

/// Resolves the set of map IDs that are legal for a tournament (and stage).
///
/// Implemented by the API layer, which owns the resolution chain
/// (tournament/stage map pool → the game's default pool → the game's map
/// catalog). Mirrors the `VetoFormatProvider` seam used by the veto service:
/// the domain must not reach into `portal-db`'s game repository directly.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait MapPoolProvider: Send + Sync {
    /// Valid map IDs for the given tournament/stage.
    ///
    /// An empty vec means "cannot be determined" — callers treat that as
    /// "skip validation" rather than "reject everything".
    async fn valid_map_ids(
        &self,
        tournament_id: TournamentId,
        stage_id: Option<TournamentStageId>,
    ) -> Result<Vec<String>, DomainError>;
}

/// `entity_changes.entity_type` used for admin score corrections (P-72).
///
/// Exported so the read side (`list_result_overrides`) and the write side
/// cannot drift — an audit row written under one key and read under another
/// is an audit trail that silently shows nothing.
pub const MATCH_RESULT_OVERRIDE_ENTITY: &str = "tournament_match";

/// `entity_changes.field_name` used for admin score corrections (P-72).
pub const MATCH_RESULT_OVERRIDE_FIELD: &str = "result_override";

/// Which authority a submitted map ID was checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapSource {
    /// The maps picked during this match's completed veto.
    Veto,
    /// The tournament/stage map pool (or the game's pool as fallback).
    Pool,
}

/// Check every submitted `map_id` against the authoritative map set.
///
/// Kept as a free function so the rule is unit-testable without standing up
/// a service and a full `TournamentMatch`.
fn check_map_ids(
    valid_maps: &[String],
    game_results: &[GameResultInput],
    source: MapSource,
) -> Result<(), DomainError> {
    for game in game_results {
        if !valid_maps.iter().any(|m| m == &game.map_id) {
            let err = match source {
                MapSource::Veto => ResultValidationError::MapNotInVeto(game.map_id.clone()),
                MapSource::Pool => ResultValidationError::UnknownMap(game.map_id.clone()),
            };
            return Err(DomainError::InvalidMatchResult(err.to_string()));
        }
    }

    Ok(())
}

/// Service for managing match result submissions.
#[derive(Clone)]
pub struct ResultService<RCR, TMR, TRR, DMLR, VSR, LTMR>
where
    RCR: ResultClaimRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    DMLR: DemoMatchLinkRepository,
    VSR: VetoSessionRepository,
    LTMR: LeagueTeamMemberRepository,
{
    claim_repo: Arc<RCR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    demo_link_repo: Arc<DMLR>,
    veto_session_repo: Arc<VSR>,
    /// Roster lookups behind [`speaks_for_registration`] — the single
    /// definition of who may act for a participant (P-168). Mandatory rather
    /// than an `Option` builder: a service missing it would silently fall back
    /// to "only the person who clicked register", which is the defect.
    member_repo: Arc<LTMR>,
    map_pool_provider: Option<Arc<dyn MapPoolProvider>>,
    match_transitioner: Option<Arc<dyn MatchStatusTransitioner>>,
    auto_confirm_timeout_seconds: i64,
}

impl<RCR, TMR, TRR, DMLR, VSR, LTMR> ResultService<RCR, TMR, TRR, DMLR, VSR, LTMR>
where
    RCR: ResultClaimRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    DMLR: DemoMatchLinkRepository,
    VSR: VetoSessionRepository,
    LTMR: LeagueTeamMemberRepository,
{
    /// Create a new result service with default 15-minute auto-confirm timeout.
    pub fn new(
        claim_repo: Arc<RCR>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
        demo_link_repo: Arc<DMLR>,
        veto_session_repo: Arc<VSR>,
        member_repo: Arc<LTMR>,
    ) -> Self {
        Self {
            claim_repo,
            match_repo,
            registration_repo,
            demo_link_repo,
            veto_session_repo,
            member_repo,
            map_pool_provider: None,
            match_transitioner: None,
            // P-57: 24 hours, raised from 15 minutes. Auto-confirm makes a score
            // OFFICIAL, and the countdown starts at submission — not when the
            // opponent first sees it — so 15 minutes meant a submit at a bad hour
            // became final unseen, across time zones and sleep. Now that P-50
            // actually notifies the opponent, a day is enough to act and short
            // enough that an ignored claim still cannot stall a bracket (the
            // dispute/review path covers genuinely wrong results either way).
            auto_confirm_timeout_seconds: 24 * 60 * 60, // 24 hours
        }
    }

    /// Create a new result service with custom auto-confirm timeout.
    pub fn with_timeout(mut self, timeout_seconds: i64) -> Self {
        self.auto_confirm_timeout_seconds = timeout_seconds;
        self
    }

    /// Attach the map pool provider used to validate submitted map IDs for
    /// matches that had no veto. Without it, non-veto matches skip map
    /// validation.
    #[must_use]
    pub fn with_map_pool_provider(mut self, provider: Arc<dyn MapPoolProvider>) -> Self {
        self.map_pool_provider = Some(provider);
        self
    }

    /// Attach the transitioner used to move a match `in_progress ->
    /// awaiting_result` when a claim is submitted. Without it the match stays
    /// `in_progress`, the opponent never receives a confirm-result action
    /// item, and the claim auto-confirms unseen (P-50).
    #[must_use]
    pub fn with_match_transitioner(
        mut self,
        transitioner: Arc<dyn MatchStatusTransitioner>,
    ) -> Self {
        self.match_transitioner = Some(transitioner);
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
        submitted_by: RegistrationActor,
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

        // Determine which participant the submitter is acting for. Any active
        // member of the registered team's roster speaks for it (P-168) — not
        // just whoever happened to click "register".
        let submitter_registration = self.find_actor_registration(&match_, submitted_by).await?;

        // Validate the claim
        self.validate_claim(
            &match_,
            claimed_winner,
            participant1_score,
            participant2_score,
            &game_results,
        )
        .await?;

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
            .map(|g| {
                self.convert_game_result(
                    &match_,
                    g,
                    claimed_winner,
                    participant1_score,
                    participant2_score,
                )
            })
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
                submitted_by_user_id: submitted_by.user_id,
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

        // Move the match into `awaiting_result` so the opponent gets a
        // confirm-result action item and the auto-confirm deadline is
        // enforced against a state that actually surfaces to them. The
        // action-item query keys the confirm/dispute item off
        // `awaiting_result` specifically; a match left `in_progress` never
        // produced that item, so a submitted result auto-confirmed in 15
        // minutes against an opponent who was never notified (P-50).
        //
        // Only the first submission needs the transition — a resubmission
        // supersedes the prior claim while the match is already
        // `awaiting_result`, and `awaiting_result -> awaiting_result` is not
        // a legal edge. The transition is driven through
        // `MatchStatusTransitioner` (the `MatchLifecycleService` path) so it
        // is written to `match_status_log` like every other transition
        // rather than as a silent raw UPDATE.
        if match_.status == portal_core::types::TournamentMatchStatus::InProgress
            && let Some(transitioner) = &self.match_transitioner
        {
            transitioner
                .transition_status(
                    match_id,
                    portal_core::types::TournamentMatchStatus::AwaitingResult,
                    TransitionTrigger::System {
                        job_name: "result_submitted".to_string(),
                    },
                    Some("Result claim submitted; awaiting opponent confirmation".to_string()),
                )
                .await?;
        }

        Ok(claim)
    }

    /// Confirm a result claim (by opponent).
    #[instrument(skip(self))]
    pub async fn confirm_claim(
        &self,
        claim_id: ResultClaimId,
        confirmed_by: RegistrationActor,
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

        // Determine which participant the confirmer is acting for (P-168:
        // roster membership, not "who registered the team").
        let confirmer_registration = self.find_actor_registration(&match_, confirmed_by).await?;

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
        let loser =
            if match_.participant1_registration_id == Some(claim.claimed_winner_registration_id) {
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
                confirmed_by.user_id,
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
            confirmed_by = %confirmed_by.user_id,
            "Result claim confirmed"
        );

        Ok(claim)
    }

    /// Administratively correct the score already recorded against a match.
    ///
    /// # Why this exists (P-72)
    ///
    /// The score-writing admin paths were: `resolve/overturn`,
    /// `resolve/adjusted` and `resolve/double-dq` — **all** of which take a
    /// `dispute_id` and refuse to run without a dispute row. So the failure
    /// mode "both parties confirmed the wrong score" (or "nobody looked and it
    /// auto-confirmed after 24h"), with nobody disputing, produced a match no
    /// operator could correct by any means, while the bracket kept advancing
    /// on it. `revert`/`reapply` exist and move the bracket, but they replay
    /// the recorded score — they cannot change it.
    ///
    /// # Why it goes through the repository's audited write
    ///
    /// `override_result_audited` performs the score write and the audit row in
    /// one transaction, using the same statement
    /// (`submit_result_in_tx`) as opponent-confirmation and adjusted-dispute
    /// resolution. So (a) the resulting match row is indistinguishable from a
    /// normally-recorded one, which is what keeps standings and progression
    /// consistent, and (b) there is no code path that changes a score without
    /// recording who changed it, from what, to what, and why.
    ///
    /// Bracket progression is **not** re-run here — deliberately, and for the
    /// same reason `resolve_adjusted` does not: moving participants that have
    /// already advanced is the job of the explicit
    /// revert/reapply-progression controls, which an admin uses after the
    /// correction if the winner changed. Silently reshaping downstream
    /// pairings as a side effect of a score edit would be worse than the
    /// defect being fixed.
    #[instrument(skip(self, reason))]
    pub async fn override_result(
        &self,
        match_id: TournamentMatchId,
        participant1_score: i32,
        participant2_score: i32,
        reason: String,
        changed_by: portal_core::PlayerId,
        request_id: Option<String>,
    ) -> Result<TournamentMatch, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // Only a match that already carries a recorded result can be
        // "corrected". Writing a score onto a match that was never played
        // would be a different operation with different consequences
        // (it would complete the match), and the admin transition + forfeit
        // controls already own that.
        if match_.winner_registration_id.is_none() {
            return Err(DomainError::InvalidState(
                "Match has no recorded result to correct".to_string(),
            ));
        }

        let (winner_id, loser_id) = crate::services::tournament::helpers::derive_result_outcome(
            &match_,
            participant1_score,
            participant2_score,
        )?;

        let old_value = serde_json::json!({
            "participant1_score": match_.participant1_score,
            "participant2_score": match_.participant2_score,
            "winner_registration_id": match_.winner_registration_id.map(|id| id.to_string()),
        });
        let new_value = serde_json::json!({
            "participant1_score": participant1_score,
            "participant2_score": participant2_score,
            "winner_registration_id": winner_id.to_string(),
            "reason": reason,
        });

        let updated = self
            .match_repo
            .override_result_audited(
                match_id,
                participant1_score,
                participant2_score,
                winner_id,
                loser_id,
                crate::repositories::CreateEntityChange {
                    entity_type: MATCH_RESULT_OVERRIDE_ENTITY.to_string(),
                    entity_id: match_id.as_uuid(),
                    change_type: crate::entities::audit::ChangeType::Update,
                    field_name: Some(MATCH_RESULT_OVERRIDE_FIELD.to_string()),
                    old_value: Some(old_value),
                    new_value: Some(new_value),
                    changed_by,
                    request_id,
                    ip_address: None,
                    user_agent: None,
                },
            )
            .await?;

        info!(
            match_id = %match_id,
            changed_by = %changed_by,
            "Match result corrected by admin override"
        );

        Ok(updated)
    }

    /// Authorize a dispute against a result claim, without writing
    /// anything.
    ///
    /// Returns the claim plus the registration the disputing user acts
    /// for; the caller then hands both to
    /// `DisputeService::raise_dispute`, which performs every write in one
    /// transaction (claim → `disputed`, match → `disputed`, `disputes`
    /// row, opening thread message).
    ///
    /// This replaces the old `dispute_claim`, which did
    /// `update_status(Disputed)` + `file_dispute` as two unguarded writes
    /// and created **no** `disputes` row at all — so a claim-path dispute
    /// was invisible to the admin dispute queue, and a failure between the
    /// two writes left a Disputed claim on a non-disputed match.
    #[instrument(skip(self))]
    pub async fn authorize_claim_dispute(
        &self,
        claim_id: ResultClaimId,
        disputed_by: RegistrationActor,
    ) -> Result<(ResultClaim, TournamentRegistrationId), DomainError> {
        let claim = self.get_claim(claim_id).await?;

        if !claim.is_pending() {
            return Err(DomainError::InvalidState(format!(
                "Cannot dispute claim in {} status",
                claim.status
            )));
        }

        // Get the match
        let match_ = self.get_match(claim.match_id).await?;

        // Determine which participant the disputer is acting for — the same
        // rule the submission it disputes was authorized under (P-168).
        let disputer_registration = self.find_actor_registration(&match_, disputed_by).await?;

        // Verify disputer is not the submitter
        if disputer_registration == claim.submitted_by_registration_id {
            return Err(DomainError::NotAuthorized(
                "Cannot dispute your own result claim".to_string(),
            ));
        }

        Ok((claim, disputer_registration))
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

    /// Get the current (authoritative) claim for a match, if any.
    ///
    /// While the match is live that is the pending claim; once it has been
    /// confirmed the confirmed claim remains the match's result.
    pub async fn get_current_claim(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultClaim>, DomainError> {
        self.claim_repo.find_current_by_match(match_id).await
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
            .ok_or(DomainError::TournamentMatchNotFound(id))
    }

    async fn get_claim(&self, id: ResultClaimId) -> Result<ResultClaim, DomainError> {
        self.claim_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::ResultClaimNotFound(id))
    }

    /// Which of this match's registrations the actor speaks for.
    ///
    /// Delegates to [`find_actor_registration`], the single definition shared
    /// with disputes, evidence, scheduling and withdrawal. The version this
    /// replaced tested `registration.registered_by == user_id`, so exactly one
    /// human per team could submit or confirm a result — while the frontend,
    /// which gates on roster membership, offered the panel to all of them
    /// (P-168).
    async fn find_actor_registration(
        &self,
        match_: &TournamentMatch,
        actor: RegistrationActor,
    ) -> Result<TournamentRegistrationId, DomainError> {
        find_actor_registration(
            self.registration_repo.as_ref(),
            self.member_repo.as_ref(),
            match_,
            actor,
        )
        .await
    }

    async fn validate_claim(
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
                        ResultValidationError::NonSequentialGameNumber(game.game_number)
                            .to_string(),
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

            // Every reported map must be a real map for this match.
            self.validate_map_ids(match_, game_results).await?;
        }

        Ok(())
    }

    /// Validate submitted map IDs against this match's authoritative map set.
    ///
    /// Precedence:
    /// 1. If the match had a **completed veto**, the picked maps are
    ///    authoritative — a map that was never picked is rejected even if it
    ///    is otherwise a real map in the pool.
    /// 2. Otherwise the tournament/stage map pool (with the game's
    ///    pool/catalog behind it) via [`MapPoolProvider`].
    ///
    /// This **fails closed**: every tournament is created with an explicit
    /// map pool, so a pool that cannot be resolved is a real error, not a
    /// reason to wave the submission through.
    async fn validate_map_ids(
        &self,
        match_: &TournamentMatch,
        game_results: &[GameResultInput],
    ) -> Result<(), DomainError> {
        // Nothing reported, nothing to validate.
        if game_results.is_empty() {
            return Ok(());
        }

        if let Some(session) = self.veto_session_repo.find_by_match(match_.id).await?
            && session.status == VetoStatus::Completed
            && !session.selected_maps.is_empty()
        {
            return check_map_ids(&session.selected_maps, game_results, MapSource::Veto);
        }

        let provider = self.map_pool_provider.as_ref().ok_or_else(|| {
            DomainError::Internal("No map pool provider configured for result validation".into())
        })?;

        let valid_maps = provider
            .valid_map_ids(match_.tournament_id, Some(match_.stage_id))
            .await?;

        if valid_maps.is_empty() {
            warn!(
                match_id = %match_.id,
                tournament_id = %match_.tournament_id,
                "no map pool resolved for match; rejecting result submission"
            );
            return Err(DomainError::InvalidMatchResult(
                "No map pool is configured for this tournament, so submitted maps cannot be \
                 validated"
                    .to_string(),
            ));
        }

        check_map_ids(&valid_maps, game_results, MapSource::Pool)
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
        let opponent =
            if match_.participant1_registration_id == Some(claim.submitted_by_registration_id) {
                match_.participant2_registration_id
            } else {
                match_.participant1_registration_id
            }
            .ok_or_else(|| DomainError::InvalidState("Opponent not found".to_string()))?;

        // Atomic auto-confirm + result application. See audit I5.
        let loser =
            if match_.participant1_registration_id == Some(claim.claimed_winner_registration_id) {
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
                true,                       // was_auto
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a game result for `map_id`; scores are arbitrary but non-tied.
    fn game_on(game_number: i32, map_id: &str) -> GameResultInput {
        GameResultInput {
            game_number,
            map_id: map_id.to_string(),
            participant1_score: 16,
            participant2_score: 10,
            duration_seconds: None,
            evidence_ids: vec![],
            demo_link_id: None,
        }
    }

    fn pool() -> Vec<String> {
        vec![
            "de_dust2".to_string(),
            "de_mirage".to_string(),
            "de_inferno".to_string(),
            "de_nuke".to_string(),
        ]
    }

    #[test]
    fn test_check_map_ids_accepts_maps_in_pool() {
        let games = vec![game_on(1, "de_dust2"), game_on(2, "de_inferno")];

        assert!(check_map_ids(&pool(), &games, MapSource::Pool).is_ok());
    }

    #[test]
    fn test_check_map_ids_rejects_placeholder_map_id() {
        let games = vec![game_on(1, "de_dust2"), game_on(2, "map_1")];

        let err = check_map_ids(&pool(), &games, MapSource::Pool)
            .expect_err("placeholder map id must be rejected");

        match err {
            DomainError::InvalidMatchResult(msg) => {
                assert!(msg.contains("map_1"), "message should name the map: {msg}");
            }
            other => panic!("expected InvalidMatchResult, got {other:?}"),
        }
    }

    #[test]
    fn test_check_map_ids_rejects_real_map_not_picked_in_veto() {
        // de_nuke is a real pool map, but the veto picked dust2 + inferno.
        let selected = vec!["de_dust2".to_string(), "de_inferno".to_string()];
        let games = vec![game_on(1, "de_dust2"), game_on(2, "de_nuke")];

        // Against the full pool it would pass...
        assert!(check_map_ids(&pool(), &games, MapSource::Pool).is_ok());

        // ...but the veto picks are authoritative.
        let err = check_map_ids(&selected, &games, MapSource::Veto)
            .expect_err("map not picked in veto must be rejected");

        match err {
            DomainError::InvalidMatchResult(msg) => {
                assert!(
                    msg.contains("de_nuke"),
                    "message should name the map: {msg}"
                );
                assert!(
                    msg.contains("veto"),
                    "veto rejection should say so, got: {msg}"
                );
            }
            other => panic!("expected InvalidMatchResult, got {other:?}"),
        }
    }

    #[test]
    fn test_check_map_ids_accepts_exact_veto_selection() {
        let selected = vec!["de_dust2".to_string(), "de_inferno".to_string()];
        let games = vec![game_on(1, "de_inferno"), game_on(2, "de_dust2")];

        assert!(check_map_ids(&selected, &games, MapSource::Veto).is_ok());
    }

    #[test]
    fn test_check_map_ids_accepts_empty_game_results() {
        assert!(check_map_ids(&pool(), &[], MapSource::Pool).is_ok());
    }

    // ------------------------------------------------------------------
    // validate_map_ids: veto precedence + fail-closed behaviour
    // ------------------------------------------------------------------

    use crate::entities::tournament::TournamentMatch;
    use crate::entities::veto::VetoSession;
    use crate::repositories::demo::MockDemoMatchLinkRepository;
    use crate::repositories::league_team::MockLeagueTeamMemberRepository;
    use crate::repositories::tournament::{
        MockResultClaimRepository, MockTournamentMatchRepository,
        MockTournamentRegistrationRepository, MockVetoSessionRepository,
    };
    use portal_core::types::{MatchFormat, TournamentMatchStatus};
    use portal_core::{
        SideSelectionMode, TournamentBracketId, TournamentId, TournamentStageId, VetoSessionId,
    };

    type TestService = ResultService<
        MockResultClaimRepository,
        MockTournamentMatchRepository,
        MockTournamentRegistrationRepository,
        MockDemoMatchLinkRepository,
        MockVetoSessionRepository,
        MockLeagueTeamMemberRepository,
    >;

    fn make_service(veto_repo: MockVetoSessionRepository) -> TestService {
        ResultService::new(
            Arc::new(MockResultClaimRepository::new()),
            Arc::new(MockTournamentMatchRepository::new()),
            Arc::new(MockTournamentRegistrationRepository::new()),
            Arc::new(MockDemoMatchLinkRepository::new()),
            Arc::new(veto_repo),
            Arc::new(MockLeagueTeamMemberRepository::new()),
        )
    }

    fn make_match() -> TournamentMatch {
        TournamentMatch {
            id: TournamentMatchId::new(),
            bracket_id: TournamentBracketId::new(),
            stage_id: TournamentStageId::new(),
            tournament_id: TournamentId::new(),
            round: 1,
            match_number: 1,
            bracket_position: "R1M1".to_string(),
            participant1_registration_id: Some(TournamentRegistrationId::new()),
            participant2_registration_id: Some(TournamentRegistrationId::new()),
            participant1_name: None,
            participant1_logo_url: None,
            participant1_seed: None,
            participant2_name: None,
            participant2_logo_url: None,
            participant2_seed: None,
            participant1_source: None,
            participant2_source: None,
            match_format: MatchFormat::Bo3,
            maps_required: 3,
            scheduled_at: None,
            schedule_deadline: None,
            started_at: None,
            completed_at: None,
            participant1_score: 0,
            participant2_score: 0,
            winner_registration_id: None,
            loser_registration_id: None,
            winner_progresses_to: None,
            loser_progresses_to: None,
            status: TournamentMatchStatus::InProgress,
            disputed: false,
            dispute_reason: None,
            dispute_resolved_by: None,
            dispute_resolution: None,
            dispute_resolved_at: None,
            stream_url: None,
            vod_url: None,
            check_in_opens_at: None,
            check_in_deadline: None,
            participant1_checked_in_at: None,
            participant2_checked_in_at: None,
            participant1_checked_in_by: None,
            participant2_checked_in_by: None,
            veto_required: false,
            check_in_required: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_completed_veto(match_id: TournamentMatchId, selected: &[&str]) -> VetoSession {
        VetoSession {
            id: VetoSessionId::new(),
            match_id,
            veto_format_id: "bo3_standard".to_string(),
            map_pool: pool(),
            coin_flip_winner_registration_id: None,
            first_action_registration_id: None,
            current_action_number: 0,
            current_team_turn: None,
            remaining_maps: vec![],
            selected_maps: selected.iter().map(|m| (*m).to_string()).collect(),
            status: VetoStatus::Completed,
            action_deadline: None,
            timeout_seconds: 30,
            side_selection_mode: SideSelectionMode::CoinFlip,
            started_at: None,
            completed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    struct StubPool(Vec<String>);

    #[async_trait]
    impl MapPoolProvider for StubPool {
        async fn valid_map_ids(
            &self,
            _tournament_id: TournamentId,
            _stage_id: Option<TournamentStageId>,
        ) -> Result<Vec<String>, DomainError> {
            Ok(self.0.clone())
        }
    }

    /// Fail closed: an unresolvable (empty) pool now rejects instead of
    /// waving the submission through.
    #[tokio::test]
    async fn test_validate_map_ids_rejects_when_pool_unresolvable() {
        let mut veto_repo = MockVetoSessionRepository::new();
        veto_repo.expect_find_by_match().returning(|_| Ok(None));

        let service = make_service(veto_repo).with_map_pool_provider(Arc::new(StubPool(vec![])));
        let match_ = make_match();
        let games = vec![game_on(1, "de_dust2")];

        let err = service
            .validate_map_ids(&match_, &games)
            .await
            .expect_err("an unresolvable map pool must fail closed");

        match err {
            DomainError::InvalidMatchResult(msg) => {
                assert!(msg.contains("map pool"), "unexpected message: {msg}");
            }
            other => panic!("expected InvalidMatchResult, got {other:?}"),
        }
    }

    /// Fail closed: no provider configured at all is an error too.
    #[tokio::test]
    async fn test_validate_map_ids_rejects_without_provider() {
        let mut veto_repo = MockVetoSessionRepository::new();
        veto_repo.expect_find_by_match().returning(|_| Ok(None));

        let service = make_service(veto_repo);
        let match_ = make_match();
        let games = vec![game_on(1, "de_dust2")];

        assert!(service.validate_map_ids(&match_, &games).await.is_err());
    }

    /// Empty game_results is still skipped — nothing to validate.
    #[tokio::test]
    async fn test_validate_map_ids_allows_empty_game_results() {
        let mut veto_repo = MockVetoSessionRepository::new();
        veto_repo.expect_find_by_match().returning(|_| Ok(None));

        let service = make_service(veto_repo).with_map_pool_provider(Arc::new(StubPool(vec![])));
        let match_ = make_match();

        assert!(service.validate_map_ids(&match_, &[]).await.is_ok());
    }

    /// A completed veto wins over the tournament pool.
    #[tokio::test]
    async fn test_validate_map_ids_prefers_completed_veto_over_pool() {
        let match_ = make_match();
        let match_id = match_.id;

        let mut veto_repo = MockVetoSessionRepository::new();
        veto_repo.expect_find_by_match().returning(move |_| {
            Ok(Some(make_completed_veto(
                match_id,
                &["de_dust2", "de_inferno"],
            )))
        });

        // The provider would happily allow de_nuke; the veto must not.
        let service = make_service(veto_repo).with_map_pool_provider(Arc::new(StubPool(pool())));

        let picked = vec![game_on(1, "de_dust2"), game_on(2, "de_inferno")];
        assert!(service.validate_map_ids(&match_, &picked).await.is_ok());

        let not_picked = vec![game_on(1, "de_dust2"), game_on(2, "de_nuke")];
        assert!(
            service
                .validate_map_ids(&match_, &not_picked)
                .await
                .is_err()
        );
    }

    /// Non-veto match validates against the tournament pool.
    #[tokio::test]
    async fn test_validate_map_ids_falls_back_to_pool_without_veto() {
        let mut veto_repo = MockVetoSessionRepository::new();
        veto_repo.expect_find_by_match().returning(|_| Ok(None));

        let service = make_service(veto_repo).with_map_pool_provider(Arc::new(StubPool(pool())));
        let match_ = make_match();

        assert!(
            service
                .validate_map_ids(&match_, &[game_on(1, "de_dust2")])
                .await
                .is_ok()
        );
        assert!(
            service
                .validate_map_ids(&match_, &[game_on(1, "map_1")])
                .await
                .is_err()
        );
    }
}
