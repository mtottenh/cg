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
            f64::from(self.kills)
        } else {
            f64::from(self.kills) / f64::from(self.deaths)
        }
    }

    pub fn hs_percent(&self) -> f64 {
        if self.kills == 0 {
            0.0
        } else {
            f64::from(self.headshots) / f64::from(self.kills) * 100.0
        }
    }

    pub fn win_rate(&self) -> f64 {
        if self.matches_played == 0 {
            0.0
        } else {
            f64::from(self.wins) / f64::from(self.matches_played) * 100.0
        }
    }
}
