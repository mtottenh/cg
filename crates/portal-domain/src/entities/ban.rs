//! Ban domain entity.

use chrono::{DateTime, Utc};
use portal_core::{BanId, UserId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of ban that determines what the user is restricted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BanType {
    /// Complete platform ban - user cannot access any features.
    Platform,
    /// Matchmaking ban - user cannot queue for matches.
    Matchmaking,
    /// Chat ban - user cannot send messages.
    Chat,
    /// League-specific ban - user is banned from a specific league.
    League,
    /// Tournament-specific ban - user is banned from a specific tournament.
    Tournament,
}

impl std::fmt::Display for BanType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform => write!(f, "platform"),
            Self::Matchmaking => write!(f, "matchmaking"),
            Self::Chat => write!(f, "chat"),
            Self::League => write!(f, "league"),
            Self::Tournament => write!(f, "tournament"),
        }
    }
}

impl std::str::FromStr for BanType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "platform" => Ok(Self::Platform),
            "matchmaking" => Ok(Self::Matchmaking),
            "chat" => Ok(Self::Chat),
            "league" => Ok(Self::League),
            "tournament" => Ok(Self::Tournament),
            _ => Err(format!("invalid ban type: {s}")),
        }
    }
}

/// Ban domain entity.
///
/// Represents a restriction placed on a user that prevents them from
/// accessing certain features of the platform.
#[derive(Debug, Clone)]
pub struct Ban {
    /// Unique identifier for this ban.
    pub id: BanId,
    /// The user who is banned.
    pub user_id: UserId,
    /// Type of ban (determines what is restricted).
    pub ban_type: BanType,
    /// Human-readable reason for the ban.
    pub reason: String,
    /// Scope type for context-specific bans (e.g., "league", "tournament").
    pub scope_type: Option<String>,
    /// Scope ID for context-specific bans (e.g., `league_id`, `tournament_id`).
    pub scope_id: Option<Uuid>,
    /// Who issued the ban (None for system-issued bans).
    pub issued_by: Option<UserId>,
    /// When the ban takes effect.
    pub starts_at: DateTime<Utc>,
    /// When the ban expires (None for permanent bans).
    pub ends_at: Option<DateTime<Utc>>,
    /// When the ban was lifted early (None if not lifted).
    pub lifted_at: Option<DateTime<Utc>>,
    /// Who lifted the ban (None if not lifted).
    pub lifted_by: Option<UserId>,
    /// Reason for lifting the ban (None if not lifted).
    pub lift_reason: Option<String>,
    /// When the ban record was created.
    pub created_at: DateTime<Utc>,
    /// When the ban record was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Ban {
    /// Check if the ban is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        let now = Utc::now();

        // Ban must have started
        if self.starts_at > now {
            return false;
        }

        // Ban must not be lifted
        if self.lifted_at.is_some() {
            return false;
        }

        // Ban must not be expired (if it has an end date)
        if let Some(ends_at) = self.ends_at
            && ends_at <= now
        {
            return false;
        }

        true
    }

    /// Check if this is a permanent ban.
    #[must_use]
    pub const fn is_permanent(&self) -> bool {
        self.ends_at.is_none()
    }

    /// Check if the ban has been lifted.
    #[must_use]
    pub const fn is_lifted(&self) -> bool {
        self.lifted_at.is_some()
    }

    /// Check if the ban has expired naturally.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        if let Some(ends_at) = self.ends_at {
            ends_at <= Utc::now()
        } else {
            false
        }
    }

    /// Get the remaining duration of the ban (None if permanent or inactive).
    #[must_use]
    pub fn remaining_duration(&self) -> Option<chrono::Duration> {
        if !self.is_active() {
            return None;
        }

        self.ends_at.map(|ends_at| ends_at - Utc::now())
    }
}

/// Command for creating a new ban.
#[derive(Debug, Clone)]
pub struct CreateBanCommand {
    /// The user to ban.
    pub user_id: UserId,
    /// Type of ban.
    pub ban_type: BanType,
    /// Reason for the ban.
    pub reason: String,
    /// Scope type for context-specific bans.
    pub scope_type: Option<String>,
    /// Scope ID for context-specific bans.
    pub scope_id: Option<Uuid>,
    /// Who is issuing the ban.
    pub issued_by: Option<UserId>,
    /// When the ban should start (defaults to now).
    pub starts_at: Option<DateTime<Utc>>,
    /// Duration of the ban in seconds (None for permanent).
    pub duration_seconds: Option<i64>,
}

/// Command for lifting a ban.
#[derive(Debug, Clone)]
pub struct LiftBanCommand {
    /// The ban to lift.
    pub ban_id: BanId,
    /// Who is lifting the ban.
    pub lifted_by: UserId,
    /// Reason for lifting the ban.
    pub lift_reason: Option<String>,
}

/// Filters for listing bans.
#[derive(Debug, Clone, Default)]
pub struct BanFilters {
    /// Filter by user ID.
    pub user_id: Option<UserId>,
    /// Filter by ban type.
    pub ban_type: Option<BanType>,
    /// Filter by active status.
    pub active_only: bool,
    /// Filter by scope type.
    pub scope_type: Option<String>,
    /// Filter by scope ID.
    pub scope_id: Option<Uuid>,
}
