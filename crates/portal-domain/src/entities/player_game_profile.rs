//! Player game profile domain entity.

use chrono::{DateTime, Utc};
use portal_core::{GameId, PlayerGameProfileId, PlayerId};

/// A player's game-specific profile with stats, rating, and rank info.
#[derive(Debug, Clone)]
pub struct PlayerGameProfile {
    pub id: PlayerGameProfileId,
    pub player_id: PlayerId,
    pub game_id: GameId,
    // Rating System (Glicko-2)
    pub rating: i32,
    pub rating_deviation: i32,
    pub volatility: f64,
    pub peak_rating: i32,
    pub peak_rating_at: Option<DateTime<Utc>>,
    // Rank Display
    pub rank_tier: Option<String>,
    pub rank_division: Option<i32>,
    pub rank_points: Option<i32>,
    // Match Statistics
    pub matches_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub win_streak: i32,
    pub best_win_streak: i32,
    // Time Statistics
    pub total_playtime_minutes: i32,
    // Game-Specific Stats (JSONB)
    pub game_specific_stats: serde_json::Value,
    // Timestamps
    pub first_match_at: Option<DateTime<Utc>>,
    pub last_match_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PlayerGameProfile {
    /// Calculate win rate as a percentage.
    #[must_use]
    pub fn win_rate(&self) -> f64 {
        if self.matches_played == 0 {
            return 0.0;
        }
        f64::from(self.wins) / f64::from(self.matches_played) * 100.0
    }
}
