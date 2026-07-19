//! Availability service for managing player/team scheduling availability.
//!
//! This service handles:
//! - CRUD operations for recurring weekly availability windows
//! - One-time availability overrides (blocked dates, extra availability)
//! - Finding overlapping availability between match participants
//! - Generating time suggestions based on availability

use std::sync::Arc;

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Utc};
use portal_core::{
    AvailabilityExceptionId, AvailabilityWindowId, DomainError, PlayerId, TournamentMatchId,
    TournamentRegistrationId,
};

use crate::entities::{
    AvailabilityOverride, AvailabilityWindow, CreateAvailabilityOverride, CreateAvailabilityWindow,
    CreateSuggestedTime, DateAvailability, OverrideType, SuggestedTime, TimeSlot,
    UpdateAvailabilityWindow,
};
use crate::repositories::{
    AvailabilityOverrideRepository, AvailabilityWindowRepository, SuggestedTimeRepository,
    TournamentMatchRepository, TournamentRegistrationRepository,
};

/// Service for managing availability and time suggestions.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AvailabilityService<AWR, AOR, STR, TMR, TRR>
where
    AWR: AvailabilityWindowRepository,
    AOR: AvailabilityOverrideRepository,
    STR: SuggestedTimeRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    window_repo: Arc<AWR>,
    override_repo: Arc<AOR>,
    suggestion_repo: Arc<STR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
}

