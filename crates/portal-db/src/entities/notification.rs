//! Notification rows.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// A row of `notifications`.
#[derive(Debug, Clone, FromRow)]
pub struct NotificationRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub tournament_id: Option<Uuid>,
    pub match_id: Option<Uuid>,
    pub dedupe_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

/// A row of `tournament_notification_settings`.
#[derive(Debug, Clone, FromRow)]
pub struct TournamentNotificationSettingsRow {
    pub tournament_id: Uuid,
    pub discord_webhook_url: Option<String>,
    pub discord_enabled: bool,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
}

/// The match and tournament fields a notification is written from.
#[derive(Debug, Clone, FromRow)]
pub struct MatchNotificationContextRow {
    pub match_id: Uuid,
    pub tournament_id: Uuid,
    pub tournament_name: String,
    pub tournament_slug: String,
    pub round: i32,
    pub participant1_registration_id: Option<Uuid>,
    pub participant1_name: Option<String>,
    pub participant2_registration_id: Option<Uuid>,
    pub participant2_name: Option<String>,
    pub participant1_score: Option<i32>,
    pub participant2_score: Option<i32>,
    pub winner_registration_id: Option<Uuid>,
    pub status: String,
}

/// One side of an open check-in window that has not checked in.
#[derive(Debug, Clone, FromRow)]
pub struct CheckInReminderRow {
    pub match_id: Uuid,
    pub registration_id: Uuid,
    pub deadline: DateTime<Utc>,
}
