//! Forfeit service.
//!
//! Handles forfeit processing for no-show, withdrawal, disqualification,
//! and technical default scenarios.

use std::sync::Arc;

use portal_core::types::{TournamentMatchStatus, TournamentRegistrationStatus};
use portal_core::{DomainError, TournamentId, TournamentMatchId, TournamentRegistrationId, UserId};
use tracing::{info, instrument, warn};

use crate::entities::forfeit::{ForfeitResult, ForfeitTrigger, ForfeitType};
use crate::entities::tournament::TournamentMatch;
use crate::repositories::forfeit::{CreateForfeitRecord, ForfeitRecordRepository};
use crate::repositories::tournament::{
    TournamentMatchRepository, TournamentRegistrationRepository,
};

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

        // Check if already forfeited
        if self.forfeit_repo.exists_for_match(match_id).await? {
            return Err(DomainError::InvalidState(format!(
                "Match {match_id} has already been forfeited"
            )));
        }

        // Determine the winner (opponent)
        let winner_registration_id = self.determine_winner(&match_, forfeiting_registration_id)?;

        // Create the forfeit record
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

        // Update match with forfeit result
        self.match_repo
            .forfeit(match_id, winner_registration_id, forfeiting_registration_id)
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
            progression_triggered: false, // Will be true when progression service is integrated
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

        // Check if already forfeited
        if self.forfeit_repo.exists_for_match(match_id).await? {
            return Err(DomainError::InvalidState(format!(
                "Match {match_id} has already been forfeited"
            )));
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

        // Update match status to cancelled (no winner)
        self.match_repo
            .update_status(match_id, TournamentMatchStatus::Cancelled)
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

    /// Withdraw a team from the tournament (forfeits all remaining matches).
    #[instrument(skip(self))]
    pub async fn withdraw_from_tournament(
        &self,
        tournament_id: TournamentId,
        registration_id: TournamentRegistrationId,
        reason: Option<String>,
        withdrawn_by: UserId,
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

        // Update registration status to withdrawn
        self.registration_repo
            .update_status(registration_id, TournamentRegistrationStatus::Withdrawn)
            .await?;

        // Find all pending matches for this registration
        let pending_matches = self.find_pending_matches(registration_id).await?;

        // Forfeit each pending match
        let mut results = Vec::new();
        for match_ in pending_matches {
            match self
                .process_forfeit(
                    match_.id,
                    registration_id,
                    ForfeitType::Withdrawal,
                    reason.clone(),
                    ForfeitTrigger::User(withdrawn_by),
                )
                .await
            {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!(
                        match_id = %match_.id,
                        error = %e,
                        "Failed to process withdrawal forfeit for match"
                    );
                }
            }
        }

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

        // Find all pending matches for this registration
        let pending_matches = self.find_pending_matches(registration_id).await?;

        // Forfeit each pending match
        let mut results = Vec::new();
        for match_ in pending_matches {
            match self
                .process_forfeit(
                    match_.id,
                    registration_id,
                    ForfeitType::Disqualification,
                    Some(reason.clone()),
                    ForfeitTrigger::Admin {
                        user_id: disqualified_by,
                        reason: reason.clone(),
                    },
                )
                .await
            {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!(
                        match_id = %match_.id,
                        error = %e,
                        "Failed to process disqualification forfeit for match"
                    );
                }
            }
        }

        info!(
            tournament_id = %tournament_id,
            registration_id = %registration_id,
            reason = %reason,
            forfeited_matches = results.len(),
            "Processed disqualification"
        );

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
        // Check if match can be forfeited based on status
        if !match_.status.can_forfeit() {
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

        // Filter to only pending/scheduled/ready matches
        Ok(matches
            .into_iter()
            .filter(|m| {
                matches!(
                    m.status,
                    TournamentMatchStatus::Pending
                        | TournamentMatchStatus::Scheduled
                        | TournamentMatchStatus::Ready
                        | TournamentMatchStatus::CheckingIn
                )
            })
            .collect())
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
