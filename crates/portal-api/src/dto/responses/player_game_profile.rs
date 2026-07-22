//! Player game profile response DTOs.

use portal_domain::entities::{
    PlayerGameProfile, PlayerMatchHistory, PlayerMmStats, PlayerRatingHistory,
};
use portal_plugins::types::DisplayStat;
use serde::Serialize;
use utoipa::ToSchema;

/// Player game profile response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlayerGameProfileResponse {
    /// Unique profile identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Player ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440001")]
    pub player_id: String,

    /// Game identifier (UUID).
    #[schema(example = "550e8400-e29b-41d4-a716-446655440002")]
    pub game_id: String,

    /// Total matches played.
    #[schema(example = 42)]
    pub matches_played: i32,

    /// Total wins.
    #[schema(example = 25)]
    pub wins: i32,

    /// Total losses.
    #[schema(example = 15)]
    pub losses: i32,

    /// Total draws.
    #[schema(example = 2)]
    pub draws: i32,

    /// Win rate as a percentage (0-100).
    #[schema(example = 59.5)]
    pub win_rate: f64,

    /// Current win streak.
    #[schema(example = 3)]
    pub win_streak: i32,

    /// Best win streak ever.
    #[schema(example = 8)]
    pub best_win_streak: i32,

    /// Plugin-formatted display stats for the game.
    ///
    /// Includes all game-specific data: rating, rank tier, combat stats, etc.
    /// Grouped by `category` (e.g., "Rating", "General", "Combat").
    pub display_stats: Vec<DisplayStatResponse>,

    /// When the player first played this game.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_match_at: Option<String>,

    /// When the player last played this game.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_match_at: Option<String>,
}

/// A formatted statistic for display on the player profile.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DisplayStatResponse {
    /// Machine-readable key.
    #[schema(example = "kd_ratio")]
    pub key: String,

    /// Human-readable label.
    #[schema(example = "K/D Ratio")]
    pub label: String,

    /// Pre-formatted value.
    #[schema(example = "1.45")]
    pub value: String,

    /// Category grouping.
    #[schema(example = "Combat")]
    pub category: String,

    /// Sort order within category.
    #[schema(example = 1)]
    pub sort_order: i32,

    /// Optional color hint (e.g., hex color for rank tier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl From<DisplayStat> for DisplayStatResponse {
    fn from(stat: DisplayStat) -> Self {
        Self {
            key: stat.key,
            label: stat.label,
            value: stat.value,
            category: stat.category,
            sort_order: stat.sort_order,
            color: stat.color,
        }
    }
}

impl PlayerGameProfileResponse {
    /// Create from a domain entity and plugin-formatted display stats.
    pub fn from_profile_with_stats(
        profile: PlayerGameProfile,
        display_stats: Vec<DisplayStat>,
    ) -> Self {
        let win_rate = profile.win_rate();
        Self {
            id: profile.id.to_string(),
            player_id: profile.player_id.to_string(),
            game_id: profile.game_id.to_string(),
            matches_played: profile.matches_played,
            wins: profile.wins,
            losses: profile.losses,
            draws: profile.draws,
            win_rate,
            win_streak: profile.win_streak,
            best_win_streak: profile.best_win_streak,
            display_stats: display_stats
                .into_iter()
                .map(DisplayStatResponse::from)
                .collect(),
            first_match_at: profile.first_match_at.map(|t| t.to_rfc3339()),
            last_match_at: profile.last_match_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// A single rating history entry.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlayerRatingHistoryResponse {
    /// Unique identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Player ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440001")]
    pub player_id: String,

    /// Game ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440002")]
    pub game_id: String,

    /// The rating at this point in time.
    #[schema(example = 15000)]
    pub rating: i32,

    /// Source of the rating update.
    #[schema(example = "mm_demo")]
    pub source: String,

    /// When the rating was observed in-game.
    pub recorded_at: String,

    /// When this record was created.
    pub created_at: String,
}

/// Public matchmaking stats card.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicMmStatsResponse {
    /// Current CS Rating (Premier).
    #[schema(example = 15000)]
    pub rating: i32,
    /// Peak rating achieved.
    #[schema(example = 16500)]
    pub peak_rating: i32,
    /// Current rank tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_tier: Option<String>,
    /// Rank tier color hex.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_color: Option<String>,

    /// Aggregate match stats from public matchmaking.
    #[schema(example = 42)]
    pub matches_played: i32,
    #[schema(example = 25)]
    pub wins: i32,
    #[schema(example = 15)]
    pub losses: i32,
    #[schema(example = 2)]
    pub draws: i32,
    #[schema(example = 59.5)]
    pub win_rate: f64,

    /// Aggregate combat stats.
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    #[schema(example = 1.45)]
    pub kd_ratio: f64,
    pub headshots: i32,
    #[schema(example = 48.5)]
    pub hs_percent: f64,
    pub mvps: i32,
    pub entry_3k: i32,
    pub entry_4k: i32,
    pub entry_5k: i32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_match_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_match_at: Option<String>,
}

impl PublicMmStatsResponse {
    pub fn from_stats_and_profile(
        stats: PlayerMmStats,
        profile: &PlayerGameProfile,
        rank_color: Option<String>,
    ) -> Self {
        Self {
            rating: profile.rating,
            peak_rating: profile.peak_rating,
            rank_tier: profile.rank_tier.clone(),
            rank_color,
            matches_played: stats.matches_played,
            wins: stats.wins,
            losses: stats.losses,
            draws: stats.draws,
            win_rate: stats.win_rate(),
            kills: stats.kills,
            deaths: stats.deaths,
            assists: stats.assists,
            kd_ratio: stats.kd_ratio(),
            headshots: stats.headshots,
            hs_percent: stats.hs_percent(),
            mvps: stats.mvps,
            entry_3k: stats.entry_3k,
            entry_4k: stats.entry_4k,
            entry_5k: stats.entry_5k,
            first_match_at: stats.first_match_at.map(|t| t.to_rfc3339()),
            last_match_at: stats.last_match_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// A single public match history entry.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MatchHistoryEntryResponse {
    pub id: String,
    pub map: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_time: Option<String>,
    pub team_scores: Vec<i32>,
    pub match_duration_secs: i32,
    pub match_result: String,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub score: i32,
    pub headshots: i32,
    pub mvps: i32,
}

impl From<PlayerMatchHistory> for MatchHistoryEntryResponse {
    fn from(h: PlayerMatchHistory) -> Self {
        Self {
            id: h.id.to_string(),
            map: h.map,
            match_time: h.match_time.map(|t| t.to_rfc3339()),
            team_scores: h.team_scores,
            match_duration_secs: h.match_duration_secs,
            match_result: h.match_result,
            kills: h.kills,
            deaths: h.deaths,
            assists: h.assists,
            score: h.score,
            headshots: h.headshots,
            mvps: h.mvps,
        }
    }
}

impl From<PlayerRatingHistory> for PlayerRatingHistoryResponse {
    fn from(h: PlayerRatingHistory) -> Self {
        Self {
            id: h.id.to_string(),
            player_id: h.player_id.to_string(),
            game_id: h.game_id.to_string(),
            rating: h.rating,
            source: h.source,
            recorded_at: h.recorded_at.to_rfc3339(),
            created_at: h.created_at.to_rfc3339(),
        }
    }
}
