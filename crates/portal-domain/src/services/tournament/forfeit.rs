//! Forfeit service.
//!
//! Handles forfeit processing for no-show, withdrawal, disqualification,
//! and technical default scenarios.

use std::sync::Arc;

use portal_core::types::{TournamentMatchStatus, TournamentRegistrationStatus, WithdrawalPolicy};
use portal_core::{DomainError, TournamentId, TournamentMatchId, TournamentRegistrationId, UserId};
use tracing::{info, instrument, warn};

use crate::entities::forfeit::{ForfeitResult, ForfeitTrigger, ForfeitType};
use crate::entities::tournament::TournamentMatch;
use crate::repositories::forfeit::{CreateForfeitRecord, ForfeitRecordRepository};
use crate::repositories::tournament::{
    TournamentMatchRepository, TournamentRegistrationRepository, TournamentRepository,
};

use super::match_completion::{DepartedParticipantForfeiter, DepartureOutcome};

/// Service for handling forfeits.
#[derive(Clone)]
pub struct ForfeitService<FRR, TMR, TRR> {
    forfeit_repo: Arc<FRR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
}

impl<FRR, TMR, TRR> ForfeitService<FRR, TMR, TRR>
where
    FRR: ForfeitRecordRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new forfeit service.
    pub fn new(forfeit_repo: Arc<FRR>, match_repo: Arc<TMR>, registration_repo: Arc<TRR>) -> Self {
        Self {
            forfeit_repo,
            match_repo,
            registration_repo,
        }
    }

    /// Process a forfeit for a match.
    #[instrument(skip(self))]
    pub async fn process_forfeit(
        &self,
        match_id: TournamentMatchId,
        forfeiting_registration_id: TournamentRegistrationId,
        forfeit_type: ForfeitType,
        reason: Option<String>,
        triggered_by: ForfeitTrigger,
    ) -> Result<ForfeitResult, DomainError> {
        // Get the match
        let match_ = self.get_match(match_id).await?;

        // Validate the match can be forfeited
        self.validate_can_forfeit(&match_, forfeiting_registration_id)?;

        // Determine the winner (opponent)
        let winner_registration_id = self.determine_winner(&match_, forfeiting_registration_id)?;

        // Write the match first. The update is guarded on the match still
        // being forfeitable, so of two concurrent forfeits (one per side, or
        // the same one twice) exactly one gets past this line; the other
        // returns `Conflict` having written nothing — in particular no
        // second forfeit record. The same statement supersedes any pending
        // result claim on the match, so a claim filed before the forfeit
        // (e.g. by a team that was then disqualified) cannot be confirmed
        // over it later.
        //
        // The walkover is recorded with its series score (wins_required–0)
        // so the derived standings credit it exactly like a played result.
        let (winner_score, loser_score) = forfeit_type.default_score(match_.match_format);
        self.match_repo
            .forfeit(
                match_id,
                winner_registration_id,
                forfeiting_registration_id,
                winner_score,
                loser_score,
            )
            .await?;

        // Then the audit record. The insert is idempotent (migration 0072).
        // There is deliberately no `exists_for_match` refusal anywhere on
        // this path: a record left behind by an attempt that died before the
        // match write (the old order) must not strand the match.
        let forfeit_record = self
            .forfeit_repo
            .create(CreateForfeitRecord {
                match_id,
                forfeiting_registration_id,
                forfeit_type,
                reason: reason.clone(),
                triggered_by_user_id: triggered_by.user_id(),
                triggered_by_system: triggered_by.is_system(),
            })
            .await?;

        info!(
            match_id = %match_id,
            forfeiting = %forfeiting_registration_id,
            winner = %winner_registration_id,
            forfeit_type = %forfeit_type,
            "Processed forfeit"
        );

        Ok(ForfeitResult {
            match_id,
            forfeit_record,
            winner_registration_id: Some(winner_registration_id),
            // The forfeit is recorded; bracket progression / standings /
            // stage advancement are run by the caller through the
            // match-completion saga (`settle_result`), which flips this.
            progression_triggered: false,
        })
    }

    /// Process a no-show forfeit (auto-triggered by check-in system).
    #[instrument(skip(self))]
    pub async fn process_no_show(
        &self,
        match_id: TournamentMatchId,
        no_show_registration_id: TournamentRegistrationId,
    ) -> Result<ForfeitResult, DomainError> {
        self.process_forfeit(
            match_id,
            no_show_registration_id,
            ForfeitType::NoShow,
            Some("Failed to check in for match".to_string()),
            ForfeitTrigger::System {
                reason: "check_in_timeout".to_string(),
            },
        )
        .await
    }

    /// Process a double forfeit (both teams forfeit).
    #[instrument(skip(self))]
    pub async fn process_double_forfeit(
        &self,
        match_id: TournamentMatchId,
        reason: Option<String>,
        triggered_by: ForfeitTrigger,
    ) -> Result<ForfeitResult, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // Validate both participants exist
        let participant1_id = match_
            .participant1_registration_id
            .ok_or_else(|| DomainError::InvalidState("No participant 1 in match".to_string()))?;
        let participant2_id = match_
            .participant2_registration_id
            .ok_or_else(|| DomainError::InvalidState("No participant 2 in match".to_string()))?;

        // Call the match off first (guarded: a match that was decided in
        // the meantime — a result, a one-sided forfeit — is a `Conflict`,
        // and nothing else is written). Pending result claims are superseded
        // in the same statement. A match that is *already* cancelled is a
        // retry of a double forfeit that died before both records were
        // written: skip the status write and finish the (idempotent) records.
        if match_.status != TournamentMatchStatus::Cancelled {
            self.match_repo.cancel_without_result(match_id).await?;
        }

        // Create forfeit records for both teams
        let forfeit_record = self
            .forfeit_repo
            .create(CreateForfeitRecord {
                match_id,
                forfeiting_registration_id: participant1_id,
                forfeit_type: ForfeitType::Disqualification,
                reason: reason.clone(),
                triggered_by_user_id: triggered_by.user_id(),
                triggered_by_system: triggered_by.is_system(),
            })
            .await?;

        // Create second forfeit record
        self.forfeit_repo
            .create(CreateForfeitRecord {
                match_id,
                forfeiting_registration_id: participant2_id,
                forfeit_type: ForfeitType::Disqualification,
                reason,
                triggered_by_user_id: triggered_by.user_id(),
                triggered_by_system: triggered_by.is_system(),
            })
            .await?;

        info!(
            match_id = %match_id,
            "Processed double forfeit"
        );

        Ok(ForfeitResult {
            match_id,
            forfeit_record,
            winner_registration_id: None,
            progression_triggered: false,
        })
    }

    /// Withdraw a team from the tournament.
    ///
    /// The tournament's `withdrawal_policy` decides what happens to the
    /// matches the team has not played:
    ///
    /// - `Forfeit` (default): every unplayed match it is seated in becomes a
    ///   walkover for the opponent (see [`Self::forfeit_remaining_matches`]).
    /// - `AdminDecision`: nothing is forfeited automatically — the
    ///   registration is marked withdrawn and its matches are left for an
    ///   admin to forfeit, cancel or reassign.
    /// - `Reseeding` / `WaitlistPromotion`: there is no mid-event reseed or
    ///   waitlist substitution, so once matches exist these fall back to
    ///   `Forfeit` — the only policy that leaves the bracket decidable.
    #[instrument(skip(self))]
    pub async fn withdraw_from_tournament(
        &self,
        tournament_id: TournamentId,
        registration_id: TournamentRegistrationId,
        reason: Option<String>,
        withdrawn_by: UserId,
        policy: WithdrawalPolicy,
    ) -> Result<Vec<ForfeitResult>, DomainError> {
        // Get the registration
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))?;

        // Verify registration belongs to this tournament
        if registration.tournament_id != tournament_id {
            return Err(DomainError::not_authorized(
                "Registration does not belong to this tournament",
            ));
        }

        // Withdrawing mid-cup is this path's purpose (it forfeits what is
        // left), but an entry that is already over — eliminated, withdrawn
        // (also how a rejection is stored), disqualified, no-show — has
        // nothing to withdraw from. Without this, withdrawing an eliminated
        // entry rewrote its result to `withdrawn`.
        if !registration.status.can_withdraw() {
            return Err(DomainError::InvalidState(format!(
                "Cannot withdraw a registration in {} status",
                registration.status
            )));
        }

        // Update registration status to withdrawn
        self.registration_repo
            .update_status(registration_id, TournamentRegistrationStatus::Withdrawn)
            .await?;

        let results = if policy == WithdrawalPolicy::AdminDecision {
            Vec::new()
        } else {
            self.forfeit_remaining_matches(
                registration_id,
                ForfeitType::Withdrawal,
                reason.clone(),
                ForfeitTrigger::User(withdrawn_by),
            )
            .await?
        };

        info!(
            tournament_id = %tournament_id,
            registration_id = %registration_id,
            forfeited_matches = results.len(),
            "Processed tournament withdrawal"
        );

        Ok(results)
    }

    /// Disqualify a team from the tournament.
    #[instrument(skip(self))]
    pub async fn disqualify(
        &self,
        tournament_id: TournamentId,
        registration_id: TournamentRegistrationId,
        reason: String,
        disqualified_by: UserId,
    ) -> Result<Vec<ForfeitResult>, DomainError> {
        // Get the registration
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))?;

        // Verify registration belongs to this tournament
        if registration.tournament_id != tournament_id {
            return Err(DomainError::not_authorized(
                "Registration does not belong to this tournament",
            ));
        }

        // Update registration status to disqualified
        self.registration_repo
            .update_status(registration_id, TournamentRegistrationStatus::Disqualified)
            .await?;

        let results = self
            .forfeit_remaining_matches(
                registration_id,
                ForfeitType::Disqualification,
                Some(reason.clone()),
                ForfeitTrigger::Admin {
                    user_id: disqualified_by,
                    reason: reason.clone(),
                },
            )
            .await?;

        info!(
            tournament_id = %tournament_id,
            registration_id = %registration_id,
            reason = %reason,
            forfeited_matches = results.len(),
            "Processed disqualification"
        );

        Ok(results)
    }

    /// Forfeit every match a registration still has to play.
    ///
    /// "Still has to play" is every match it is seated in whose status admits
    /// an administrative forfeit — `Ready`, `Scheduled`, `CheckingIn`,
    /// `PickBan`, `InProgress`, `AwaitingResult`. That includes `Ready`, the
    /// status every round-robin match is generated in (the old filter picked
    /// `Ready` and `Pending` and `process_forfeit` then refused both, so a
    /// round-robin withdrawal forfeited nothing). Left alone:
    ///
    /// - `Pending` — the opponent is not known yet, there is nobody to award
    ///   the walkover to;
    /// - `Disputed` — a result is already on the table; the dispute decides it;
    /// - anything terminal.
    ///
    /// Used by withdrawal and both disqualification endpoints. A failure on
    /// one match is logged and does not stop the others.
    #[instrument(skip(self, reason, triggered_by))]
    pub async fn forfeit_remaining_matches(
        &self,
        registration_id: TournamentRegistrationId,
        forfeit_type: ForfeitType,
        reason: Option<String>,
        triggered_by: ForfeitTrigger,
    ) -> Result<Vec<ForfeitResult>, DomainError> {
        let pending_matches = self.find_pending_matches(registration_id).await?;

        let mut results = Vec::new();
        for match_ in pending_matches {
            match self
                .process_forfeit(
                    match_.id,
                    registration_id,
                    forfeit_type,
                    reason.clone(),
                    triggered_by.clone(),
                )
                .await
            {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!(
                        match_id = %match_.id,
                        error = %e,
                        forfeit_type = %forfeit_type,
                        "Failed to forfeit remaining match"
                    );
                }
            }
        }

        Ok(results)
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

    fn validate_can_forfeit(
        &self,
        match_: &TournamentMatch,
        forfeiting_registration_id: TournamentRegistrationId,
    ) -> Result<(), DomainError> {
        // Check if match can be forfeited based on status. Administrative /
        // system forfeits also act on `Ready` matches (see
        // `can_forfeit_administratively`).
        if !match_.status.can_forfeit_administratively() {
            return Err(DomainError::InvalidState(format!(
                "Match in {} status cannot be forfeited",
                match_.status
            )));
        }

        // Check if the forfeiting party is a participant
        let is_participant1 =
            match_.participant1_registration_id == Some(forfeiting_registration_id);
        let is_participant2 =
            match_.participant2_registration_id == Some(forfeiting_registration_id);

        if !is_participant1 && !is_participant2 {
            return Err(DomainError::not_authorized(format!(
                "Registration {} is not a participant in match {}",
                forfeiting_registration_id, match_.id
            )));
        }

        Ok(())
    }

    fn determine_winner(
        &self,
        match_: &TournamentMatch,
        forfeiting_registration_id: TournamentRegistrationId,
    ) -> Result<TournamentRegistrationId, DomainError> {
        if match_.participant1_registration_id == Some(forfeiting_registration_id) {
            match_
                .participant2_registration_id
                .ok_or_else(|| DomainError::InvalidState("No opponent to award win".to_string()))
        } else {
            match_
                .participant1_registration_id
                .ok_or_else(|| DomainError::InvalidState("No opponent to award win".to_string()))
        }
    }

    async fn find_pending_matches(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let matches = self.match_repo.list_by_participant(registration_id).await?;

        // Exactly the matches `validate_can_forfeit` will accept, so nothing
        // selected here is then refused (and silently skipped).
        Ok(matches
            .into_iter()
            .filter(|m| m.status.can_forfeit_administratively() && m.has_both_participants())
            .collect())
    }
}

