//! Steam tracking domain entity.

use chrono::{DateTime, Utc};
use portal_core::{GameId, PlayerId, SteamTrackingId};

/// A player's opt-in to Steam match tracking for a specific game.
#[derive(Debug, Clone)]
pub struct SteamTracking {
    pub id: SteamTrackingId,
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    pub last_known_code: Option<String>,
    pub is_active: bool,
    /// Consecutive transient failures — the backoff exponent, not a ceiling.
    /// Resets on any successful poll; rate limiting does not increment it.
    pub poll_errors: i32,
    pub last_poll_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Earliest time the poller may try this entry again.
    pub next_poll_at: DateTime<Utc>,
    /// `ok` · `backoff` · `auth_expired` · `cursor_invalid`.
    pub poll_state: String,
    pub paused_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SteamTracking {
    /// Whether the poller has stopped working this entry pending a human.
    ///
    /// Distinct from `!is_active`, which is the player switching tracking off.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        matches!(self.poll_state.as_str(), "auth_expired" | "cursor_invalid")
    }
}

/// How one poll of a tracking entry ended.
///
/// The poller classifies; the repository decides what that means for
/// scheduling. The distinction the old `poll_errors >= 10` cliff could not
/// make is the whole point: two of these are worth retrying forever, one is
/// worth retrying but must not be charged to the player, and two cannot
/// succeed until a human acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollOutcome {
    /// Polled cleanly. Clears the error count and any backoff.
    Ok,
    /// Network error, 5xx, timeout — anything that might work next time.
    /// Backs off exponentially and retries indefinitely.
    Transient,
    /// Steam returned 429. Backs the entry off WITHOUT counting against it:
    /// rate limiting is a property of our own request volume, not of this
    /// player's token, and charging it here is how rate limiting alone could
    /// permanently kill every entry in the table.
    RateLimited,
    /// Steam rejected the match-sharing auth code (403). Pauses immediately —
    /// only the player supplying a new code can fix it.
    AuthExpired,
    /// Steam rejected the stored cursor (412). Pauses immediately — needs the
    /// cursor reset, not another attempt with the same bad value.
    CursorInvalid,
}

impl PollOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Transient => "transient",
            Self::RateLimited => "rate-limited",
            Self::AuthExpired => "auth-expired",
            Self::CursorInvalid => "cursor-invalid",
        }
    }

    /// Parse the wire name used by the poller's `poll-result` call.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ok" => Some(Self::Ok),
            "transient" => Some(Self::Transient),
            "rate-limited" => Some(Self::RateLimited),
            "auth-expired" => Some(Self::AuthExpired),
            "cursor-invalid" => Some(Self::CursorInvalid),
            _ => None,
        }
    }

    /// The `poll_state` this outcome puts the entry into.
    #[must_use]
    pub const fn resulting_state(self) -> &'static str {
        match self {
            // Rate limiting delays the entry but says nothing about its health,
            // so it leaves the state clean rather than reporting a fault.
            Self::Ok | Self::RateLimited => "ok",
            Self::Transient => "backoff",
            Self::AuthExpired => "auth_expired",
            Self::CursorInvalid => "cursor_invalid",
        }
    }

    /// Whether this outcome stops the poller until a human intervenes.
    #[must_use]
    pub const fn is_paused(self) -> bool {
        matches!(self, Self::AuthExpired | Self::CursorInvalid)
    }
}

/// Command to register for steam tracking.
#[derive(Debug, Clone)]
pub struct CreateSteamTrackingCommand {
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    /// Most recent share code — used as the starting cursor for the poller.
    pub initial_share_code: Option<String>,
}

/// Command to update a tracking entry's poll result.
#[derive(Debug, Clone)]
pub struct UpdatePollResultCommand {
    /// Newest share code discovered, advancing the cursor.
    ///
    /// Set independently of `outcome`: a walk that discovered three codes and
    /// then failed should still bank those three, or a player whose walk keeps
    /// breaking partway never makes any progress at all.
    pub last_known_code: Option<String>,
    pub outcome: PollOutcome,
    pub error: Option<String>,
}
