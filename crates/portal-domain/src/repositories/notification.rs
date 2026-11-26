//! Notification repository trait.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{
    NotificationId, TournamentId, TournamentMatchId, TournamentRegistrationId, UserId,
};

use crate::entities::notification::{
    Audience, CheckInReminderCandidate, MatchNotificationContext, NewNotification, Notification,
    TournamentNotificationSettings,
};

/// Storage for notifications and their delivery settings.
#[async_trait]
pub trait NotificationRepository: Send + Sync + 'static {
    /// The match, its tournament and both sides, or `None` if it is gone.
    async fn match_context(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<MatchNotificationContext>, DomainError>;

    /// The users a notification for these registrations goes to, without
    /// duplicates.
    async fn recipients(
        &self,
        registration_ids: &[TournamentRegistrationId],
        audience: Audience,
    ) -> Result<Vec<UserId>, DomainError>;

    /// Write one copy for each user. A user who already has a notification
    /// with the same `dedupe_key` is skipped. Returns the users written for.
    async fn insert_for_users(
        &self,
        user_ids: &[UserId],
        notification: &NewNotification,
    ) -> Result<Vec<UserId>, DomainError>;

    /// A user's notifications, newest first, older than `before` when given.
    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        before: Option<NotificationId>,
    ) -> Result<Vec<Notification>, DomainError>;

    /// How many unread notifications the user has, and the newest unread id.
    async fn unread_count(
        &self,
        user_id: UserId,
    ) -> Result<(i64, Option<NotificationId>), DomainError>;

    /// Mark the user's notifications read: those listed, or all when `ids`
    /// is `None`. Other users' notifications are never touched.
    async fn mark_read(
        &self,
        user_id: UserId,
        ids: Option<&[NotificationId]>,
        now: DateTime<Utc>,
    ) -> Result<u64, DomainError>;

    /// Delete read notifications older than `read_before`, and any older than
    /// `any_before`.
    async fn delete_expired(
        &self,
        read_before: DateTime<Utc>,
        any_before: DateTime<Utc>,
    ) -> Result<u64, DomainError>;

    /// Sides of open check-in windows closing by `until` that have not
    /// checked in.
    async fn check_in_reminder_candidates(
        &self,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<CheckInReminderCandidate>, DomainError>;

    /// A tournament's delivery settings, if any were saved.
    async fn get_settings(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Option<TournamentNotificationSettings>, DomainError>;

    /// Save a tournament's delivery settings. `webhook` is `None` to keep the
    /// stored URL, `Some(None)` to clear it.
    async fn upsert_settings(
        &self,
        tournament_id: TournamentId,
        discord_enabled: bool,
        webhook: Option<Option<String>>,
        updated_by: UserId,
    ) -> Result<TournamentNotificationSettings, DomainError>;
}
