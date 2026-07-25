//! Game-server integration types.
//!
//! Statuses for registered game servers, match reservations, and mid-series
//! substitutions, plus the MatchZy `get5_status` gamestate vocabulary.
//! Design: docs/matchzy-integration.md §4, §6.7, §6.8.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Operational status of a registered game server.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GameServerStatus {
    /// No agent heartbeat (or never connected).
    #[default]
    Offline,
    /// Idle and eligible for allocation.
    Available,
    /// Claimed by a pending/configuring reservation.
    Reserved,
    /// A match config is being loaded.
    Configuring,
    /// Hosting a portal-managed match.
    InMatch,
    /// Heartbeat shows a match the portal did not set up (out-of-band pug).
    BusyExternal,
    /// Agent reports an unhealthy state (e.g. RCON unreachable).
    Error,
}

impl GameServerStatus {
    /// Whether the allocation predicate may consider this server at all.
    ///
    /// Freshness/gamestate checks are applied on top of this (§6.7).
    #[must_use]
    pub const fn is_allocatable(&self) -> bool {
        matches!(self, Self::Available)
    }

    /// Whether the server is currently tied to a reservation.
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::Reserved | Self::Configuring | Self::InMatch)
    }
}

impl fmt::Display for GameServerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => write!(f, "offline"),
            Self::Available => write!(f, "available"),
            Self::Reserved => write!(f, "reserved"),
            Self::Configuring => write!(f, "configuring"),
            Self::InMatch => write!(f, "in_match"),
            Self::BusyExternal => write!(f, "busy_external"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl FromStr for GameServerStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "offline" => Ok(Self::Offline),
            "available" => Ok(Self::Available),
            "reserved" => Ok(Self::Reserved),
            "configuring" => Ok(Self::Configuring),
            "in_match" => Ok(Self::InMatch),
            "busy_external" => Ok(Self::BusyExternal),
            "error" => Ok(Self::Error),
            _ => Err(format!("invalid game server status: {s}")),
        }
    }
}

/// Lifecycle of a match's server reservation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReservationStatus {
    /// Created; awaiting a free server / agent.
    #[default]
    Pending,
    /// `load_match` sent; awaiting the `series_start` webhook.
    Configuring,
    /// Config loaded — players may connect.
    Ready,
    /// `going_live` received.
    Live,
    /// `series_end` received.
    Completed,
    /// Setup failed (agent offline, load rejected, retries exhausted).
    Failed,
    /// Cancelled by an admin or superseded by reassignment.
    Cancelled,
}

impl ReservationStatus {
    /// States counted by the one-live-reservation-per-server invariant (§6.7).
    ///
    /// Must match the `uq_server_reservations_live_*` partial indexes in
    /// migration 0080.
    #[must_use]
    pub const fn is_live_state(&self) -> bool {
        matches!(
            self,
            Self::Pending | Self::Configuring | Self::Ready | Self::Live
        )
    }

    /// Whether the reservation can never change state again.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

impl fmt::Display for ReservationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Configuring => write!(f, "configuring"),
            Self::Ready => write!(f, "ready"),
            Self::Live => write!(f, "live"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for ReservationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "configuring" => Ok(Self::Configuring),
            "ready" => Ok(Self::Ready),
            "live" => Ok(Self::Live),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid reservation status: {s}")),
        }
    }
}

/// Lifecycle of a mid-series substitution request (§6.8).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SubstitutionStatus {
    /// Created; not yet routed to approval or application.
    #[default]
    Pending,
    /// Waiting on admin approval (tournament policy).
    AwaitingApproval,
    /// Queued/retrying against the server (e.g. blocked by halftime).
    Applying,
    /// Roster edit accepted by the server; lineups updated.
    Applied,
    /// The server rejected the edit permanently or retries were exhausted.
    Failed,
    /// Rejected by an admin.
    Rejected,
    /// Withdrawn by the requester before it applied.
    Cancelled,
}

