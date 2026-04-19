//! League season participant service for individual format.

use crate::entities::league_team::{LeagueSeasonParticipant, LeagueSeasonParticipantStatus};
use crate::repositories::league_team::{
    LeagueSeasonParticipantRepository, LeagueSeasonRepository, RegisterLeagueSeasonParticipant,
};
use portal_core::{DomainError, LeagueSeasonId, PlayerId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for individual league participation (1v1 tournaments).
pub struct LeagueSeasonParticipantService<PR, SR>
where
    PR: LeagueSeasonParticipantRepository,
    SR: LeagueSeasonRepository,
{
    participant_repo: Arc<PR>,
    season_repo: Arc<SR>,
}

impl<PR, SR> LeagueSeasonParticipantService<PR, SR>
where
    PR: LeagueSeasonParticipantRepository,
    SR: LeagueSeasonRepository,
{
    /// Create a new participant service.
    pub const fn new(participant_repo: Arc<PR>, season_repo: Arc<SR>) -> Self {
        Self {
            participant_repo,
            season_repo,
        }
    }

    /// Register a player for an individual format season.
    #[instrument(skip(self))]
    pub async fn register(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<LeagueSeasonParticipant, DomainError> {
        let season = self
            .season_repo
            .find_by_id(season_id)
            .await?
            .ok_or_else(|| DomainError::LeagueSeasonNotFound(season_id))?;

        if !season.is_registration_open() {
            return Err(DomainError::RegistrationClosed);
        }

        // Check if already registered
        if self
            .participant_repo
            .is_registered(season_id, player_id)
            .await?
        {
            return Err(DomainError::Conflict(
                "player is already registered for this season".to_string(),
            ));
        }

        let participant = self
            .participant_repo
            .register(RegisterLeagueSeasonParticipant {
                season_id,
                player_id,
            })
            .await?;

        info!(
            participant_id = %participant.id,
            season_id = %season_id,
            player_id = %player_id,
            "Player registered for individual format season"
        );

        Ok(participant)
    }

    /// Withdraw from a season.
    #[instrument(skip(self))]
    pub async fn withdraw(
        &self,
        participant_id: uuid::Uuid,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        let participant = self
            .participant_repo
            .find_by_id(participant_id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed { resource: "participant", query: participant_id.to_string() })?;

        // Verify the player is the participant
        if participant.player_id != player_id {
            return Err(DomainError::NotAuthorized(
                "only the participant can withdraw".to_string(),
            ));
        }

        if participant.status == LeagueSeasonParticipantStatus::Withdrawn {
            return Err(DomainError::InvalidState(
                "already withdrawn from this season".to_string(),
            ));
        }

        self.participant_repo.withdraw(participant_id).await?;

        info!(
            participant_id = %participant_id,
            player_id = %player_id,
            "Player withdrew from season"
        );

        Ok(())
    }

    /// List participants in a season.
    #[instrument(skip(self))]
    pub async fn list_participants(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<LeagueSeasonParticipantStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueSeasonParticipant>, i64), DomainError> {
        let status_str = status_filter.map(|s| s.to_string());
        self.participant_repo
            .list_by_season(season_id, status_str, limit, offset)
            .await
    }

    /// Check if a player is registered for a season.
    pub async fn is_registered(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        self.participant_repo
            .is_registered(season_id, player_id)
            .await
    }
}

impl<PR, SR> Clone for LeagueSeasonParticipantService<PR, SR>
where
    PR: LeagueSeasonParticipantRepository,
    SR: LeagueSeasonRepository,
{
    fn clone(&self) -> Self {
        Self {
            participant_repo: Arc::clone(&self.participant_repo),
            season_repo: Arc::clone(&self.season_repo),
        }
    }
}
