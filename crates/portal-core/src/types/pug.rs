//! Pick-Up Game (PUG) types.
//!
//! A PUG is a one-off match outside any league or tournament. The `pugs`
//! aggregate owns the social/gathering phase; at lock-in it materializes as a
//! hidden single-match container tournament (kind = pug) so the veto, game
//! server, results and demo pipelines run unmodified.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Lifecycle status of a PUG lobby.
///
/// Mirrors the DB CHECK constraint in `migrations/0091_pugs.sql`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PugStatus {
    /// Players joining via the share link; teams forming.
    #[default]
    Gathering,
    /// Locked and materialized; veto or wheel in progress.
    MapSelection,
    /// Maps chosen; queued for / loading onto a game server.
    AwaitingServer,
    /// Match in progress on a server.
    Live,
    /// Series finished; result denormalized onto the pug row.
    Completed,
    /// Cancelled by the creator.
    Cancelled,
    /// Gathering lobby passed its TTL without locking.
    Expired,
}

impl PugStatus {
    /// Terminal statuses — the pug will never change again.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Expired)
    }

    /// Whether players may still join/leave/change teams.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self, Self::Gathering)
    }
}

impl fmt::Display for PugStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gathering => write!(f, "gathering"),
            Self::MapSelection => write!(f, "map_selection"),
            Self::AwaitingServer => write!(f, "awaiting_server"),
            Self::Live => write!(f, "live"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

impl FromStr for PugStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gathering" => Ok(Self::Gathering),
            "map_selection" => Ok(Self::MapSelection),
            "awaiting_server" => Ok(Self::AwaitingServer),
            "live" => Ok(Self::Live),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "expired" => Ok(Self::Expired),
            _ => Err(format!("invalid pug status: {s}")),
        }
    }
}

/// How a PUG chooses its maps.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PugMapSelectionMode {
    /// Standard alternating pick/ban veto over a fixed pool.
    #[default]
    Veto,
    /// Players nominate maps; a weighted spinning wheel picks each map.
    Wheel,
}

impl fmt::Display for PugMapSelectionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Veto => write!(f, "veto"),
            Self::Wheel => write!(f, "wheel"),
        }
    }
}

impl FromStr for PugMapSelectionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "veto" => Ok(Self::Veto),
            "wheel" => Ok(Self::Wheel),
            _ => Err(format!("invalid pug map selection mode: {s}")),
        }
    }
}
