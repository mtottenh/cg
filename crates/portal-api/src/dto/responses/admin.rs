//! Admin response DTOs.

use serde::Serialize;
use utoipa::ToSchema;

/// Platform statistics for admin dashboard.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlatformStatsResponse {
    /// Total number of registered users.
    pub total_users: u64,
    /// Total number of player profiles.
    pub total_players: u64,
    /// Total number of teams.
    pub total_teams: u64,
    /// Total number of active games.
    pub active_games: u64,
    /// Total number of active bans.
    pub active_bans: u64,
    /// Users registered in the last 24 hours.
    pub users_last_24h: u64,
    /// Users registered in the last 7 days.
    pub users_last_7d: u64,
    /// Teams created in the last 7 days.
    pub teams_last_7d: u64,
}
