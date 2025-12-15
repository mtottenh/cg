//! Demo catalog types.
//!
//! Types for categorizing and managing demo files independently of matches.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Category for demo files in the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DemoCategory {
    /// Newly discovered, not yet categorized.
    #[default]
    Uncategorized,
    /// Casual pick-up game / scrim.
    Pug,
    /// Official league tournament match.
    League,
    /// Team practice scrim (organized but not competitive).
    Scrim,
    /// Hidden / ignored demo (not relevant).
    Ignored,
}

impl fmt::Display for DemoCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uncategorized => write!(f, "uncategorized"),
            Self::Pug => write!(f, "pug"),
            Self::League => write!(f, "league"),
            Self::Scrim => write!(f, "scrim"),
            Self::Ignored => write!(f, "ignored"),
        }
    }
}

impl FromStr for DemoCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "uncategorized" => Ok(Self::Uncategorized),
            "pug" => Ok(Self::Pug),
            "league" => Ok(Self::League),
            "scrim" => Ok(Self::Scrim),
            "ignored" => Ok(Self::Ignored),
            _ => Err(format!("invalid demo category: {s}")),
        }
    }
}

impl DemoCategory {
    /// Check if this category represents a competitive match.
    #[must_use]
    pub const fn is_competitive(&self) -> bool {
        matches!(self, Self::League)
    }

    /// Check if this demo should be visible in public browsing.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        !matches!(self, Self::Ignored)
    }
}

/// Processing status for demo files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DemoStatus {
    /// Discovered but stats not yet fetched.
    #[default]
    Pending,
    /// Currently fetching/parsing stats.
    Processing,
    /// Stats available, ready for use.
    Ready,
    /// Stats fetching failed.
    Failed,
    /// Older demo, archived (stats may be unavailable).
    Archived,
}

impl fmt::Display for DemoStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Ready => write!(f, "ready"),
            Self::Failed => write!(f, "failed"),
            Self::Archived => write!(f, "archived"),
        }
    }
}

impl FromStr for DemoStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("invalid demo status: {s}")),
        }
    }
}

impl DemoStatus {
    /// Check if stats are available for this demo.
    #[must_use]
    pub const fn has_stats(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Check if this demo needs processing.
    #[must_use]
    pub const fn needs_processing(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Check if this demo is in a terminal state (no more processing).
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Ready | Self::Failed | Self::Archived)
    }
}

/// Type of link between a demo and a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DemoLinkType {
    /// Manually linked by admin/user.
    #[default]
    Manual,
    /// Auto-matched by system based on players/teams/time.
    AutoMatched,
    /// Linked as match evidence for disputes.
    Evidence,
}

impl fmt::Display for DemoLinkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manual => write!(f, "manual"),
            Self::AutoMatched => write!(f, "auto_matched"),
            Self::Evidence => write!(f, "evidence"),
        }
    }
}

impl FromStr for DemoLinkType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "manual" => Ok(Self::Manual),
            "auto_matched" => Ok(Self::AutoMatched),
            "evidence" => Ok(Self::Evidence),
            _ => Err(format!("invalid demo link type: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demo_category_roundtrip() {
        for category in [
            DemoCategory::Uncategorized,
            DemoCategory::Pug,
            DemoCategory::League,
            DemoCategory::Scrim,
            DemoCategory::Ignored,
        ] {
            let s = category.to_string();
            let parsed: DemoCategory = s.parse().unwrap();
            assert_eq!(category, parsed);
        }
    }

    #[test]
    fn test_demo_status_roundtrip() {
        for status in [
            DemoStatus::Pending,
            DemoStatus::Processing,
            DemoStatus::Ready,
            DemoStatus::Failed,
            DemoStatus::Archived,
        ] {
            let s = status.to_string();
            let parsed: DemoStatus = s.parse().unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_demo_category_visibility() {
        assert!(DemoCategory::Uncategorized.is_visible());
        assert!(DemoCategory::Pug.is_visible());
        assert!(DemoCategory::League.is_visible());
        assert!(!DemoCategory::Ignored.is_visible());
    }

    #[test]
    fn test_demo_status_needs_processing() {
        assert!(DemoStatus::Pending.needs_processing());
        assert!(!DemoStatus::Processing.needs_processing());
        assert!(!DemoStatus::Ready.needs_processing());
    }

    #[test]
    fn test_demo_link_type_roundtrip() {
        for link_type in [
            DemoLinkType::Manual,
            DemoLinkType::AutoMatched,
            DemoLinkType::Evidence,
        ] {
            let s = link_type.to_string();
            let parsed: DemoLinkType = s.parse().unwrap();
            assert_eq!(link_type, parsed);
        }
    }
}
