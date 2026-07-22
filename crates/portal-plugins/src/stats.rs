//! Stat catalog and fact extraction types for tournament awards.
//!
//! Game plugins declare a catalog of stat definitions (used by UI pickers and
//! award templates, which reference stats by `stat_key`) and extract EAV-shaped
//! facts (`steam_id`, `stat_key`, `value`) from a demo's stats JSON. The
//! aggregation layer sums/averages facts across demos; the plugin's only job is
//! to describe and emit them.

use serde::{Deserialize, Serialize};

/// How a stat's per-demo value should be interpreted when aggregating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatValueType {
    /// An additive count (kills, plants, …) — summing across demos is meaningful.
    Count,
    /// A per-demo ratio (ADR, HS%) — average or recompute across demos; never sum.
    Ratio,
}

/// A single stat a game plugin knows how to extract, described for catalogs
/// and UI pickers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatDefinition {
    /// Stable key referenced by award templates (e.g. `headshot_kills`,
    /// `kills.weapon.mag7`).
    pub key: String,
    /// Human-readable label (e.g. "Headshot Kills").
    pub label: String,
    /// Grouping for UI pickers: `Combat`, `Utility`, `Objective` or `Weapons`.
    pub category: String,
    /// How the value aggregates across demos.
    pub value_type: StatValueType,
    /// Longer description for tooltips / admin UI.
    pub description: String,
}

/// One extracted fact: a player (by Steam ID) has `value` for `stat_key` in a
/// single demo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatFact {
    /// Steam ID (64-bit, as a string) of the player the fact belongs to.
    pub steam_id: String,
    /// Stat key, matching the catalog for known stats. Extraction may also emit
    /// open-set keys (e.g. `kills.weapon.{name}`) not listed in the catalog.
    pub stat_key: String,
    /// Numeric value for this demo.
    pub value: f64,
}
