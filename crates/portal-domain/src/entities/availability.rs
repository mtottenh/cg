//! Availability system domain entities.
//!
//! Provides types for managing player/team availability windows and scheduling suggestions.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use portal_core::{
    AvailabilityExceptionId, AvailabilityWindowId, PlayerId, SuggestedTimeId, TournamentMatchId,
    TournamentRegistrationId,
};

/// Recurring weekly availability window.
///
/// Represents a time slot during which a player or team registration is available
/// on a specific day of the week (e.g., "Monday 6pm-10pm").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityWindow {
    pub id: AvailabilityWindowId,

    /// Player this availability belongs to (if player-based).
    pub player_id: Option<PlayerId>,

    /// Tournament registration this availability belongs to (if registration-based).
    pub registration_id: Option<TournamentRegistrationId>,

    /// Day of week (0 = Sunday, 1 = Monday, ... 6 = Saturday).
    pub day_of_week: u8,

    /// Start time of the availability window (UTC).
    pub start_time: NaiveTime,

    /// End time of the availability window (UTC).
    pub end_time: NaiveTime,

    /// Optional timezone preference for display.
    pub timezone: Option<String>,

    /// Whether this is a preference (soft constraint) or hard constraint.
    pub is_preferred: bool,

    /// Optional notes about this availability.
    pub notes: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Command for creating an availability window.
#[derive(Debug, Clone)]
pub struct CreateAvailabilityWindow {
    pub player_id: Option<PlayerId>,
    pub registration_id: Option<TournamentRegistrationId>,
    pub day_of_week: u8,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub timezone: Option<String>,
    pub is_preferred: bool,
    pub notes: Option<String>,
}

/// Command for updating an availability window.
#[derive(Debug, Clone, Default)]
pub struct UpdateAvailabilityWindow {
    pub day_of_week: Option<u8>,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub timezone: Option<Option<String>>,
    pub is_preferred: Option<bool>,
    pub notes: Option<Option<String>>,
}

/// One-time date override for availability.
///
/// Can be used to mark a specific date as blocked (vacation, etc.)
/// or to add extra availability outside regular windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityOverride {
    pub id: AvailabilityExceptionId,

    /// Player this override belongs to (if player-based).
    pub player_id: Option<PlayerId>,

    /// Tournament registration this override belongs to (if registration-based).
    pub registration_id: Option<TournamentRegistrationId>,

    /// The specific date this override applies to.
    pub override_date: NaiveDate,

    /// Start time (None = all day).
    pub start_time: Option<NaiveTime>,

    /// End time (None = all day).
    pub end_time: Option<NaiveTime>,

    /// Type of override.
    pub override_type: OverrideType,

    /// Reason for the override.
    pub reason: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Type of availability override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideType {
    /// Player is unavailable during this time.
    Blocked,
    /// Player has extra availability during this time.
    Available,
}

impl std::fmt::Display for OverrideType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocked => write!(f, "blocked"),
            Self::Available => write!(f, "available"),
        }
    }
}

impl std::str::FromStr for OverrideType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "blocked" => Ok(Self::Blocked),
            "available" => Ok(Self::Available),
            _ => Err(format!("Invalid override type: {s}")),
        }
    }
}

/// Command for creating an availability override.
#[derive(Debug, Clone)]
pub struct CreateAvailabilityOverride {
    pub player_id: Option<PlayerId>,
    pub registration_id: Option<TournamentRegistrationId>,
    pub override_date: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub override_type: OverrideType,
    pub reason: Option<String>,
}

/// Suggested meeting time for a match.
///
/// Auto-generated or manually created suggestions based on
/// participant availability overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestedTime {
    pub id: SuggestedTimeId,

    /// Match this suggestion is for.
    pub match_id: TournamentMatchId,

    /// Suggested start time.
    pub suggested_start: DateTime<Utc>,

    /// Suggested end time.
    pub suggested_end: DateTime<Utc>,

    /// Confidence score (0-100) based on overlap quality.
    pub confidence_score: i32,

    /// Whether both participants are available at this time.
    pub is_mutual_overlap: bool,

    /// Whether this was auto-generated or manually suggested.
    pub is_auto_generated: bool,

    /// Current status of this suggestion.
    pub status: SuggestionStatus,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Status of a time suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionStatus {
    /// Suggestion is pending review.
    Suggested,
    /// Suggestion was accepted.
    Accepted,
    /// Suggestion was rejected.
    Rejected,
    /// Suggestion has expired.
    Expired,
}

impl std::fmt::Display for SuggestionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Suggested => write!(f, "suggested"),
            Self::Accepted => write!(f, "accepted"),
            Self::Rejected => write!(f, "rejected"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

impl std::str::FromStr for SuggestionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "suggested" => Ok(Self::Suggested),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            _ => Err(format!("Invalid suggestion status: {s}")),
        }
    }
}

/// Command for creating a time suggestion.
#[derive(Debug, Clone)]
pub struct CreateSuggestedTime {
    pub match_id: TournamentMatchId,
    pub suggested_start: DateTime<Utc>,
    pub suggested_end: DateTime<Utc>,
    pub confidence_score: i32,
    pub is_mutual_overlap: bool,
    pub is_auto_generated: bool,
}

/// Availability for a specific date, combining windows and overrides.
#[derive(Debug, Clone)]
pub struct DateAvailability {
    /// The date this availability is for.
    pub date: NaiveDate,

    /// Available time slots on this date.
    pub available_slots: Vec<TimeSlot>,

    /// Whether this date is fully blocked.
    pub is_blocked: bool,

    /// Any notes for this date.
    pub notes: Vec<String>,
}

/// A time slot with start and end times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSlot {
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub is_preferred: bool,
}

impl TimeSlot {
    /// Check if this slot overlaps with another.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Find the intersection of two time slots.
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        if !self.overlaps(other) {
            return None;
        }

        let start = self.start.max(other.start);
        let end = self.end.min(other.end);

        Some(Self {
            start,
            end,
            is_preferred: self.is_preferred && other.is_preferred,
        })
    }
}
