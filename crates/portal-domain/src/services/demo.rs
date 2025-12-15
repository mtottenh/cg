//! Demo catalog service.
//!
//! Handles demo cataloging, categorization, stats processing, and match linking.

use std::sync::Arc;

use chrono::Utc;
use portal_core::{
    DemoCategory, DemoId, DemoLinkType, DemoStatus, DomainError, GameId, LeagueId, PlayerId,
    TournamentId, TournamentMatchId, UserId,
};
use tracing::{info, instrument, warn};

use crate::entities::demo::{
    Demo, DemoFilter, DemoListResult, DemoMatchLink, DemoPlayer, DemoPlayerStats,
    ParsedDemoMetadata,
};
use crate::repositories::demo::{
    CreateDemo, CreateDemoMatchLink, CreateDemoPlayer, DemoMatchLinkRepository,
    DemoMatchLinkWithData, DemoPlayerRepository, DemoRepository,
};

/// Service for managing the demo catalog.
#[derive(Clone)]
pub struct DemoService<DR, DMLR, DPR>
where
    DR: DemoRepository,
    DMLR: DemoMatchLinkRepository,
    DPR: DemoPlayerRepository,
{
    demo_repo: Arc<DR>,
    link_repo: Arc<DMLR>,
    player_repo: Arc<DPR>,
}

