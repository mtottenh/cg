//! Demo stat-fact (EAV) repository trait.
//!
//! `demo_player_stats` holds per-`(demo, steam_id, stat_key)` fact rows
//! extracted from a demo's raw stats JSON at ingest by the game plugin
//! (extraction happens at the API layer — the domain must not depend on
//! `portal-plugins`). Facts are the aggregation surface for leaderboards
//! and awards; `demos.stats_json` remains the immutable source of truth.
//!
//! Design: `docs/design-tournament-awards.md` §3.2.

use async_trait::async_trait;
use portal_core::{DemoId, DomainError, LeagueSeasonId, PlayerId, TournamentId};

use crate::entities::award::{MinQualifier, StatAggregation, StatDirection};

/// Version of the fact-extraction logic. Bump when extraction changes in a
/// way that requires re-extracting historical demos; rows carry the version
/// so a backfill can target stale ones.
pub const CURRENT_EXTRACTOR_VERSION: i32 = 1;

/// One extracted stat fact for a single demo.
#[derive(Debug, Clone, PartialEq)]
pub struct DemoStatFact {
    /// Steam ID (64-bit, as a string) the fact belongs to.
    pub steam_id: String,
    /// Resolved portal player, when known at extraction time. Facts without
    /// a resolved player never rank in leaderboards.
    pub player_id: Option<PlayerId>,
    /// Catalog stat key (`headshot_kills`, `kills.weapon.mag7`, ...).
    pub stat_key: String,
    /// Numeric value for this demo.
    pub value: f64,
}

/// Aggregation boundary for a leaderboard query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderboardScope {
    /// Demos linked to matches of a single tournament.
    Tournament(TournamentId),
    /// Demos linked to matches of every tournament in a league season.
    Season(LeagueSeasonId),
}

/// A leaderboard request: scope + metric tuple + qualifier + limit.
#[derive(Debug, Clone)]
pub struct LeaderboardQuery {
    /// Aggregation boundary.
    pub scope: LeaderboardScope,
    /// Stat key to rank on.
    pub stat_key: String,
    /// How per-demo values fold into a ranked value.
    pub aggregation: StatAggregation,
    /// Ranking direction.
    pub direction: StatDirection,
    /// Optional minimum-participation qualifier.
    pub min_qualifier: Option<MinQualifier>,
    /// Maximum rows returned.
    pub limit: i64,
}

/// One ranked leaderboard row. Only facts with a resolved `player_id` rank.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardEntry {
    pub player_id: PlayerId,
    /// Player display name (joined from `players`).
    pub display_name: String,
    /// Player avatar URL (joined from `players`).
    pub avatar_url: Option<String>,
    /// Aggregated value.
    pub value: f64,
    /// Distinct demos that contributed to `value`.
    pub demos_counted: i64,
}

/// Repository for extracted demo stat facts and their aggregations.
#[async_trait]
pub trait DemoPlayerStatsRepository: Send + Sync + 'static {
    /// Idempotently replace every fact row for a demo (delete-and-reinsert),
    /// then resolve `player_id` against `players.steam_id_64` in the same
    /// transaction. Returns the number of fact rows inserted.
    async fn replace_for_demo(
        &self,
        demo_id: DemoId,
        extractor_version: i32,
        facts: Vec<DemoStatFact>,
    ) -> Result<u64, DomainError>;

    /// Rank resolved players within a scope by an aggregated stat.
    ///
    /// A demo may link to multiple matches; implementations must aggregate
    /// over *distinct* demos in scope so no fact is double-counted.
    async fn leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<Vec<LeaderboardEntry>, DomainError>;
}
