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

/// Sort column for a combined player-stats leaderboard. Selecting the
/// `ORDER BY` column from this fixed set keeps user input out of the SQL
/// text — every other query value is a bound parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerStatsSort {
    /// Rank by summed kills.
    #[default]
    Kills,
    /// Rank by summed deaths.
    Deaths,
    /// Rank by summed assists.
    Assists,
    /// Rank by summed damage dealt.
    TotalDamage,
    /// Rank by rounds-weighted ADR.
    Adr,
}

impl PlayerStatsSort {
    /// The whitelisted SQL column name this sort maps to. The returned
    /// string is a fixed literal, never user input.
    #[must_use]
    pub const fn column(self) -> &'static str {
        match self {
            Self::Kills => "kills",
            Self::Deaths => "deaths",
            Self::Assists => "assists",
            Self::TotalDamage => "total_damage",
            Self::Adr => "adr",
        }
    }
}

/// A combined player-stats leaderboard request: one row per player with
/// separate kill/death/assist/damage columns and a rounds-weighted ADR.
#[derive(Debug, Clone)]
pub struct PlayerStatsQuery {
    /// Aggregation boundary.
    pub scope: LeaderboardScope,
    /// Column the rows are ordered by (descending).
    pub sort: PlayerStatsSort,
    /// Only rank players with at least this many counted demos.
    pub min_demos: i64,
    /// Only rank players with at least this many rounds played in scope.
    pub min_rounds: f64,
    /// Maximum rows returned.
    pub limit: i64,
}

/// One combined player-stats row: summed core stats plus a rounds-weighted
/// ADR (`SUM(damage) / SUM(rounds)`, not an average of per-demo ADRs). Only
/// facts with a resolved `player_id` rank.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerStatsEntry {
    pub player_id: PlayerId,
    /// Player display name (joined from `players`).
    pub display_name: String,
    /// Player avatar URL (joined from `players`).
    pub avatar_url: Option<String>,
    /// Summed kills across counted demos.
    pub kills: f64,
    /// Summed deaths across counted demos.
    pub deaths: f64,
    /// Summed assists across counted demos.
    pub assists: f64,
    /// Summed damage dealt across counted demos.
    pub total_damage: f64,
    /// Rounds-weighted average damage per round.
    pub adr: f64,
    /// Summed rounds played across counted demos.
    pub rounds_played: f64,
    /// Distinct demos that contributed to the row.
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

    /// Combined per-player stat leaderboard: one row per resolved player with
    /// separate summed kills/deaths/assists/damage columns and a
    /// rounds-weighted ADR, ordered by the query's sort column.
    ///
    /// As with [`Self::leaderboard`], implementations must aggregate over
    /// *distinct* demos in scope so no fact is double-counted.
    async fn player_stats_leaderboard(
        &self,
        query: &PlayerStatsQuery,
    ) -> Result<Vec<PlayerStatsEntry>, DomainError>;
}
