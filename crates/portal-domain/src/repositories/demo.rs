//! Demo repository traits.
//!
//! Repository for demo catalog operations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{
    DemoCategory, DemoId, DemoLinkType, DemoMatchLinkId, DemoPlayerId, DemoStatus, GameId,
    DomainError, LeagueId, PlayerId, TournamentId, TournamentMatchId, UserId,
};

use crate::entities::demo::{
    Demo, DemoFilter, DemoListResult, DemoMatchLink, DemoPlayer, DemoPlayerStats,
    ParsedDemoMetadata,
};

// =============================================================================
// DEMO REPOSITORY
// =============================================================================

/// Repository trait for demo catalog operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DemoRepository: Send + Sync {
    /// Find demo by ID.
    async fn find_by_id(&self, id: DemoId) -> Result<Option<Demo>, DomainError>;

    /// Find demo by S3 key (unique identifier in bucket).
    async fn find_by_s3_key(&self, bucket: &str, key: &str) -> Result<Option<Demo>, DomainError>;

    /// List demos with filtering and pagination.
    async fn list(&self, filter: DemoFilter) -> Result<DemoListResult, DomainError>;

    /// Create a new demo entry.
    async fn create(&self, demo: CreateDemo) -> Result<Demo, DomainError>;

    /// Update demo metadata after stats processing.
    async fn update_stats(
        &self,
        id: DemoId,
        metadata: ParsedDemoMetadata,
        stats_json: serde_json::Value,
    ) -> Result<Demo, DomainError>;

    /// Update demo status (processing state).
    async fn update_status(&self, id: DemoId, status: DemoStatus) -> Result<Demo, DomainError>;

    /// Mark demo stats fetch as failed with error message.
    async fn mark_stats_failed(&self, id: DemoId, error: &str) -> Result<Demo, DomainError>;

    /// Categorize a demo.
    async fn categorize(
        &self,
        id: DemoId,
        category: DemoCategory,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError>;

    /// Set demo visibility (hide/unhide).
    async fn set_visibility(
        &self,
        id: DemoId,
        is_hidden: bool,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError>;

    /// Associate demo with league/tournament.
    async fn associate(
        &self,
        id: DemoId,
        league_id: Option<LeagueId>,
        tournament_id: Option<TournamentId>,
    ) -> Result<Demo, DomainError>;

    /// Set admin notes on a demo.
    async fn set_admin_notes(
        &self,
        id: DemoId,
        notes: Option<String>,
    ) -> Result<Demo, DomainError>;

    /// Find demos pending stats processing.
    async fn find_pending_processing(&self, limit: i64) -> Result<Vec<Demo>, DomainError>;

    /// Count demos by status (for admin dashboard).
    async fn count_by_status(&self) -> Result<Vec<(DemoStatus, i64)>, DomainError>;

    /// Delete a demo (hard delete, use with caution).
    async fn delete(&self, id: DemoId) -> Result<(), DomainError>;
}

/// Data for creating a demo catalog entry.
#[derive(Debug, Clone)]
pub struct CreateDemo {
    pub game_id: GameId,
    pub file_name: String,
    pub s3_bucket: String,
    pub s3_key: String,
    pub file_size_bytes: Option<i64>,
    pub discovered_at: DateTime<Utc>,
}

// =============================================================================
// DEMO MATCH LINK REPOSITORY
// =============================================================================

/// Repository trait for demo-match link operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DemoMatchLinkRepository: Send + Sync {
    /// Find link by ID.
    async fn find_by_id(&self, id: DemoMatchLinkId) -> Result<Option<DemoMatchLink>, DomainError>;

    /// Find links by multiple IDs.
    async fn find_by_ids(&self, ids: &[DemoMatchLinkId]) -> Result<Vec<DemoMatchLink>, DomainError>;

    /// Find all links for a demo.
    async fn find_by_demo(&self, demo_id: DemoId) -> Result<Vec<DemoMatchLink>, DomainError>;

    /// Find all links for a match.
    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<DemoMatchLink>, DomainError>;

    /// Find all links for a match with full demo and player data.
    ///
    /// Returns tuples of (link, demo, players) for each linked demo.
    async fn find_by_match_with_demos(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<DemoMatchLinkWithData>, DomainError>;

    /// Find link between specific demo and match.
    async fn find_by_demo_and_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
    ) -> Result<Option<DemoMatchLink>, DomainError>;

    /// Create a new demo-match link.
    async fn create(&self, link: CreateDemoMatchLink) -> Result<DemoMatchLink, DomainError>;

    /// Update link validation result.
    async fn mark_validated(
        &self,
        id: DemoMatchLinkId,
        validation_result: serde_json::Value,
    ) -> Result<DemoMatchLink, DomainError>;

    /// Delete a link (unlink demo from match).
    async fn delete(&self, id: DemoMatchLinkId) -> Result<(), DomainError>;

    /// Delete all links for a demo.
    async fn delete_by_demo(&self, demo_id: DemoId) -> Result<(), DomainError>;
}

/// Data for creating a demo-match link.
#[derive(Debug, Clone)]
pub struct CreateDemoMatchLink {
    pub demo_id: DemoId,
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,
    pub link_type: DemoLinkType,
    pub confidence_score: Option<f32>,
    pub linked_by_user_id: Option<UserId>,
}

// =============================================================================
// DEMO PLAYER REPOSITORY
// =============================================================================

/// Repository trait for demo player operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DemoPlayerRepository: Send + Sync {
    /// Find player entry by ID.
    async fn find_by_id(&self, id: DemoPlayerId) -> Result<Option<DemoPlayer>, DomainError>;

    /// Find all players in a demo.
    async fn find_by_demo(&self, demo_id: DemoId) -> Result<Vec<DemoPlayer>, DomainError>;

    /// Find demos by Steam ID (player's game identity).
    async fn find_demos_by_steam_id(&self, steam_id: &str) -> Result<Vec<DemoId>, DomainError>;

    /// Find player entries by Steam ID.
    async fn find_by_steam_id(&self, steam_id: &str) -> Result<Vec<DemoPlayer>, DomainError>;

    /// Create player entries for a demo (batch insert).
    async fn create_batch(
        &self,
        demo_id: DemoId,
        players: Vec<CreateDemoPlayer>,
    ) -> Result<Vec<DemoPlayer>, DomainError>;

    /// Link a demo player to a portal player account.
    async fn link_to_player(
        &self,
        id: DemoPlayerId,
        player_id: PlayerId,
    ) -> Result<DemoPlayer, DomainError>;

    /// Delete all player entries for a demo.
    async fn delete_by_demo(&self, demo_id: DemoId) -> Result<(), DomainError>;
}

/// Data for creating a demo player entry.
#[derive(Debug, Clone)]
pub struct CreateDemoPlayer {
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,
    pub stats: DemoPlayerStats,
}

// =============================================================================
// DEMO MATCH LINK WITH DATA
// =============================================================================

/// A demo-match link with full demo and player data.
#[derive(Debug, Clone)]
pub struct DemoMatchLinkWithData {
    /// The link between demo and match.
    pub link: DemoMatchLink,
    /// The demo details.
    pub demo: Demo,
    /// The players in this demo.
    pub players: Vec<DemoPlayer>,
}
