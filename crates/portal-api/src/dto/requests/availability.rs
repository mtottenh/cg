//! Availability request DTOs.

use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request to create an availability window.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateAvailabilityWindowRequest {
    /// Day of week (0 = Sunday, 1 = Monday, ... 6 = Saturday).
    #[validate(range(min = 0, max = 6))]
    pub day_of_week: u8,

    /// Start time (HH:MM:SS format).
    pub start_time: NaiveTime,

    /// End time (HH:MM:SS format).
    pub end_time: NaiveTime,

    /// Optional timezone preference for display.
    pub timezone: Option<String>,

    /// Whether this is a preferred time (true) or just available (false).
    #[serde(default = "default_true")]
    pub is_preferred: bool,

    /// Optional notes.
    pub notes: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Request to update an availability window.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateAvailabilityWindowRequest {
    /// Day of week (0 = Sunday, 1 = Monday, ... 6 = Saturday).
    #[validate(range(min = 0, max = 6))]
    pub day_of_week: Option<u8>,

    /// Start time (HH:MM:SS format).
    pub start_time: Option<NaiveTime>,

    /// End time (HH:MM:SS format).
    pub end_time: Option<NaiveTime>,

    /// Timezone preference for display.
    pub timezone: Option<String>,

    /// Whether this is a preferred time.
    pub is_preferred: Option<bool>,

    /// Optional notes.
    pub notes: Option<String>,
}

/// Request to create an availability override.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateAvailabilityOverrideRequest {
    /// The specific date for this override.
    pub override_date: NaiveDate,

    /// Start time (null = all day).
    pub start_time: Option<NaiveTime>,

    /// End time (null = all day).
    pub end_time: Option<NaiveTime>,

    /// Type of override: "blocked" or "available".
    pub override_type: String,

    /// Reason for the override.
    pub reason: Option<String>,
}

/// Request to generate time suggestions for a match.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct GenerateSuggestionsRequest {
    /// Start date for suggestion range.
    pub start_date: NaiveDate,

    /// End date for suggestion range.
    pub end_date: NaiveDate,

    /// Minimum match duration in minutes.
    #[serde(default = "default_duration")]
    #[validate(range(min = 15, max = 480))]
    pub min_duration_minutes: i64,
}

fn default_duration() -> i64 {
    60
}

/// Query parameters for getting availability on a date.
#[derive(Debug, Deserialize, ToSchema)]
pub struct GetAvailabilityQuery {
    /// The date to get availability for.
    pub date: NaiveDate,
}
