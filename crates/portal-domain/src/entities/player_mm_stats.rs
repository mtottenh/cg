//! Player public matchmaking stats entity.

use chrono::{DateTime, Utc};
use portal_core::{GameId, PlayerId, PlayerMmStatsId};

/// Aggregate public matchmaking stats for a player in a game.
pub struct PlayerMmStats {
    pub id: PlayerMmStatsId,
    pub player_id: PlayerId,
    pub game_id: GameId,
    pub matches_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub headshots: i32,
    pub mvps: i32,
    pub entry_3k: i32,
    pub entry_4k: i32,
    pub entry_5k: i32,
    pub total_score: i32,
    pub total_duration_secs: i32,
    pub first_match_at: Option<DateTime<Utc>>,
    pub last_match_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PlayerMmStats {
    pub fn kd_ratio(&self) -> f64 {
        if self.deaths == 0 {
            self.kills as f64
        } else {
            self.kills as f64 / self.deaths as f64
        }
    }

    pub fn hs_percent(&self) -> f64 {
        if self.kills == 0 {
            0.0
        } else {
            self.headshots as f64 / self.kills as f64 * 100.0
        }
    }

    pub fn win_rate(&self) -> f64 {
        if self.matches_played == 0 {
            0.0
        } else {
            self.wins as f64 / self.matches_played as f64 * 100.0
        }
    }
}
