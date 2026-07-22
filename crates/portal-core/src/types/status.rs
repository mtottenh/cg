//! Common status enums used across entities.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Generic entity status (active/inactive/deleted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EntityStatus {
    /// Entity is active and usable.
    #[default]
    Active,
    /// Entity is inactive but not deleted.
    Inactive,
    /// Entity is soft-deleted.
    Deleted,
}

impl fmt::Display for EntityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Deleted => write!(f, "deleted"),
        }
    }
}

impl FromStr for EntityStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "deleted" => Ok(Self::Deleted),
            _ => Err(format!("invalid entity status: {s}")),
        }
    }
}

/// Status of a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    /// Match is being set up.
    #[default]
    Pending,
    /// Match is ready to start.
    Ready,
    /// Match is currently in progress.
    InProgress,
    /// Match has been completed.
    Completed,
    /// Match was cancelled.
    Cancelled,
    /// Match was forfeited by one side.
    Forfeited,
    /// Match was disputed and under review.
    Disputed,
}

impl fmt::Display for MatchStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Ready => write!(f, "ready"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Forfeited => write!(f, "forfeited"),
            Self::Disputed => write!(f, "disputed"),
        }
    }
}

impl FromStr for MatchStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "forfeited" => Ok(Self::Forfeited),
            "disputed" => Ok(Self::Disputed),
            _ => Err(format!("invalid match status: {s}")),
        }
    }
}

impl MatchStatus {
    /// Check if the match is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Forfeited)
    }

    /// Check if the match can be cancelled.
    #[must_use]
    pub const fn can_cancel(&self) -> bool {
        matches!(self, Self::Pending | Self::Ready)
    }

    /// Check if the match can be started.
    #[must_use]
    pub const fn can_start(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Status of a tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TournamentStatus {
    /// Tournament is a draft (not published).
    #[default]
    Draft,
    /// Tournament is published but registration not yet open.
    Published,
    /// Registration is open.
    Registration,
    /// Registration is closed, tournament hasn't started.
    Scheduled,
    /// Tournament matches are being played.
    InProgress,
    /// All matches complete, pending final verification.
    Completed,
    /// Tournament has been finalized with results.
    Finalized,
    /// Tournament was cancelled.
    Cancelled,
}

impl fmt::Display for TournamentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Published => write!(f, "published"),
            Self::Registration => write!(f, "registration"),
            Self::Scheduled => write!(f, "scheduled"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Finalized => write!(f, "finalized"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for TournamentStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            "registration" => Ok(Self::Registration),
            "scheduled" => Ok(Self::Scheduled),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "finalized" => Ok(Self::Finalized),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid tournament status: {s}")),
        }
    }
}

impl TournamentStatus {
    /// Check if registration is open.
    #[must_use]
    pub const fn is_registration_open(&self) -> bool {
        matches!(self, Self::Registration)
    }

    /// Check if the tournament is active (not finished or cancelled).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Draft | Self::Published | Self::Registration | Self::Scheduled | Self::InProgress
        )
    }

    /// Check if the tournament is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Finalized | Self::Cancelled)
    }

    /// Valid transitions from this status.
    #[must_use]
    pub fn valid_transitions(&self) -> Vec<Self> {
        match self {
            Self::Draft => vec![Self::Published, Self::Cancelled],
            Self::Published => vec![Self::Registration, Self::Cancelled],
            Self::Registration => vec![Self::Scheduled, Self::InProgress, Self::Cancelled],
            Self::Scheduled => vec![Self::Registration, Self::InProgress, Self::Cancelled],
            Self::InProgress => vec![Self::Completed, Self::Cancelled],
            Self::Completed => vec![Self::Finalized],
            Self::Finalized => vec![],
            Self::Cancelled => vec![],
        }
    }

    /// Check if a transition to the given status is valid.
    #[must_use]
    pub fn can_transition_to(&self, target: Self) -> bool {
        self.valid_transitions().contains(&target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_status_roundtrip() {
        for status in [
            EntityStatus::Active,
            EntityStatus::Inactive,
            EntityStatus::Deleted,
        ] {
            let s = status.to_string();
            let parsed: EntityStatus = s.parse().unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_match_status_terminal() {
        assert!(!MatchStatus::Pending.is_terminal());
        assert!(!MatchStatus::InProgress.is_terminal());
        assert!(MatchStatus::Completed.is_terminal());
        assert!(MatchStatus::Cancelled.is_terminal());
        assert!(MatchStatus::Forfeited.is_terminal());
    }

    #[test]
    fn test_tournament_status_registration() {
        assert!(!TournamentStatus::Draft.is_registration_open());
        assert!(TournamentStatus::Registration.is_registration_open());
        assert!(!TournamentStatus::InProgress.is_registration_open());
    }
}
