//! Steam tracking repository trait.

use crate::entities::steam_tracking::{SteamTracking, UpdatePollResultCommand};
use crate::repositories::discovered_match::BackoffPolicy;
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, SteamTrackingId};

/// Repository trait for steam tracking operations.
#[async_trait]
pub trait SteamTrackingRepository: Send + Sync {
    /// Find a tracking entry by ID.
    async fn find_by_id(&self, id: SteamTrackingId) -> Result<Option<SteamTracking>, DomainError>;

    /// Find a tracking entry for a player and game.
    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<SteamTracking>, DomainError>;

    /// Create a new tracking entry.
    async fn create(&self, cmd: CreateSteamTracking) -> Result<SteamTracking, DomainError>;

    /// Update the game auth code, resuming the entry.
    ///
    /// Supplying a new code IS the fix for an `auth_expired` pause, so this
    /// clears the pause, the error count and any backoff. It previously did
    /// none of that — meaning the one user action that could recover a parked
    /// entry left it exactly as parked.
    async fn update_auth_code(
        &self,
        id: SteamTrackingId,
        auth_code: &str,
    ) -> Result<SteamTracking, DomainError>;

    /// Clear a pause or backoff and make the entry due immediately.
    ///
    /// The operator escape hatch. Before this, the only route back from the
    /// error cliff was a hand-written UPDATE against production.
    ///
    /// `reset_cursor` additionally clears `last_known_code`, which is what a
    /// `cursor_invalid` pause needs: the stored cursor is the thing Steam is
    /// rejecting, so resuming without dropping it just reproduces the 412.
    async fn resume(
        &self,
        id: SteamTrackingId,
        reset_cursor: bool,
    ) -> Result<SteamTracking, DomainError>;

    /// Deactivate tracking.
    async fn deactivate(&self, id: SteamTrackingId) -> Result<(), DomainError>;

    /// Delete tracking entry.
    async fn delete(&self, id: SteamTrackingId) -> Result<(), DomainError>;

    /// Get all active tracking entries for a game, regardless of poll state.
    async fn find_active_by_game(&self, game_id: GameId)
    -> Result<Vec<SteamTracking>, DomainError>;

    /// The poller's work list: active, not paused, and due for a poll.
    ///
    /// All three conditions belong here rather than in the bot. The poller
    /// used to fetch every active entry and apply `poll_errors >= 10` itself,
    /// which put the retry policy in the one place that could not durably
    /// record it and left the portal unable to say why an entry was idle.
    async fn find_due_for_poll(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<SteamTracking>, DomainError>;

    /// Record a poll result and schedule the next attempt.
    ///
    /// `backoff` applies only to [`PollOutcome::Transient`]; see
    /// [`UpdatePollResultCommand`] for why the cursor advances independently
    /// of the outcome.
    ///
    /// [`PollOutcome::Transient`]: crate::entities::steam_tracking::PollOutcome::Transient
    async fn update_poll_result(
        &self,
        id: SteamTrackingId,
        cmd: UpdatePollResultCommand,
        backoff: BackoffPolicy,
        rate_limit_cooldown_secs: i64,
    ) -> Result<SteamTracking, DomainError>;

    /// List tracking entries with their player's identity, worst health
    /// first, for the admin pipeline view (P-73).
    ///
    /// Unlike [`SteamTrackingRepository::find_active_by_game`] this includes
    /// deactivated entries: an operator asking "why did ingestion stop" needs
    /// to see a token that was switched off, not only the ones the poller is
    /// still working through.
    async fn list_health(
        &self,
        game_id: Option<GameId>,
        limit: i64,
    ) -> Result<Vec<TrackingHealthEntry>, DomainError>;

    /// Aggregate tracking health counts, optionally scoped to one game.
    ///
    /// `stale_after_hours` classifies an entry as stale when its last poll is
    /// older than that. Never-polled entries are counted separately — a token
    /// the poller has never touched is a different failure from one it has
    /// stopped touching.
    async fn tracking_health_summary(
        &self,
        game_id: Option<GameId>,
        stale_after_hours: i64,
    ) -> Result<TrackingHealthSummary, DomainError>;
}

/// A tracking entry joined with the identity of the player it belongs to.
///
/// `game_auth_code` is a live Steam credential and is deliberately **not**
/// part of this projection — the admin surface needs to know that a token is
/// failing, never what the token is.
#[derive(Debug, Clone)]
pub struct TrackingHealthEntry {
    pub id: SteamTrackingId,
    pub player_id: PlayerId,
    pub player_display_name: String,
    pub game_id: GameId,
    pub game_slug: String,
    pub steam_id_64: i64,
    pub is_active: bool,
    pub poll_errors: i32,
    pub last_poll_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
    /// `ok` · `backoff` · `auth_expired` · `cursor_invalid`. The two paused
    /// states each name a specific human action, which is the difference
    /// between "ingestion is degraded" and "go ask this player for a new code".
    pub poll_state: String,
    pub next_poll_at: chrono::DateTime<chrono::Utc>,
    pub paused_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether a share-code cursor has been recorded yet.
    pub has_share_code: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Aggregate counts over the tracking table.
#[derive(Debug, Clone, Default)]
pub struct TrackingHealthSummary {
    pub total: i64,
    pub active: i64,
    pub inactive: i64,
    pub with_errors: i64,
    pub never_polled: i64,
    pub stale: i64,
    /// Active entries the poller has stopped working pending a human — a
    /// revoked auth code or an unusable cursor. Counted separately from
    /// `with_errors`: an entry that is backing off will recover on its own,
    /// whereas these will not recover until someone acts.
    pub paused: i64,
    /// Of `paused`, those needing the player to supply a new auth code.
    pub paused_auth_expired: i64,
    /// Of `paused`, those needing the share-code cursor reset.
    pub paused_cursor_invalid: i64,
    pub last_poll_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Data for creating a new steam tracking entry.
#[derive(Debug, Clone)]
pub struct CreateSteamTracking {
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    /// Initial share code to use as the polling cursor.
    pub initial_share_code: Option<String>,
}