// =============================================================================
// DEPARTED PARTICIPANTS
// =============================================================================

/// Forfeits matches on behalf of participants who have left the tournament.
///
/// [`ForfeitService::forfeit_remaining_matches`] deals with the matches a
/// participant is in when it withdraws or is disqualified. This deals with
/// the ones it only reaches *afterwards* — see
/// [`MatchCompletionSaga::settle_departed`](super::MatchCompletionSaga::settle_departed),
/// which calls it for every match progression makes playable.
#[derive(Clone)]
pub struct DepartedForfeiter<FRR, TMR, TRR, TR> {
    forfeits: ForfeitService<FRR, TMR, TRR>,
    tournament_repo: Arc<TR>,
}

impl<FRR, TMR, TRR, TR> DepartedForfeiter<FRR, TMR, TRR, TR>
where
    FRR: ForfeitRecordRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    TR: TournamentRepository,
{
    /// Create a new departed-participant forfeiter.
    pub const fn new(forfeits: ForfeitService<FRR, TMR, TRR>, tournament_repo: Arc<TR>) -> Self {
        Self {
            forfeits,
            tournament_repo,
        }
    }

    /// The forfeit type owed by a registration that has left, or `None` if
    /// it is still in the event (or its matches are an admin's call).
    async fn departure(
        &self,
        tournament_id: TournamentId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Option<ForfeitType>, DomainError> {
        let registration = self
            .forfeits
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))?;
        Ok(match registration.status {
            TournamentRegistrationStatus::Disqualified => Some(ForfeitType::Disqualification),
            TournamentRegistrationStatus::NoShow => Some(ForfeitType::NoShow),
            TournamentRegistrationStatus::Withdrawn => {
                // Same policy the withdrawal itself applied to the matches
                // the team was in: under `AdminDecision` nothing is forfeited
                // automatically.
                let policy = self
                    .tournament_repo
                    .find_by_id(tournament_id)
                    .await?
                    .ok_or(DomainError::TournamentNotFound(tournament_id))?
                    .withdrawal_policy;
                (policy != WithdrawalPolicy::AdminDecision).then_some(ForfeitType::Withdrawal)
            }
            _ => None,
        })
    }
}

