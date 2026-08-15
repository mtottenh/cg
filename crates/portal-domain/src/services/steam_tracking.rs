//! Steam tracking service with business logic.

use crate::entities::steam_tracking::{
    CreateSteamTrackingCommand, SteamTracking, UpdatePollResultCommand,
};
use crate::repositories::discovered_match::BackoffPolicy;
use crate::repositories::steam_tracking::{
    CreateSteamTracking, SteamTrackingRepository, TrackingHealthEntry, TrackingHealthSummary,
};
use crate::repositories::user::PlayerRepository;
use portal_core::{DomainError, GameId, PlayerId, SteamTrackingId};
use std::sync::Arc;
use tracing::{info, instrument, warn};

/// Backoff for transient poll failures: 1m, 2m, 4m … capped at 6 hours.
///
/// Deliberately has NO attempt ceiling to go with it. A tracking entry is a
/// standing subscription, not a unit of work: one request every six hours
/// costs nothing, so permanently abandoning an entry — which is what
/// `poll_errors >= 10` did — is strictly worse than checking on it
/// occasionally forever. Nothing is lost by polling less often either, since
/// the poller walks forward from a stored cursor and catches up.
///
/// Retrying only stops for failures that provably cannot succeed, and those
/// pause on the first occurrence rather than the tenth.
pub const POLL_BACKOFF: BackoffPolicy = BackoffPolicy {
    base_secs: 60.0,
    cap_secs: 21_600.0,
};

/// Flat cooldown after Steam returns 429.
///
/// Flat, not exponential: rate limiting reflects our own aggregate request
/// volume rather than anything about the entry that happened to hit it, so
/// escalating per entry would punish tokens at random for a global condition.
pub const RATE_LIMIT_COOLDOWN_SECS: i64 = 300;

/// Service for steam tracking business logic.
pub struct SteamTrackingService<STR, PR>
where
    STR: SteamTrackingRepository,
    PR: PlayerRepository,
{
    tracking_repo: Arc<STR>,
    player_repo: Arc<PR>,
}

