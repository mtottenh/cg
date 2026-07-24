//! Match lineup type definitions.
//!
//! A lineup answers "who actually played this match?" — distinct from the roster,
//! which answers "who is eligible?". See `docs/lineup-design.md`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Lifecycle status of a lineup (§0 Q2).
///
/// A captain declares a provisional lineup (`draft`), submits it (`submitted`),
/// and it becomes read-only when the match starts (`locked`, stamped on the
/// `PickBan`/`InProgress` transition). A lineup is only opponent-visible once
/// `locked` (§0 Q3).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LineupStatus {
    /// Being edited; not yet submitted.
    #[default]
    Draft,
    /// Submitted by a captain; still editable until the match starts.
    Submitted,
    /// The match has started; the lineup is read-only and opponent-visible.
    Locked,
}

impl fmt::Display for LineupStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Submitted => write!(f, "submitted"),
            Self::Locked => write!(f, "locked"),
        }
    }
}

impl FromStr for LineupStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Self::Draft),
            "submitted" => Ok(Self::Submitted),
            "locked" => Ok(Self::Locked),
            _ => Err(format!("invalid lineup status: {s}")),
        }
    }
}

impl LineupStatus {
    /// Whether the lineup may still be edited (not yet locked).
    #[must_use]
    pub const fn is_editable(&self) -> bool {
        !matches!(self, Self::Locked)
    }
}

/// Provenance of a lineup player row (§0a).
///
/// This is the load-bearing distinction of the two-phase model:
/// `Declared` rows are a captain's provisional promise; the rest are
/// authoritative records of who actually played, best-to-worst automation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LineupSource {
    /// The provisional lineup a captain entered at pick/ban (a promise, not proof).
    #[default]
    Declared,
    /// The authoritative lineup parsed from the map demo (automatic).
    Demo,
    /// From a submitted screenshot/artefact, entered by an admin.
    Evidence,
    /// Filled in manually by an admin with no artefact (last resort).
    Admin,
}

impl LineupSource {
    /// Whether this source is authoritative (counts for stats/eligibility).
    #[must_use]
    pub const fn is_authoritative(&self) -> bool {
        !matches!(self, Self::Declared)
    }
}

impl fmt::Display for LineupSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declared => write!(f, "declared"),
            Self::Demo => write!(f, "demo"),
            Self::Evidence => write!(f, "evidence"),
            Self::Admin => write!(f, "admin"),
        }
    }
}

impl FromStr for LineupSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "declared" => Ok(Self::Declared),
            "demo" => Ok(Self::Demo),
            "evidence" => Ok(Self::Evidence),
            "admin" => Ok(Self::Admin),
            _ => Err(format!("invalid lineup source: {s}")),
        }
    }
}

/// How a player participated in the match (§0a).
///
/// `Substituted`/`LeftEarly` are reachable through the demo path but are not
/// written by any producer yet (deferred — see the task boundary).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ParticipationStatus {
    /// Played the full match.
    #[default]
    Confirmed,
    /// Declared but did not appear.
    NoShow,
    /// Left before the match ended.
    LeftEarly,
    /// Was substituted out during the match.
    Substituted,
    /// Removed from the lineup.
    Removed,
}

/// The §0.4 majority rule: substitutes must be a **strict minority** of the
/// effective lineup — `subs * 2 < lineup_size`.
///
/// The even-lineup tie-break is decided here (the doc named it as the one open
/// implementation choice, §0a): 2-of-4 is **illegal** — a substitute count of
/// exactly half the lineup does not satisfy the strict-minority rule. This is
/// the recommended default the schema comment encodes.
///
/// Returns `true` when the lineup is legal under the majority rule.
#[must_use]
pub const fn substitutes_are_minority(substitute_count: usize, lineup_size: usize) -> bool {
    // Strict minority: subs * 2 < lineup_size. An empty lineup is vacuously ok.
    substitute_count * 2 < lineup_size || lineup_size == 0
}

impl fmt::Display for ParticipationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Confirmed => write!(f, "confirmed"),
            Self::NoShow => write!(f, "no_show"),
            Self::LeftEarly => write!(f, "left_early"),
            Self::Substituted => write!(f, "substituted"),
            Self::Removed => write!(f, "removed"),
        }
    }
}

impl FromStr for ParticipationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "confirmed" => Ok(Self::Confirmed),
            "no_show" => Ok(Self::NoShow),
            "left_early" => Ok(Self::LeftEarly),
            "substituted" => Ok(Self::Substituted),
            "removed" => Ok(Self::Removed),
            _ => Err(format!("invalid participation status: {s}")),
        }
    }
}

#[cfg(test)]
mod majority_tests {
    use super::substitutes_are_minority;

    #[test]
    fn strict_minority_rule_with_even_lineup_tiebreak() {
        // 0 of 5 subs -> legal
        assert!(substitutes_are_minority(0, 5));
        // 2 of 5 -> legal (minority)
        assert!(substitutes_are_minority(2, 5));
        // 2 of 4 -> ILLEGAL (exactly half is not a strict minority — the tie-break)
        assert!(!substitutes_are_minority(2, 4));
        // 1 of 4 -> legal
        assert!(substitutes_are_minority(1, 4));
        // 3 of 5 -> illegal (majority)
        assert!(!substitutes_are_minority(3, 5));
        // empty lineup -> vacuously legal
        assert!(substitutes_are_minority(0, 0));
    }
}