impl<DR, DMLR, DPR> DemoService<DR, DMLR, DPR>
where
    DR: DemoRepository,
    DMLR: DemoMatchLinkRepository,
    DPR: DemoPlayerRepository,
{
    /// Create a new demo service.
    pub fn new(demo_repo: Arc<DR>, link_repo: Arc<DMLR>, player_repo: Arc<DPR>) -> Self {
        Self {
            demo_repo,
            link_repo,
            player_repo,
        }
    }

    // =========================================================================
    // Demo Catalog Operations
    // =========================================================================

    /// Get a demo by ID.
    #[instrument(skip(self))]
    pub async fn get_demo(&self, id: DemoId) -> Result<Demo, DomainError> {
        self.demo_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::not_found("demo", id.to_string()))
    }

    /// List demos with filtering.
    #[instrument(skip(self))]
    pub async fn list_demos(&self, filter: DemoFilter) -> Result<DemoListResult, DomainError> {
        self.demo_repo.list(filter).await
    }

    /// Catalog a new demo discovered in S3.
    #[instrument(skip(self))]
    pub async fn catalog_demo(
        &self,
        game_id: GameId,
        file_name: String,
        s3_bucket: String,
        s3_key: String,
        file_size_bytes: Option<i64>,
    ) -> Result<Demo, DomainError> {
        // Check if demo already exists
        if let Some(existing) = self.demo_repo.find_by_s3_key(&s3_bucket, &s3_key).await? {
            info!(demo_id = %existing.id, "Demo already cataloged");
            return Ok(existing);
        }

        let demo = self
            .demo_repo
            .create(CreateDemo {
                game_id,
                file_name,
                s3_bucket,
                s3_key,
                file_size_bytes,
                discovered_at: Utc::now(),
            })
            .await?;

        info!(demo_id = %demo.id, "Cataloged new demo");
        Ok(demo)
    }

    /// Get demos pending stats processing.
    #[instrument(skip(self))]
    pub async fn get_pending_demos(&self, limit: i64) -> Result<Vec<Demo>, DomainError> {
        self.demo_repo.find_pending_processing(limit).await
    }

    /// Update demo status to processing.
    #[instrument(skip(self))]
    pub async fn mark_processing(&self, id: DemoId) -> Result<Demo, DomainError> {
        self.demo_repo.update_status(id, DemoStatus::Processing).await
    }

    /// Save parsed demo stats.
    #[instrument(skip(self, metadata, stats_json, players))]
    pub async fn save_demo_stats(
        &self,
        id: DemoId,
        metadata: ParsedDemoMetadata,
        stats_json: serde_json::Value,
        players: Vec<DemoPlayerInput>,
    ) -> Result<Demo, DomainError> {
        // Update demo with stats
        let demo = self
            .demo_repo
            .update_stats(id, metadata, stats_json)
            .await?;

        // Create player entries
        let player_creates: Vec<CreateDemoPlayer> = players
            .into_iter()
            .map(|p| CreateDemoPlayer {
                steam_id: p.steam_id,
                player_name: p.player_name,
                team_name: p.team_name,
                stats: p.stats,
            })
            .collect();

        if !player_creates.is_empty() {
            self.player_repo.create_batch(id, player_creates).await?;
        }

        info!(demo_id = %id, "Saved demo stats");
        Ok(demo)
    }

    /// Mark demo stats fetch as failed.
    #[instrument(skip(self))]
    pub async fn mark_stats_failed(&self, id: DemoId, error: &str) -> Result<Demo, DomainError> {
        warn!(demo_id = %id, error = %error, "Demo stats fetch failed");
        self.demo_repo.mark_stats_failed(id, error).await
    }

    // =========================================================================
    // Categorization & Visibility
    // =========================================================================

    /// Categorize a demo (PUG, League, Scrim, Ignored).
    #[instrument(skip(self))]
    pub async fn categorize_demo(
        &self,
        id: DemoId,
        category: DemoCategory,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError> {
        let demo = self.demo_repo.categorize(id, category, by_user_id).await?;
        info!(demo_id = %id, category = %category, by_user = %by_user_id, "Categorized demo");
        Ok(demo)
    }

    /// Hide or unhide a demo.
    #[instrument(skip(self))]
    pub async fn set_demo_visibility(
        &self,
        id: DemoId,
        is_hidden: bool,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError> {
        let demo = self
            .demo_repo
            .set_visibility(id, is_hidden, by_user_id)
            .await?;
        info!(demo_id = %id, hidden = is_hidden, by_user = %by_user_id, "Updated demo visibility");
        Ok(demo)
    }

    /// Associate a demo with a league or tournament.
    #[instrument(skip(self))]
    pub async fn associate_demo(
        &self,
        id: DemoId,
        league_id: Option<LeagueId>,
        tournament_id: Option<TournamentId>,
    ) -> Result<Demo, DomainError> {
        self.demo_repo.associate(id, league_id, tournament_id).await
    }

    /// Set admin notes on a demo.
    #[instrument(skip(self))]
    pub async fn set_admin_notes(
        &self,
        id: DemoId,
        notes: Option<String>,
    ) -> Result<Demo, DomainError> {
        self.demo_repo.set_admin_notes(id, notes).await
    }

    // =========================================================================
    // Match Linking
    // =========================================================================

    /// Link a demo to a tournament match.
    #[instrument(skip(self))]
    pub async fn link_to_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
        game_number: Option<i32>,
        link_type: DemoLinkType,
        linked_by: Option<UserId>,
    ) -> Result<DemoMatchLink, DomainError> {
        // Check if link already exists
        if self
            .link_repo
            .find_by_demo_and_match(demo_id, match_id)
            .await?
            .is_some()
        {
            return Err(DomainError::conflict(format!(
                "Demo {} is already linked to match {}",
                demo_id, match_id
            )));
        }

        let link = self
            .link_repo
            .create(CreateDemoMatchLink {
                demo_id,
                match_id,
                game_number,
                link_type,
                confidence_score: None,
                linked_by_user_id: linked_by,
            })
            .await?;

        info!(demo_id = %demo_id, match_id = %match_id, "Linked demo to match");
        Ok(link)
    }

    /// Unlink a demo from a match.
    #[instrument(skip(self))]
    pub async fn unlink_from_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError> {
        let link = self
            .link_repo
            .find_by_demo_and_match(demo_id, match_id)
            .await?
            .ok_or_else(|| {
                DomainError::not_found(
                    "demo match link",
                    format!("demo={},match={}", demo_id, match_id),
                )
            })?;

        self.link_repo.delete(link.id).await?;
        info!(demo_id = %demo_id, match_id = %match_id, "Unlinked demo from match");
        Ok(())
    }

    /// Get all demos linked to a match.
    #[instrument(skip(self))]
    pub async fn get_match_demos(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<DemoMatchLink>, DomainError> {
        self.link_repo.find_by_match(match_id).await
    }

    /// Get all demos linked to a match with full demo and player data.
    ///
    /// This is useful for displaying match demos with all their details.
    #[instrument(skip(self))]
    pub async fn get_match_demos_with_data(
        &self,
        match_id: TournamentMatchId,
        include_stats: bool,
        game_number: Option<i32>,
    ) -> Result<Vec<DemoMatchLinkWithData>, DomainError> {
        let mut results = self.link_repo.find_by_match_with_demos(match_id).await?;

        // Filter by game number if specified
        if let Some(gn) = game_number {
            results.retain(|r| r.link.game_number == Some(gn));
        }

        // Optionally strip players if not needed
        if !include_stats {
            for result in &mut results {
                result.players.clear();
            }
        }

        Ok(results)
    }

    /// Get all matches linked to a demo.
    #[instrument(skip(self))]
    pub async fn get_demo_links(&self, demo_id: DemoId) -> Result<Vec<DemoMatchLink>, DomainError> {
        self.link_repo.find_by_demo(demo_id).await
    }

    // =========================================================================
    // Player Queries
    // =========================================================================

    /// Get players from a demo.
    #[instrument(skip(self))]
    pub async fn get_demo_players(&self, demo_id: DemoId) -> Result<Vec<DemoPlayer>, DomainError> {
        self.player_repo.find_by_demo(demo_id).await
    }

    /// Find all demos featuring a player by Steam ID.
    #[instrument(skip(self))]
    pub async fn find_demos_by_steam_id(&self, steam_id: &str) -> Result<Vec<DemoId>, DomainError> {
        self.player_repo.find_demos_by_steam_id(steam_id).await
    }

    /// Link a demo player entry to a portal player account.
    #[instrument(skip(self))]
    pub async fn link_player_account(
        &self,
        demo_player_id: portal_core::DemoPlayerId,
        player_id: PlayerId,
    ) -> Result<DemoPlayer, DomainError> {
        self.player_repo.link_to_player(demo_player_id, player_id).await
    }

    // =========================================================================
    // Admin Operations
    // =========================================================================

    /// Get demo count by status (for admin dashboard).
    #[instrument(skip(self))]
    pub async fn get_status_counts(&self) -> Result<Vec<(DemoStatus, i64)>, DomainError> {
        self.demo_repo.count_by_status().await
    }

    /// Delete a demo and all associated data.
    #[instrument(skip(self))]
    pub async fn delete_demo(&self, id: DemoId) -> Result<(), DomainError> {
        // Delete player entries first
        self.player_repo.delete_by_demo(id).await?;

        // Delete match links
        self.link_repo.delete_by_demo(id).await?;

        // Delete the demo
        self.demo_repo.delete(id).await?;

        info!(demo_id = %id, "Deleted demo and all associated data");
        Ok(())
    }
}

/// Input for creating demo player entries.
#[derive(Debug, Clone)]
pub struct DemoPlayerInput {
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,
    pub stats: DemoPlayerStats,
}