impl<STR, PR> SteamTrackingService<STR, PR>
where
    STR: SteamTrackingRepository,
    PR: PlayerRepository,
{
    /// Create a new steam tracking service.
    pub const fn new(tracking_repo: Arc<STR>, player_repo: Arc<PR>) -> Self {
        Self {
            tracking_repo,
            player_repo,
        }
    }

    /// Register a player for steam tracking.
    #[instrument(skip(self, cmd))]
    pub async fn register(
        &self,
        cmd: CreateSteamTrackingCommand,
    ) -> Result<SteamTracking, DomainError> {
        // Verify the player exists and has a steam_id_64
        let player = self
            .player_repo
            .find_by_id(cmd.player_id)
            .await?
            .ok_or(DomainError::PlayerNotFound(cmd.player_id))?;

        // Validate: player must have steam_id linked (checked via steam_id field)
        if !player.has_steam_linked() {
            return Err(DomainError::RequirementsNotMet(
                "Player must have a linked Steam account".into(),
            ));
        }

        // Validate auth code format: AAAA-AAAAA-AAAA
        validate_auth_code(&cmd.game_auth_code)?;

        let tracking = self
            .tracking_repo
            .create(CreateSteamTracking {
                player_id: cmd.player_id,
                game_id: cmd.game_id,
                steam_id_64: cmd.steam_id_64,
                game_auth_code: cmd.game_auth_code,
                initial_share_code: cmd.initial_share_code,
            })
            .await?;

        info!(
            tracking_id = %tracking.id,
            player_id = %tracking.player_id,
            steam_id_64 = tracking.steam_id_64,
            "Steam tracking registered"
        );

        Ok(tracking)
    }

    /// Get a player's tracking entry for a specific game.
    #[instrument(skip(self))]
    pub async fn get_for_player(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<SteamTracking>, DomainError> {
        self.tracking_repo
            .find_by_player_and_game(player_id, game_id)
            .await
    }

    /// Update the game auth code for a tracking entry.
    #[instrument(skip(self, auth_code))]
    pub async fn update_auth_code(
        &self,
        id: SteamTrackingId,
        player_id: PlayerId,
        auth_code: &str,
    ) -> Result<SteamTracking, DomainError> {
        // Verify ownership
        let tracking = self
            .tracking_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal("Steam tracking entry not found".into()))?;

        if tracking.player_id != player_id {
            return Err(DomainError::Forbidden(
                "Cannot modify another player's tracking".into(),
            ));
        }

        validate_auth_code(auth_code)?;

        self.tracking_repo.update_auth_code(id, auth_code).await
    }

    /// Delete tracking for a player.
    #[instrument(skip(self))]
    pub async fn delete(
        &self,
        id: SteamTrackingId,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        let tracking = self
            .tracking_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal("Steam tracking entry not found".into()))?;

        if tracking.player_id != player_id {
            return Err(DomainError::Forbidden(
                "Cannot delete another player's tracking".into(),
            ));
        }

        self.tracking_repo.delete(id).await
    }

    /// Get all active tracking entries for a game (internal/bot use).
    #[instrument(skip(self))]
    pub async fn get_active_for_game(
        &self,
        game_id: GameId,
    ) -> Result<Vec<SteamTracking>, DomainError> {
        self.tracking_repo.find_active_by_game(game_id).await
    }

    /// The poller's work list: active, unpaused, and due (internal/bot use).
    #[instrument(skip(self))]
    pub async fn get_due_for_poll(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<SteamTracking>, DomainError> {
        self.tracking_repo.find_due_for_poll(game_id, limit).await
    }

    /// Update a tracking entry's poll result (internal/bot use).
    #[instrument(skip(self))]
    pub async fn update_poll_result(
        &self,
        id: SteamTrackingId,
        cmd: UpdatePollResultCommand,
    ) -> Result<SteamTracking, DomainError> {
        let outcome = cmd.outcome;
        let result = self
            .tracking_repo
            .update_poll_result(id, cmd, POLL_BACKOFF, RATE_LIMIT_COOLDOWN_SECS)
            .await?;

        if outcome.is_paused() {
            // Loud, and only once: the state change is what is newsworthy, and
            // a paused entry is not polled again so this cannot spam.
            warn!(
                tracking_id = %id,
                steam_id = result.steam_id_64,
                poll_state = %result.poll_state,
                error = result.last_error.as_deref().unwrap_or(""),
                "Tracking entry paused — needs a person: {}",
                match result.poll_state.as_str() {
                    "auth_expired" => "the player must supply a new match-sharing auth code",
                    "cursor_invalid" =>
                        "the share-code cursor is rejected; resume with a cursor reset",
                    _ => "unknown",
                }
            );
        }

        Ok(result)
    }

    /// Clear a pause or backoff and poll the entry again immediately.
    ///
    /// `reset_cursor` drops `last_known_code`, which is the point for a
    /// `cursor_invalid` pause — resuming while keeping the cursor Steam is
    /// rejecting just reproduces the same 412.
    #[instrument(skip(self))]
    pub async fn resume(
        &self,
        id: SteamTrackingId,
        reset_cursor: bool,
    ) -> Result<SteamTracking, DomainError> {
        let result = self.tracking_repo.resume(id, reset_cursor).await?;
        info!(
            tracking_id = %id,
            reset_cursor,
            "Tracking entry resumed"
        );
        Ok(result)
    }

    /// Tracking-token health for the admin pipeline view (P-73), worst first.
    #[instrument(skip(self))]
    pub async fn list_health(
        &self,
        game_id: Option<GameId>,
        limit: i64,
    ) -> Result<Vec<TrackingHealthEntry>, DomainError> {
        self.tracking_repo.list_health(game_id, limit).await
    }

    /// Aggregate tracking-token health counts.
    #[instrument(skip(self))]
    pub async fn health_summary(
        &self,
        game_id: Option<GameId>,
        stale_after_hours: i64,
    ) -> Result<TrackingHealthSummary, DomainError> {
        self.tracking_repo
            .tracking_health_summary(game_id, stale_after_hours)
            .await
    }
}

impl<STR, PR> Clone for SteamTrackingService<STR, PR>
where
    STR: SteamTrackingRepository,
    PR: PlayerRepository,
{
    fn clone(&self) -> Self {
        Self {
            tracking_repo: Arc::clone(&self.tracking_repo),
            player_repo: Arc::clone(&self.player_repo),
        }
    }
}

/// Validate game auth code format: `XXXX-XXXXX-XXXX` (alphanumeric groups).
fn validate_auth_code(code: &str) -> Result<(), DomainError> {
    let parts: Vec<&str> = code.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 5 || parts[2].len() != 4 {
        return Err(DomainError::Validation(
            portal_core::ValidationError::field(portal_core::FieldError::format(
                "game_auth_code",
                "in XXXX-XXXXX-XXXX format",
            )),
        ));
    }
    if !parts
        .iter()
        .all(|p| p.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return Err(DomainError::Validation(
            portal_core::ValidationError::field(portal_core::FieldError::format(
                "game_auth_code",
                "alphanumeric characters only",
            )),
        ));
    }
    Ok(())
}
