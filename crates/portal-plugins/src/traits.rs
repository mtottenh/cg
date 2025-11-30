//! Plugin trait definitions.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{RatingError, StatsError};
use crate::types::{
    DisplayStat, LobbyStateMachine, MapPickBanFormat, MatchConfig, MatchData, MatchFormat,
    MatchmakingCriteria, PlayerInfo, RankedParticipant, RatingChange, TournamentFormatId,
};

// ============================================================================
// Core Plugin Trait
// ============================================================================

/// Trait for game plugins.
///
/// Game plugins provide game-specific logic for:
/// - Map definitions and pools
/// - Player statistics schemas and calculations
/// - Matchmaking criteria and validation
/// - Rating/ranking calculations
/// - Tournament format support
/// - Lobby state machines
#[async_trait]
pub trait GamePlugin: Send + Sync {
    // ========================================================================
    // Identity & Metadata
    // ========================================================================

    /// Get the plugin identifier (e.g., "cs2", "aoe4").
    fn id(&self) -> &str;

    /// Get the game display name (e.g., "Counter-Strike 2").
    fn display_name(&self) -> &str;

    /// Get a short name for the game (e.g., "CS2").
    fn short_name(&self) -> &str {
        self.id()
    }

    /// Get an optional description of the game.
    fn description(&self) -> Option<&str> {
        None
    }

    /// Get the icon URL for the game.
    fn icon_url(&self) -> Option<&str> {
        None
    }

    // ========================================================================
    // Maps
    // ========================================================================

    /// Get available maps for this game.
    fn available_maps(&self) -> Vec<MapInfo>;

    /// Get the default map pool.
    fn default_map_pool(&self) -> Vec<String>;

    /// Check if custom map pools are supported.
    fn supports_custom_map_pool(&self) -> bool {
        true
    }

    /// Validate a map pool (returns error message if invalid).
    fn validate_map_pool(&self, maps: &[String]) -> Result<(), String> {
        let available: Vec<String> = self.available_maps().iter().map(|m| m.id.clone()).collect();
        for map in maps {
            if !available.contains(map) {
                return Err(format!("Unknown map: {map}"));
            }
        }
        Ok(())
    }

    // ========================================================================
    // Team Size
    // ========================================================================

    /// Get minimum team size.
    fn team_size_min(&self) -> u32;

    /// Get maximum team size.
    fn team_size_max(&self) -> u32;

    /// Get default team size.
    fn team_size_default(&self) -> u32;

    // ========================================================================
    // Statistics
    // ========================================================================

    /// Get the JSON schema for player stats.
    fn player_stats_schema(&self) -> Value;

    /// Calculate updated player stats from match data.
    ///
    /// Takes the raw match data and the player's existing stats,
    /// returns the updated stats object.
    fn calculate_player_stats(
        &self,
        match_data: &MatchData,
        player_id: uuid::Uuid,
        existing_stats: &Value,
    ) -> Result<Value, StatsError>;

    /// Format player stats for display.
    ///
    /// Converts the raw stats JSON into a list of formatted display stats
    /// suitable for showing on player profiles.
    fn format_player_stats(&self, stats: &Value) -> Vec<DisplayStat>;

    // ========================================================================
    // Ranking
    // ========================================================================

    /// Get rank tier definitions.
    fn rank_tiers(&self) -> Vec<RankTier>;

    /// Calculate rating changes for match participants.
    ///
    /// Takes the match result and all participants with their current ratings,
    /// returns the rating changes to apply.
    fn calculate_rating_change(
        &self,
        participants: &[RankedParticipant],
    ) -> Result<Vec<RatingChange>, RatingError>;

    /// Get the rank tier for a given rating.
    fn rating_to_rank_tier(&self, rating: i32) -> Option<RankTier> {
        self.rank_tiers()
            .into_iter()
            .find(|tier| {
                rating >= tier.min_rating
                    && tier.max_rating.is_none_or(|max| rating <= max)
            })
    }

    // ========================================================================
    // Matchmaking
    // ========================================================================

    /// Get matchmaking criteria for this game.
    fn matchmaking_criteria(&self) -> MatchmakingCriteria {
        MatchmakingCriteria::default()
    }

    /// Check if a group of players can queue together.
    ///
    /// Returns Ok(()) if they can queue, or an error message explaining why not.
    fn can_queue_together(&self, players: &[PlayerInfo]) -> Result<(), String> {
        let criteria = self.matchmaking_criteria();

        if players.len() < 2 {
            return Ok(());
        }

        let ratings: Vec<i32> = players.iter().map(|p| p.rating).collect();
        let min_rating = *ratings.iter().min().unwrap();
        let max_rating = *ratings.iter().max().unwrap();
        let spread = max_rating - min_rating;

        if spread > criteria.max_party_rating_spread {
            return Err(format!(
                "Rating spread ({}) exceeds maximum allowed ({})",
                spread, criteria.max_party_rating_spread
            ));
        }

        Ok(())
    }

    /// Validate match configuration.
    fn validate_match_config(&self, config: &MatchConfig) -> Result<(), String> {
        // Validate team size
        if config.team_size < self.team_size_min() {
            return Err(format!(
                "Team size {} is below minimum {}",
                config.team_size,
                self.team_size_min()
            ));
        }
        if config.team_size > self.team_size_max() {
            return Err(format!(
                "Team size {} exceeds maximum {}",
                config.team_size,
                self.team_size_max()
            ));
        }

        // Validate map pool
        self.validate_map_pool(&config.map_pool)?;

        Ok(())
    }

    // ========================================================================
    // Tournament Support
    // ========================================================================

    /// Get tournament formats supported by this game.
    fn supported_tournament_formats(&self) -> Vec<TournamentFormatId> {
        vec![
            TournamentFormatId::SingleElimination,
            TournamentFormatId::DoubleElimination,
        ]
    }

    /// Get map pick/ban formats available for this game.
    fn map_pick_ban_formats(&self) -> Vec<MapPickBanFormat>;

    /// Get the default map pick/ban format.
    fn default_map_pick_ban_format(&self) -> Option<String> {
        self.map_pick_ban_formats().first().map(|f| f.id.clone())
    }

    /// Get the default match format.
    fn default_match_format(&self) -> MatchFormat {
        MatchFormat::Bo1
    }

    /// Get supported match formats.
    fn supported_match_formats(&self) -> Vec<MatchFormat> {
        vec![MatchFormat::Bo1, MatchFormat::Bo3, MatchFormat::Bo5]
    }

    // ========================================================================
    // Lobby (future phase)
    // ========================================================================

    /// Get the lobby state machine for this game.
    ///
    /// Returns None if this game doesn't use custom lobby logic.
    fn lobby_state_machine(&self) -> Option<Box<dyn LobbyStateMachine>> {
        None
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Information about a map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
    pub id: String,
    pub display_name: String,
    pub image_url: Option<String>,
    pub game_modes: Vec<String>,
}

/// Rank tier definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankTier {
    pub id: String,
    pub display_name: String,
    pub min_rating: i32,
    pub max_rating: Option<i32>,
    pub icon_url: Option<String>,
    pub color: Option<String>,
    pub order: i32,
}