impl<AWR, AOR, STR, TMR, TRR> AvailabilityService<AWR, AOR, STR, TMR, TRR>
where
    AWR: AvailabilityWindowRepository,
    AOR: AvailabilityOverrideRepository,
    STR: SuggestedTimeRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new availability service.
    pub fn new(
        window_repo: Arc<AWR>,
        override_repo: Arc<AOR>,
        suggestion_repo: Arc<STR>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
    ) -> Self {
        Self {
            window_repo,
            override_repo,
            suggestion_repo,
            match_repo,
            registration_repo,
        }
    }

    // =========================================================================
    // AVAILABILITY WINDOWS
    // =========================================================================

    /// Create a new availability window.
    pub async fn create_window(
        &self,
        command: CreateAvailabilityWindow,
    ) -> Result<AvailabilityWindow, DomainError> {
        // Validate that at least one owner is set
        if command.player_id.is_none() && command.registration_id.is_none() {
            return Err(DomainError::InvalidState(
                "Availability window must belong to a player or registration".to_string(),
            ));
        }

        // Validate day of week
        if command.day_of_week > 6 {
            return Err(DomainError::InvalidState(
                "Day of week must be 0-6 (Sunday-Saturday)".to_string(),
            ));
        }

        // Validate time range
        if command.start_time >= command.end_time {
            return Err(DomainError::InvalidState(
                "Start time must be before end time".to_string(),
            ));
        }

        // Check for duplicate
        if self
            .window_repo
            .exists(
                command.player_id,
                command.registration_id,
                command.day_of_week,
                command.start_time,
                command.end_time,
            )
            .await?
        {
            return Err(DomainError::conflict(
                "Availability window already exists for this time slot",
            ));
        }

        self.window_repo.create(command).await
    }

    /// Get an availability window by ID.
    pub async fn get_window(
        &self,
        id: AvailabilityWindowId,
    ) -> Result<Option<AvailabilityWindow>, DomainError> {
        self.window_repo.find_by_id(id).await
    }

    /// Get all availability windows for a player.
    pub async fn get_player_windows(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<AvailabilityWindow>, DomainError> {
        self.window_repo.find_by_player_id(player_id).await
    }

    /// Get all availability windows for a registration.
    pub async fn get_registration_windows(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<AvailabilityWindow>, DomainError> {
        self.window_repo
            .find_by_registration_id(registration_id)
            .await
    }

    /// Update an availability window.
    pub async fn update_window(
        &self,
        id: AvailabilityWindowId,
        command: UpdateAvailabilityWindow,
    ) -> Result<AvailabilityWindow, DomainError> {
        // Validate time range if provided
        if let (Some(start), Some(end)) = (command.start_time, command.end_time)
            && start >= end
        {
            return Err(DomainError::InvalidState(
                "Start time must be before end time".to_string(),
            ));
        }

        self.window_repo.update(id, command).await
    }

    /// Delete an availability window.
    pub async fn delete_window(&self, id: AvailabilityWindowId) -> Result<bool, DomainError> {
        self.window_repo.delete(id).await
    }

    /// Clear all availability windows for a player.
    pub async fn clear_player_windows(&self, player_id: PlayerId) -> Result<u64, DomainError> {
        self.window_repo.delete_by_player_id(player_id).await
    }

    /// Clear all availability windows for a registration.
    pub async fn clear_registration_windows(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<u64, DomainError> {
        self.window_repo
            .delete_by_registration_id(registration_id)
            .await
    }

    // =========================================================================
    // AVAILABILITY OVERRIDES
    // =========================================================================

    /// Create an availability override (blocked or available date).
    pub async fn create_override(
        &self,
        command: CreateAvailabilityOverride,
    ) -> Result<AvailabilityOverride, DomainError> {
        // Validate that at least one owner is set
        if command.player_id.is_none() && command.registration_id.is_none() {
            return Err(DomainError::InvalidState(
                "Availability override must belong to a player or registration".to_string(),
            ));
        }

        // Validate time range if provided
        if let (Some(start), Some(end)) = (command.start_time, command.end_time)
            && start >= end
        {
            return Err(DomainError::InvalidState(
                "Start time must be before end time".to_string(),
            ));
        }

        self.override_repo.create(command).await
    }

    /// Get an override by ID.
    pub async fn get_override(
        &self,
        id: AvailabilityExceptionId,
    ) -> Result<Option<AvailabilityOverride>, DomainError> {
        self.override_repo.find_by_id(id).await
    }

    /// Get all overrides for a player.
    pub async fn get_player_overrides(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<AvailabilityOverride>, DomainError> {
        self.override_repo.find_by_player_id(player_id).await
    }

    /// Get all overrides for a registration.
    pub async fn get_registration_overrides(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<AvailabilityOverride>, DomainError> {
        self.override_repo
            .find_by_registration_id(registration_id)
            .await
    }

    /// Get overrides for a player within a date range.
    pub async fn get_player_overrides_in_range(
        &self,
        player_id: PlayerId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<AvailabilityOverride>, DomainError> {
        self.override_repo
            .find_by_player_id_and_date_range(player_id, start_date, end_date)
            .await
    }

    /// Delete an override.
    pub async fn delete_override(&self, id: AvailabilityExceptionId) -> Result<bool, DomainError> {
        self.override_repo.delete(id).await
    }

    /// Delete expired overrides.
    pub async fn cleanup_expired_overrides(&self) -> Result<u64, DomainError> {
        let today = Utc::now().date_naive();
        self.override_repo.delete_expired(today).await
    }

    // =========================================================================
    // AVAILABILITY CALCULATION
    // =========================================================================

    /// Get availability for a specific date, combining windows and overrides.
    pub async fn get_availability_for_date(
        &self,
        player_id: Option<PlayerId>,
        registration_id: Option<TournamentRegistrationId>,
        date: NaiveDate,
    ) -> Result<DateAvailability, DomainError> {
        let day_of_week = date.weekday().num_days_from_sunday() as u8;

        // Get recurring windows for this day
        let windows = if let Some(pid) = player_id {
            self.window_repo
                .find_by_player_and_day(pid, day_of_week)
                .await?
        } else if let Some(rid) = registration_id {
            self.window_repo
                .find_by_registration_and_day(rid, day_of_week)
                .await?
        } else {
            return Err(DomainError::InvalidState(
                "Must provide player_id or registration_id".to_string(),
            ));
        };

        // Get overrides for this specific date
        let overrides = if let Some(pid) = player_id {
            self.override_repo
                .find_by_player_id_and_date_range(pid, date, date)
                .await?
        } else if let Some(rid) = registration_id {
            self.override_repo
                .find_by_registration_id_and_date_range(rid, date, date)
                .await?
        } else {
            vec![]
        };

        // Check if the entire day is blocked
        let all_day_blocked = overrides.iter().any(|o| {
            o.override_type == OverrideType::Blocked
                && o.start_time.is_none()
                && o.end_time.is_none()
        });

        if all_day_blocked {
            return Ok(DateAvailability {
                date,
                available_slots: vec![],
                is_blocked: true,
                notes: overrides.iter().filter_map(|o| o.reason.clone()).collect(),
            });
        }

        // Build base availability from windows
        let mut slots: Vec<TimeSlot> = windows
            .into_iter()
            .map(|w| TimeSlot {
                start: w.start_time,
                end: w.end_time,
                is_preferred: w.is_preferred,
            })
            .collect();

        // Apply overrides
        for ovr in &overrides {
            match ovr.override_type {
                OverrideType::Blocked => {
                    // Remove blocked time from slots
                    if let (Some(start), Some(end)) = (ovr.start_time, ovr.end_time) {
                        slots = slots
                            .into_iter()
                            .flat_map(|slot| remove_time_from_slot(&slot, start, end))
                            .collect();
                    }
                }
                OverrideType::Available => {
                    // Add extra availability
                    if let (Some(start), Some(end)) = (ovr.start_time, ovr.end_time) {
                        slots.push(TimeSlot {
                            start,
                            end,
                            is_preferred: true,
                        });
                    }
                }
            }
        }

        // Sort and merge overlapping slots
        slots = merge_time_slots(slots);

        Ok(DateAvailability {
            date,
            available_slots: slots,
            is_blocked: false,
            notes: overrides.iter().filter_map(|o| o.reason.clone()).collect(),
        })
    }

    // =========================================================================
    // TIME SUGGESTIONS
    // =========================================================================

    /// Generate time suggestions for a match based on participant availability.
    pub async fn generate_suggestions(
        &self,
        match_id: TournamentMatchId,
        start_date: NaiveDate,
        end_date: NaiveDate,
        min_duration_minutes: i64,
    ) -> Result<Vec<SuggestedTime>, DomainError> {
        // Get the match
        let tournament_match = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(match_id))?;

        let reg1_id = tournament_match
            .participant1_registration_id
            .ok_or_else(|| {
                DomainError::InvalidState("Match participant 1 not assigned".to_string())
            })?;
        let reg2_id = tournament_match
            .participant2_registration_id
            .ok_or_else(|| {
                DomainError::InvalidState("Match participant 2 not assigned".to_string())
            })?;

        let mut suggestions = Vec::new();

        // Iterate through each date in the range
        let mut current_date = start_date;
        while current_date <= end_date {
            // Get availability for both participants
            let avail1 = self
                .get_availability_for_date(None, Some(reg1_id), current_date)
                .await?;
            let avail2 = self
                .get_availability_for_date(None, Some(reg2_id), current_date)
                .await?;

            // Find overlapping slots
            for slot1 in &avail1.available_slots {
                for slot2 in &avail2.available_slots {
                    if let Some(overlap) = slot1.intersect(slot2) {
                        // Check if overlap is long enough
                        let duration_mins = (overlap.end - overlap.start).num_minutes();
                        if duration_mins >= min_duration_minutes {
                            // Convert to DateTime
                            let suggested_start = current_date.and_time(overlap.start).and_utc();
                            let suggested_end = current_date.and_time(overlap.end).and_utc();

                            // Calculate confidence score
                            let confidence = calculate_confidence_score(
                                overlap.is_preferred,
                                duration_mins,
                                min_duration_minutes,
                            );

                            let suggestion = self
                                .suggestion_repo
                                .create(CreateSuggestedTime {
                                    match_id,
                                    suggested_start,
                                    suggested_end,
                                    confidence_score: confidence,
                                    is_mutual_overlap: true,
                                    is_auto_generated: true,
                                })
                                .await?;

                            suggestions.push(suggestion);
                        }
                    }
                }
            }

            current_date += Duration::days(1);
        }

        // Sort by confidence score
        suggestions.sort_by(|a, b| b.confidence_score.cmp(&a.confidence_score));

        Ok(suggestions)
    }

    /// Get suggested times for a match.
    pub async fn get_suggestions(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<SuggestedTime>, DomainError> {
        self.suggestion_repo.find_by_match_id(match_id).await
    }

    /// Get active (pending) suggestions for a match.
    pub async fn get_active_suggestions(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<SuggestedTime>, DomainError> {
        self.suggestion_repo.find_active_by_match_id(match_id).await
    }

    /// Accept a time suggestion.
    pub async fn accept_suggestion(
        &self,
        suggestion_id: portal_core::SuggestedTimeId,
    ) -> Result<SuggestedTime, DomainError> {
        self.suggestion_repo.accept(suggestion_id).await
    }

    /// Reject a time suggestion.
    pub async fn reject_suggestion(
        &self,
        suggestion_id: portal_core::SuggestedTimeId,
    ) -> Result<SuggestedTime, DomainError> {
        self.suggestion_repo.reject(suggestion_id).await
    }

    /// Reject all pending suggestions for a match (e.g., when match is scheduled).
    pub async fn reject_all_pending_suggestions(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<u64, DomainError> {
        self.suggestion_repo.reject_all_pending(match_id).await
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Remove a time range from a slot, potentially splitting it.
fn remove_time_from_slot(
    slot: &TimeSlot,
    block_start: NaiveTime,
    block_end: NaiveTime,
) -> Vec<TimeSlot> {
    // No overlap
    if block_end <= slot.start || block_start >= slot.end {
        return vec![slot.clone()];
    }

    // Complete overlap - slot is entirely blocked
    if block_start <= slot.start && block_end >= slot.end {
        return vec![];
    }

    let mut result = Vec::new();

    // Partial overlap - keep unblocked parts
    if block_start > slot.start {
        result.push(TimeSlot {
            start: slot.start,
            end: block_start,
            is_preferred: slot.is_preferred,
        });
    }

    if block_end < slot.end {
        result.push(TimeSlot {
            start: block_end,
            end: slot.end,
            is_preferred: slot.is_preferred,
        });
    }

    result
}

/// Merge overlapping time slots.
fn merge_time_slots(mut slots: Vec<TimeSlot>) -> Vec<TimeSlot> {
    if slots.is_empty() {
        return slots;
    }

    // Sort by start time
    slots.sort_by(|a, b| a.start.cmp(&b.start));

    let mut merged = Vec::new();
    let mut current = slots.remove(0);

    for slot in slots {
        if slot.start <= current.end {
            // Overlapping or adjacent - merge
            current.end = current.end.max(slot.end);
            current.is_preferred = current.is_preferred && slot.is_preferred;
        } else {
            // No overlap - save current and start new
            merged.push(current);
            current = slot;
        }
    }

    merged.push(current);
    merged
}

/// Calculate confidence score for a time suggestion.
fn calculate_confidence_score(
    is_preferred: bool,
    duration_mins: i64,
    min_duration_mins: i64,
) -> i32 {
    let mut score = 50; // Base score

    // Bonus for preferred times
    if is_preferred {
        score += 20;
    }

    // Bonus for extra duration
    let extra_mins = duration_mins - min_duration_mins;
    if extra_mins > 0 {
        // Up to +30 for extra duration (1 point per 10 extra minutes, max 30)
        score += (extra_mins / 10).min(30) as i32;
    }

    score.min(100)
}
