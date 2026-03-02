//! Player game profile response DTOs.

use portal_domain::entities::PlayerGameProfile;
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

    /// Current Glicko-2 rating.
    #[schema(example = 1500)]
    pub rating: i32,

    /// Current rank tier name.
    #[schema(example = "Gold")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_tier: Option<String>,

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
}

impl From<DisplayStat> for DisplayStatResponse {
    fn from(stat: DisplayStat) -> Self {
        Self {
            key: stat.key,
            label: stat.label,
            value: stat.value,
            category: stat.category,
            sort_order: stat.sort_order,
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
            rating: profile.rating,
            rank_tier: profile.rank_tier,
            matches_played: profile.matches_played,
            wins: profile.wins,
            losses: profile.losses,
            draws: profile.draws,
            win_rate,
            win_streak: profile.win_streak,
            best_win_streak: profile.best_win_streak,
            display_stats: display_stats.into_iter().map(DisplayStatResponse::from).collect(),
            first_match_at: profile.first_match_at.map(|t| t.to_rfc3339()),
            last_match_at: profile.last_match_at.map(|t| t.to_rfc3339()),
        }
    }
}