impl fmt::Display for SubstitutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::AwaitingApproval => write!(f, "awaiting_approval"),
            Self::Applying => write!(f, "applying"),
            Self::Applied => write!(f, "applied"),
            Self::Failed => write!(f, "failed"),
            Self::Rejected => write!(f, "rejected"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for SubstitutionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "applying" => Ok(Self::Applying),
            "applied" => Ok(Self::Applied),
            "failed" => Ok(Self::Failed),
            "rejected" => Ok(Self::Rejected),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid substitution status: {s}")),
        }
    }
}

/// MatchZy `get5_status` gamestate, as reported in agent heartbeats.
///
/// MatchZy owns this vocabulary (verified against v0.8.15 `G5API.cs`);
/// unknown strings parse to [`AgentGamestate::Unknown`] rather than failing,
/// so a plugin update never breaks heartbeat ingestion.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentGamestate {
    /// No match loaded — the idle state the allocator requires.
    #[default]
    None,
    /// Match loaded, in-server veto not yet started (unused by the portal).
    PreVeto,
    /// In-server veto running (unused by the portal — we skip veto).
    Veto,
    /// Warmup / ready-up phase.
    Warmup,
    /// Knife round in progress.
    Knife,
    /// Knife winner deciding sides.
    WaitingForKnifeDecision,
    /// Countdown to live.
    GoingLive,
    /// Match live.
    Live,
    /// A round-backup restore is pending.
    PendingRestore,
    /// Map/series finished, before reset.
    PostGame,
    /// Unrecognized gamestate string (future MatchZy versions).
    Unknown,
}

impl AgentGamestate {
    /// Whether the server is idle from MatchZy's perspective.
    #[must_use]
    pub const fn is_idle(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Parse leniently: never fails, unknown values become [`Self::Unknown`].
    #[must_use]
    pub fn parse_lenient(s: &str) -> Self {
        match s {
            "none" => Self::None,
            "pre_veto" => Self::PreVeto,
            "veto" => Self::Veto,
            "warmup" => Self::Warmup,
            "knife" => Self::Knife,
            "waiting_for_knife_decision" => Self::WaitingForKnifeDecision,
            "going_live" => Self::GoingLive,
            "live" => Self::Live,
            "pending_restore" => Self::PendingRestore,
            "post_game" => Self::PostGame,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for AgentGamestate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::PreVeto => write!(f, "pre_veto"),
            Self::Veto => write!(f, "veto"),
            Self::Warmup => write!(f, "warmup"),
            Self::Knife => write!(f, "knife"),
            Self::WaitingForKnifeDecision => write!(f, "waiting_for_knife_decision"),
            Self::GoingLive => write!(f, "going_live"),
            Self::Live => write!(f, "live"),
            Self::PendingRestore => write!(f, "pending_restore"),
            Self::PostGame => write!(f, "post_game"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_live_states_match_migration_partial_index() {
        // Keep in lockstep with uq_server_reservations_live_* in 0080.
        let live: Vec<ReservationStatus> = [
            "pending",
            "configuring",
            "ready",
            "live",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
        for status in live {
            assert!(status.is_live_state());
            assert!(!status.is_terminal());
        }
        assert!(!ReservationStatus::Completed.is_live_state());
        assert!(!ReservationStatus::Failed.is_live_state());
        assert!(!ReservationStatus::Cancelled.is_live_state());
    }

    #[test]
    fn gamestate_parses_leniently() {
        assert_eq!(AgentGamestate::parse_lenient("none"), AgentGamestate::None);
        assert_eq!(
            AgentGamestate::parse_lenient("post_game"),
            AgentGamestate::PostGame
        );
        assert_eq!(
            AgentGamestate::parse_lenient("some_future_state"),
            AgentGamestate::Unknown
        );
    }

    #[test]
    fn statuses_round_trip_display_fromstr() {
        for s in [
            "offline",
            "available",
            "reserved",
            "configuring",
            "in_match",
            "busy_external",
            "error",
        ] {
            let parsed: GameServerStatus = s.parse().unwrap();
            assert_eq!(parsed.to_string(), s);
        }
    }
}
