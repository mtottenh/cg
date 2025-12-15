//! Forfeit record repository trait.

use async_trait::async_trait;
use portal_core::errors::DomainError;
use portal_core::ids::{ForfeitRecordId, TournamentMatchId, TournamentRegistrationId, UserId};

use crate::entities::forfeit::{ForfeitRecord, ForfeitType};

/// Data for creating a forfeit record.
#[derive(Debug, Clone)]
pub struct CreateForfeitRecord {
    pub match_id: TournamentMatchId,
    pub forfeiting_registration_id: TournamentRegistrationId,
    pub forfeit_type: ForfeitType,
    pub reason: Option<String>,
    pub triggered_by_user_id: Option<UserId>,
    pub triggered_by_system: bool,
}

/// Repository for forfeit records.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ForfeitRecordRepository: Send + Sync + 'static {
    /// Create a new forfeit record.
    async fn create(&self, data: CreateForfeitRecord) -> Result<ForfeitRecord, DomainError>;

    /// Find a forfeit record by ID.
    async fn find_by_id(&self, id: ForfeitRecordId) -> Result<Option<ForfeitRecord>, DomainError>;

    /// Find forfeit record for a match (there can only be one per match).
    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ForfeitRecord>, DomainError>;

    /// Find all forfeit records for a registration.
    async fn find_by_registration(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<ForfeitRecord>, DomainError>;

    /// Check if a match has already been forfeited.
    async fn exists_for_match(&self, match_id: TournamentMatchId) -> Result<bool, DomainError>;
}