#[async_trait::async_trait]
impl<FRR, TMR, TRR, TR> DepartedParticipantForfeiter for DepartedForfeiter<FRR, TMR, TRR, TR>
where
    FRR: ForfeitRecordRepository + 'static,
    TMR: TournamentMatchRepository + 'static,
    TRR: TournamentRegistrationRepository + 'static,
    TR: TournamentRepository + 'static,
{
    #[instrument(skip(self))]
    async fn forfeit_if_departed(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<DepartureOutcome>, DomainError> {
        let match_ = self.forfeits.get_match(match_id).await?;
        let (Some(p1), Some(p2)) = (
            match_.participant1_registration_id,
            match_.participant2_registration_id,
        ) else {
            return Ok(None);
        };
        if !match_.status.can_forfeit_administratively() {
            return Ok(None);
        }

        let d1 = self.departure(match_.tournament_id, p1).await?;
        let d2 = self.departure(match_.tournament_id, p2).await?;
        let trigger = ForfeitTrigger::System {
            reason: "participant_departed".to_string(),
        };

        let result = match (d1, d2) {
            (None, None) => return Ok(None),
            (Some(_), Some(_)) => self
                .forfeits
                .process_double_forfeit(
                    match_id,
                    Some("Both participants have left the tournament".to_string()),
                    trigger,
                )
                .await
                .map(|_| DepartureOutcome::CalledOff),
            (Some(forfeit_type), None) | (None, Some(forfeit_type)) => {
                let departed = if d1.is_some() { p1 } else { p2 };
                self.forfeits
                    .process_forfeit(
                        match_id,
                        departed,
                        forfeit_type,
                        Some("Participant has left the tournament".to_string()),
                        trigger,
                    )
                    .await
                    .map(|_| DepartureOutcome::Walkover)
            }
        };

        match result {
            Ok(outcome) => Ok(Some(outcome)),
            // Decided by someone else between the read above and the write.
            Err(DomainError::Conflict(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forfeit_type_default_score() {
        use portal_core::types::MatchFormat;

        let bo3 = MatchFormat::Bo3;
        assert_eq!(ForfeitType::NoShow.default_score(bo3), (2, 0));

        let bo5 = MatchFormat::Bo5;
        assert_eq!(ForfeitType::Withdrawal.default_score(bo5), (3, 0));
    }
}
