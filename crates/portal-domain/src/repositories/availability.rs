//! Availability repository traits.

use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use portal_core::{
    AvailabilityExceptionId, AvailabilityWindowId, DomainError, PlayerId, SuggestedTimeId,
    TournamentMatchId, TournamentRegistrationId,
};

use crate::entities::{
    AvailabilityOverride, AvailabilityWindow, CreateAvailabilityOverride, CreateAvailabilityWindow,
    CreateSuggestedTime, SuggestedTime, UpdateAvailabilityWindow,
};

/// Repository for availability windows.
#[async_trait]
pub trait AvailabilityWindowRepository: Send + Sync + 'static {
    /// Find an availability window by ID.
    async fn find_by_id(
        &self,
        id: AvailabilityWindowId,
    ) -> Result<Option<AvailabilityWindow>, DomainError>;

    /// Find all availability windows for a player.
    async fn find_by_player_id(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    /// Find all availability windows for a tournament registration.
    async fn find_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    /// Find availability windows for a player on a specific day.
    async fn find_by_player_and_day(
        &self,
        player_id: PlayerId,
        day_of_week: u8,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    /// Find availability windows for a registration on a specific day.
    async fn find_by_registration_and_day(
        &self,
        registration_id: TournamentRegistrationId,
        day_of_week: u8,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    /// Create a new availability window.
    async fn create(
        &self,
        command: CreateAvailabilityWindow,
    ) -> Result<AvailabilityWindow, DomainError>;

    /// Update an availability window.
    async fn update(
        &self,
        id: AvailabilityWindowId,
        command: UpdateAvailabilityWindow,
    ) -> Result<AvailabilityWindow, DomainError>;

    /// Delete an availability window.
    async fn delete(&self, id: AvailabilityWindowId) -> Result<bool, DomainError>;

    /// Delete all availability windows for a player.
    async fn delete_by_player_id(&self, player_id: PlayerId) -> Result<u64, DomainError>;

    /// Delete all availability windows for a registration.
    async fn delete_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<u64, DomainError>;

    /// Check if a slot already exists (to prevent duplicates).
    async fn exists(
        &self,
        player_id: Option<PlayerId>,
        registration_id: Option<TournamentRegistrationId>,
        day_of_week: u8,
        start_time: NaiveTime,
        end_time: NaiveTime,
    ) -> Result<bool, DomainError>;
}

/// Repository for availability overrides.
#[async_trait]
pub trait AvailabilityOverrideRepository: Send + Sync + 'static {
    /// Find an override by ID.
    async fn find_by_id(
        &self,
        id: AvailabilityExceptionId,
    ) -> Result<Option<AvailabilityOverride>, DomainError>;

    /// Find all overrides for a player.
    async fn find_by_player_id(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<AvailabilityOverride>, DomainError>;

    /// Find all overrides for a registration.
    async fn find_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<AvailabilityOverride>, DomainError>;

    /// Find overrides for a player within a date range.
    async fn find_by_player_id_and_date_range(
        &self,
        player_id: PlayerId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<AvailabilityOverride>, DomainError>;

    /// Find overrides for a registration within a date range.
    async fn find_by_registration_id_and_date_range(
        &self,
        registration_id: TournamentRegistrationId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<AvailabilityOverride>, DomainError>;

    /// Create a new override.
    async fn create(
        &self,
        command: CreateAvailabilityOverride,
    ) -> Result<AvailabilityOverride, DomainError>;

    /// Delete an override.
    async fn delete(&self, id: AvailabilityExceptionId) -> Result<bool, DomainError>;

    /// Delete all overrides for a player.
    async fn delete_by_player_id(&self, player_id: PlayerId) -> Result<u64, DomainError>;

    /// Delete all overrides for a registration.
    async fn delete_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<u64, DomainError>;

    /// Delete expired overrides (past dates).
    async fn delete_expired(&self, before_date: NaiveDate) -> Result<u64, DomainError>;
}

/// Repository for suggested times.
#[async_trait]
pub trait SuggestedTimeRepository: Send + Sync + 'static {
    /// Find a suggestion by ID.
    async fn find_by_id(&self, id: SuggestedTimeId) -> Result<Option<SuggestedTime>, DomainError>;

    /// Find all suggestions for a match.
    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<SuggestedTime>, DomainError>;

    /// Find active (non-expired/rejected) suggestions for a match.
    async fn find_active_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<SuggestedTime>, DomainError>;

    /// Create a new suggestion.
    async fn create(&self, command: CreateSuggestedTime) -> Result<SuggestedTime, DomainError>;

    /// Mark a suggestion as accepted.
    async fn accept(&self, id: SuggestedTimeId) -> Result<SuggestedTime, DomainError>;

    /// Mark a suggestion as rejected.
    async fn reject(&self, id: SuggestedTimeId) -> Result<SuggestedTime, DomainError>;

    /// Mark a suggestion as expired.
    async fn expire(&self, id: SuggestedTimeId) -> Result<SuggestedTime, DomainError>;

    /// Delete all suggestions for a match.
    async fn delete_by_match_id(&self, match_id: TournamentMatchId) -> Result<u64, DomainError>;

    /// Delete auto-generated, still-pending suggestions for a match.
    ///
    /// Used to give suggestion regeneration replace semantics: stale
    /// auto-generated proposals are cleared before a fresh pass, so
    /// regenerating does not accumulate duplicates. Manually-suggested slots
    /// and any already accepted/rejected/expired decisions are preserved.
    async fn delete_auto_generated_pending(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<u64, DomainError>;

    /// Reject all non-accepted suggestions for a match (when match is scheduled).
    async fn reject_all_pending(&self, match_id: TournamentMatchId) -> Result<u64, DomainError>;
}
