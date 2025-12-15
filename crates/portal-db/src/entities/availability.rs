//! Availability database row types.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use portal_core::{
    AvailabilityExceptionId, AvailabilityWindowId, PlayerId, SuggestedTimeId, TournamentMatchId,
    TournamentRegistrationId,
};
use portal_domain::entities::{
    AvailabilityOverride, AvailabilityWindow, OverrideType, SuggestedTime, SuggestionStatus,
};

/// Database row for availability_windows table.
#[derive(Debug, Clone, FromRow)]
pub struct AvailabilityWindowRow {
    pub id: Uuid,
    pub player_id: Option<Uuid>,
    pub registration_id: Option<Uuid>,
    pub day_of_week: i16,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub timezone: Option<String>,
    pub is_preferred: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<AvailabilityWindowRow> for AvailabilityWindow {
    fn from(row: AvailabilityWindowRow) -> Self {
        Self {
            id: AvailabilityWindowId::from_uuid(row.id),
            player_id: row.player_id.map(PlayerId::from_uuid),
            registration_id: row.registration_id.map(TournamentRegistrationId::from_uuid),
            day_of_week: row.day_of_week as u8,
            start_time: row.start_time,
            end_time: row.end_time,
            timezone: row.timezone,
            is_preferred: row.is_preferred,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Database row for availability_overrides table.
#[derive(Debug, Clone, FromRow)]
pub struct AvailabilityOverrideRow {
    pub id: Uuid,
    pub player_id: Option<Uuid>,
    pub registration_id: Option<Uuid>,
    pub override_date: NaiveDate,
    pub start_time: Option<NaiveTime>,
    pub end_time: Option<NaiveTime>,
    pub override_type: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<AvailabilityOverrideRow> for AvailabilityOverride {
    type Error = String;

    fn try_from(row: AvailabilityOverrideRow) -> Result<Self, Self::Error> {
        let override_type: OverrideType = row.override_type.parse()?;

        Ok(Self {
            id: AvailabilityExceptionId::from_uuid(row.id),
            player_id: row.player_id.map(PlayerId::from_uuid),
            registration_id: row.registration_id.map(TournamentRegistrationId::from_uuid),
            override_date: row.override_date,
            start_time: row.start_time,
            end_time: row.end_time,
            override_type,
            reason: row.reason,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// Database row for suggested_times table.
#[derive(Debug, Clone, FromRow)]
pub struct SuggestedTimeRow {
    pub id: Uuid,
    pub match_id: Uuid,
    pub suggested_start: DateTime<Utc>,
    pub suggested_end: DateTime<Utc>,
    pub confidence_score: i32,
    pub is_mutual_overlap: bool,
    pub is_auto_generated: bool,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<SuggestedTimeRow> for SuggestedTime {
    type Error = String;

    fn try_from(row: SuggestedTimeRow) -> Result<Self, Self::Error> {
        let status: SuggestionStatus = row.status.parse()?;

        Ok(Self {
            id: SuggestedTimeId::from_uuid(row.id),
            match_id: TournamentMatchId::from_uuid(row.match_id),
            suggested_start: row.suggested_start,
            suggested_end: row.suggested_end,
            confidence_score: row.confidence_score,
            is_mutual_overlap: row.is_mutual_overlap,
            is_auto_generated: row.is_auto_generated,
            status,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}
