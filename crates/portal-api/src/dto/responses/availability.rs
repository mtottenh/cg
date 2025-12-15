//! Availability response DTOs.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use portal_domain::entities::{
    AvailabilityOverride, AvailabilityWindow, DateAvailability, SuggestedTime, TimeSlot,
};

/// Response for an availability window.
#[derive(Debug, Serialize, ToSchema)]
pub struct AvailabilityWindowResponse {
    pub id: String,
    pub player_id: Option<String>,
    pub registration_id: Option<String>,
    pub day_of_week: u8,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub timezone: Option<String>,
    pub is_preferred: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<AvailabilityWindow> for AvailabilityWindowResponse {
    fn from(w: AvailabilityWindow) -> Self {
        Self {
            id: w.id.to_string(),
            player_id: w.player_id.map(|id| id.to_string()),
            registration_id: w.registration_id.map(|id| id.to_string()),
            day_of_week: w.day_of_week,
            start_time: w.start_time,
            end_time: w.end_time,
            timezone: w.timezone,
            is_preferred: w.is_preferred,
            notes: w.notes,
            created_at: w.created_at,
            updated_at: w.updated_at,
        }
    }
}

/// Response for an availability override.
#[derive(Debug, Serialize, ToSchema)]
pub struct AvailabilityOverrideResponse {
    pub id: String,
    pub player_id: Option<String>,
    pub registration_id: Option<String>,
    pub override_date: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub override_type: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<AvailabilityOverride> for AvailabilityOverrideResponse {
    fn from(o: AvailabilityOverride) -> Self {
        Self {
            id: o.id.to_string(),
            player_id: o.player_id.map(|id| id.to_string()),
            registration_id: o.registration_id.map(|id| id.to_string()),
            override_date: o.override_date,
            start_time: o.start_time,
            end_time: o.end_time,
            override_type: o.override_type.to_string(),
            reason: o.reason,
            created_at: o.created_at,
            updated_at: o.updated_at,
        }
    }
}

/// Response for a time slot.
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeSlotResponse {
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub is_preferred: bool,
}

impl From<TimeSlot> for TimeSlotResponse {
    fn from(s: TimeSlot) -> Self {
        Self {
            start: s.start,
            end: s.end,
            is_preferred: s.is_preferred,
        }
    }
}

/// Response for availability on a specific date.
#[derive(Debug, Serialize, ToSchema)]
pub struct DateAvailabilityResponse {
    pub date: NaiveDate,
    pub available_slots: Vec<TimeSlotResponse>,
    pub is_blocked: bool,
    pub notes: Vec<String>,
}

impl From<DateAvailability> for DateAvailabilityResponse {
    fn from(a: DateAvailability) -> Self {
        Self {
            date: a.date,
            available_slots: a.available_slots.into_iter().map(Into::into).collect(),
            is_blocked: a.is_blocked,
            notes: a.notes,
        }
    }
}

/// Response for a suggested time.
#[derive(Debug, Serialize, ToSchema)]
pub struct SuggestedTimeResponse {
    pub id: String,
    pub match_id: String,
    pub suggested_start: DateTime<Utc>,
    pub suggested_end: DateTime<Utc>,
    pub confidence_score: i32,
    pub is_mutual_overlap: bool,
    pub is_auto_generated: bool,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<SuggestedTime> for SuggestedTimeResponse {
    fn from(s: SuggestedTime) -> Self {
        Self {
            id: s.id.to_string(),
            match_id: s.match_id.to_string(),
            suggested_start: s.suggested_start,
            suggested_end: s.suggested_end,
            confidence_score: s.confidence_score,
            is_mutual_overlap: s.is_mutual_overlap,
            is_auto_generated: s.is_auto_generated,
            status: s.status.to_string(),
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}
