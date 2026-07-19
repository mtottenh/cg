//! Stats repository for admin dashboard.

use crate::DbPool;
use crate::error::RepositoryError;

/// Platform statistics for admin dashboard.
#[derive(Debug)]
pub struct PlatformStats {
    /// Total registered users.
    pub total_users: i64,
    /// Total player profiles.
    pub total_players: i64,
    /// Total active teams.
    pub total_teams: i64,
    /// Total active games.
    pub active_games: i64,
    /// Total active bans.
    pub active_bans: i64,
    /// Users registered in last 24 hours.
    pub users_last_24h: i64,
    /// Users registered in last 7 days.
    pub users_last_7d: i64,
    /// Teams created in last 7 days.
    pub teams_last_7d: i64,
}

/// Repository for platform statistics.
#[derive(Clone)]
pub struct StatsRepository {
    pool: DbPool,
}

impl StatsRepository {
    /// Create a new stats repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Get platform-wide statistics for admin dashboard.
    pub async fn get_platform_stats(&self) -> Result<PlatformStats, RepositoryError> {
        let stats = sqlx::query_as!(
            PlatformStats,
            r#"
            SELECT
                (SELECT COUNT(*) FROM users) as "total_users!",
                (SELECT COUNT(*) FROM players) as "total_players!",
                (SELECT COUNT(*) FROM league_teams WHERE status = 'active') as "total_teams!",
                (SELECT COUNT(*) FROM games WHERE status = 'active') as "active_games!",
                (SELECT COUNT(*) FROM bans WHERE (ends_at IS NULL OR ends_at > NOW()) AND lifted_at IS NULL) as "active_bans!",
                (SELECT COUNT(*) FROM users WHERE created_at > NOW() - INTERVAL '24 hours') as "users_last_24h!",
                (SELECT COUNT(*) FROM users WHERE created_at > NOW() - INTERVAL '7 days') as "users_last_7d!",
                (SELECT COUNT(*) FROM league_teams WHERE created_at > NOW() - INTERVAL '7 days') as "teams_last_7d!"
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(stats)
    }
}
