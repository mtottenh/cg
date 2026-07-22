//! Award domain entities.
//!
//! An award is an organizer-named claim over a stat within a scope
//! (tournament or league season): "Swag 7" = most MAG-7 kills. Awards are
//! authored from per-game templates or built custom from the game plugin's
//! stat catalog, ranked live from `demo_player_stats` facts, and snapshotted
//! into immutable [`AwardResult`] podium rows on finalization.
//!
//! Design: `docs/design-tournament-awards.md`.

use chrono::{DateTime, Utc};
use portal_core::{AwardId, AwardResultId, AwardTemplateId, GameId, PlayerId, UserId};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Scope an award aggregates over (`awards.scope_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AwardScopeType {
    /// A single tournament.
    Tournament,
    /// A league season (spans every tournament in the season).
    LeagueSeason,
}

impl AwardScopeType {
    /// The database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tournament => "tournament",
            Self::LeagueSeason => "league_season",
        }
    }
}

impl fmt::Display for AwardScopeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AwardScopeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tournament" => Ok(Self::Tournament),
            "league_season" => Ok(Self::LeagueSeason),
            other => Err(format!("invalid award scope type: {other}")),
        }
    }
}

/// How per-demo stat facts fold into a single ranked value
/// (`awards.aggregation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StatAggregation {
    /// Sum of the stat across every counted demo.
    #[default]
    Sum,
    /// The best single-demo value.
    MaxSingleDemo,
    /// Average per counted demo (for per-demo ratios like ADR).
    AvgPerDemo,
}

impl StatAggregation {
    /// The database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::MaxSingleDemo => "max_single_demo",
            Self::AvgPerDemo => "avg_per_demo",
        }
    }
}

impl fmt::Display for StatAggregation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StatAggregation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sum" => Ok(Self::Sum),
            "max_single_demo" => Ok(Self::MaxSingleDemo),
            "avg_per_demo" => Ok(Self::AvgPerDemo),
            other => Err(format!("invalid stat aggregation: {other}")),
        }
    }
}

/// Ranking direction (`awards.direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StatDirection {
    /// Highest value wins (most kills).
    #[default]
    Desc,
    /// Lowest value wins (fewest deaths).
    Asc,
}

impl StatDirection {
    /// The database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Desc => "desc",
            Self::Asc => "asc",
        }
    }
}

impl fmt::Display for StatDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StatDirection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "desc" => Ok(Self::Desc),
            "asc" => Ok(Self::Asc),
            other => Err(format!("invalid stat direction: {other}")),
        }
    }
}

/// Kind of minimum-participation qualifier (`awards.min_qualifier_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MinQualifierType {
    /// Minimum number of distinct demos counted.
    Matches,
    /// Minimum total rounds played (sum of `rounds_played` facts in scope).
    Rounds,
}

impl MinQualifierType {
    /// The database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Matches => "matches",
            Self::Rounds => "rounds",
        }
    }
}

impl fmt::Display for MinQualifierType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MinQualifierType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "matches" => Ok(Self::Matches),
            "rounds" => Ok(Self::Rounds),
            other => Err(format!("invalid qualifier type: {other}")),
        }
    }
}

/// A minimum-participation qualifier: players below the threshold do not
/// rank (essential for ratio stats like ADR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinQualifier {
    /// What the threshold counts.
    pub qualifier_type: MinQualifierType,
    /// The threshold value (inclusive).
    pub value: i32,
}

/// Who an award ranks (`awards.subject_type`). V1 is player-only; `team`
/// is reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AwardSubjectType {
    /// Individual players.
    #[default]
    Player,
    /// Teams (reserved; not implemented in v1).
    Team,
}

impl AwardSubjectType {
    /// The database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Team => "team",
        }
    }
}

impl fmt::Display for AwardSubjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AwardSubjectType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "player" => Ok(Self::Player),
            "team" => Ok(Self::Team),
            other => Err(format!("invalid award subject type: {other}")),
        }
    }
}

/// Award lifecycle status (`awards.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AwardStatus {
    /// Live: standings computed on read, editable by the organizer.
    #[default]
    Active,
    /// Snapshotted: results written, presentation locked.
    Finalized,
    /// Cancelled by the organizer; excluded from standings and podiums.
    Void,
}

impl AwardStatus {
    /// The database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Finalized => "finalized",
            Self::Void => "void",
        }
    }
}

impl fmt::Display for AwardStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AwardStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "finalized" => Ok(Self::Finalized),
            "void" => Ok(Self::Void),
            other => Err(format!("invalid award status: {other}")),
        }
    }
}

/// A per-game award template — the organizer's default picker entry
/// ("Headshot Machine", "Swag 7", ...). Template = branding + metric tuple.
#[derive(Debug, Clone)]
pub struct AwardTemplate {
    pub id: AwardTemplateId,
    pub game_id: GameId,
    /// Stable slug used for seeding and API lookups (e.g. `swag7`).
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    /// mdi icon name.
    pub icon: Option<String>,
    /// `#rrggbb` accent color.
    pub color: Option<String>,
    pub stat_key: String,
    pub aggregation: StatAggregation,
    pub direction: StatDirection,
    pub min_qualifier: Option<MinQualifier>,
    pub created_at: DateTime<Utc>,
}

/// An award instance attached to a tournament or league season.
#[derive(Debug, Clone)]
pub struct Award {
    pub id: AwardId,
    pub scope_type: AwardScopeType,
    /// Tournament id or league-season id depending on `scope_type`.
    pub scope_id: Uuid,
    pub game_id: GameId,
    /// Source template, when created from one (branding may since diverge).
    pub template_id: Option<AwardTemplateId>,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub stat_key: String,
    pub aggregation: StatAggregation,
    pub direction: StatDirection,
    pub min_qualifier: Option<MinQualifier>,
    pub subject_type: AwardSubjectType,
    pub status: AwardStatus,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Award {
    /// Whether the award is still live (not finalized, not void).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == AwardStatus::Active
    }
}

/// An immutable podium row written at finalization. Rank is shared on ties
/// (two rank-1 rows = shared award).
#[derive(Debug, Clone)]
pub struct AwardResult {
    pub id: AwardResultId,
    pub award_id: AwardId,
    /// Competition rank (1-based; ties share a rank).
    pub rank: i32,
    pub player_id: PlayerId,
    pub value: f64,
    /// Distinct demos that contributed to `value`.
    pub demos_counted: i32,
    pub finalized_at: DateTime<Utc>,
}
