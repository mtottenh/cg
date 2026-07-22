//! Plugin trait definitions.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{RatingError, StatsError};
use crate::stats::{StatDefinition, StatFact};
use portal_core::MatchFormat;
use portal_core::types::evidence::{
    DemoFileMetadata, DiscoveredEvidenceData, EvidenceStorage, EvidenceValidationResult,
    GameMatchResult, MatchEvidenceContext,
};
use portal_core::types::veto::{SideSelectionMode, VetoFormatConfig};

use crate::types::{
    DemoData, DemoPlayerData, DisplayStat, LobbyStateMachine, MapPickBanFormat, MatchConfig,
    MatchData, MatchPlayerData, MatchTeamData, MatchmakingCriteria, PlayerInfo, PlayerStatsContext,
    RankedParticipant, RatingChange, TournamentFormatId,
};

// ============================================================================
// Helpers
// ============================================================================

/// Resolve a demo player's team_id (1 or 2) by matching their team_name
/// against the demo's team1/team2 names.
///
/// Public so plugins can reuse this in their `build_match_data_from_demo` overrides.
pub fn resolve_demo_team_id(
    player: &DemoPlayerData,
    team1_name: &str,
    team2_name: &str,
) -> Option<u32> {
    match player.team_name.as_deref() {
        Some(name) if name == team1_name => Some(1),
        Some(name) if name == team2_name => Some(2),
        _ => None,
    }
}

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
    /// suitable for showing on player profiles. The `context` provides
    /// platform-managed rating/rank data so plugins can include game-specific
    /// rating display stats.
    fn format_player_stats(&self, stats: &Value, context: &PlayerStatsContext) -> Vec<DisplayStat>;

    /// Build a `MatchData` from raw demo data.
    ///
    /// Plugins override this to control how game-specific demo fields (kills,
    /// damage, clutches, etc.) are mapped into `MatchPlayerData.game_specific_stats`.
    /// The default implementation maps the core stats (kills/deaths/assists/damage/headshots)
    /// which may be sufficient for simple games.
    fn build_match_data_from_demo(&self, demo: &DemoData) -> Result<MatchData, StatsError> {
        let winner_team_id = match demo.team1_score.cmp(&demo.team2_score) {
            std::cmp::Ordering::Greater => Some(1u32),
            std::cmp::Ordering::Less => Some(2u32),
            std::cmp::Ordering::Equal => None,
        };

        let teams = vec![
            MatchTeamData {
                team_id: 1,
                score: demo.team1_score,
                rounds_won: Some(demo.team1_score as u32),
                side_scores: None,
            },
            MatchTeamData {
                team_id: 2,
                score: demo.team2_score,
                rounds_won: Some(demo.team2_score as u32),
                side_scores: None,
            },
        ];

        let players: Vec<MatchPlayerData> = demo
            .players
            .iter()
            .filter_map(|dp| {
                let player_id = dp.player_id?;
                let team_id = resolve_demo_team_id(dp, &demo.team1_name, &demo.team2_name)?;
                Some(MatchPlayerData {
                    player_id,
                    team_id,
                    game_specific_stats: dp.stats.clone(),
                })
            })
            .collect();

        Ok(MatchData {
            match_id: demo.match_id,
            game_id: demo.game_id.clone(),
            map_id: demo.map_name.clone(),
            duration_seconds: demo.duration_seconds,
            players,
            teams,
            winner_team_id,
            game_specific_data: demo.raw_stats.clone(),
        })
    }

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
        self.rank_tiers().into_iter().find(|tier| {
            rating >= tier.min_rating && tier.max_rating.is_none_or(|max| rating <= max)
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

    /// Get the tournament plugin extension if this game supports it.
    ///
    /// Override this method to return `Some(self)` in plugins that implement
    /// the `TournamentPlugin` trait.
    fn as_tournament_plugin(&self) -> Option<&dyn TournamentPlugin> {
        None
    }

    /// Get the evidence plugin extension if this game supports it.
    ///
    /// Override this method to return `Some(self)` in plugins that implement
    /// the `EvidencePlugin` trait.
    fn as_evidence_plugin(&self) -> Option<&dyn EvidencePlugin> {
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
    /// External identifier (e.g., Steam Workshop ID).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// External URL (e.g., full Steam Workshop URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
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

// ============================================================================
// Tournament Plugin Extension
// ============================================================================

/// Extended tournament support for games.
///
/// This trait provides additional veto/pick-ban functionality
/// beyond the base `GamePlugin` trait.
pub trait TournamentPlugin: GamePlugin {
    /// Get veto formats available for this game.
    ///
    /// Returns a list of veto format configurations that define
    /// the sequence of bans, picks, and deciders.
    fn veto_formats(&self) -> Vec<VetoFormatConfig> {
        Vec::new()
    }

    /// Get the default veto format for a match format.
    ///
    /// Returns the recommended veto format ID for the given match format (Bo1, Bo3, etc.).
    fn default_veto_format(&self, match_format: MatchFormat) -> Option<String> {
        let formats = self.veto_formats();
        match match_format {
            MatchFormat::Bo1 => formats
                .iter()
                .find(|f| f.id.contains("bo1"))
                .map(|f| f.id.clone()),
            MatchFormat::Bo3 => formats
                .iter()
                .find(|f| f.id.contains("bo3"))
                .map(|f| f.id.clone()),
            MatchFormat::Bo5 => formats
                .iter()
                .find(|f| f.id.contains("bo5"))
                .map(|f| f.id.clone()),
            MatchFormat::Bo7 => formats
                .iter()
                .find(|f| f.id.contains("bo7"))
                .map(|f| f.id.clone()),
        }
    }

    /// Validate a map pool for a specific veto format.
    ///
    /// Returns an error if the map pool is insufficient for the veto format.
    fn validate_map_pool_for_veto(
        &self,
        maps: &[String],
        veto_format_id: &str,
    ) -> Result<(), String> {
        // First validate the maps exist
        self.validate_map_pool(maps)?;

        // Then check count requirements
        if let Some(format) = self.veto_formats().iter().find(|f| f.id == veto_format_id)
            && maps.len() < format.min_map_pool
        {
            return Err(format!(
                "Map pool requires at least {} maps for {} format, got {}",
                format.min_map_pool,
                format.display_name,
                maps.len()
            ));
        }

        Ok(())
    }

    /// Get metadata for a specific map.
    fn get_map_metadata(&self, map_id: &str) -> Option<MapMetadata> {
        self.available_maps()
            .into_iter()
            .find(|m| m.id == map_id)
            .map(|m| MapMetadata {
                id: m.id,
                display_name: m.display_name,
                image_url: m.image_url,
                thumbnail_url: None,
                game_modes: m.game_modes,
            })
    }

    /// Get available side options for a map.
    ///
    /// For games like CS2, this returns CT/T sides.
    /// Returns empty vec if the game doesn't have side selection.
    fn get_available_sides(&self, _map_id: &str) -> Vec<SideOption> {
        Vec::new()
    }

    /// Check if side selection is required after picks.
    fn requires_side_selection(&self) -> bool {
        !self.get_available_sides("").is_empty()
    }

    /// Get side selection modes this game supports.
    fn available_side_selection_modes(&self) -> Vec<SideSelectionMode> {
        vec![SideSelectionMode::Knife]
    }

    /// Get the default side selection mode for this game.
    fn default_side_selection_mode(&self) -> SideSelectionMode {
        SideSelectionMode::Knife
    }

    // ========================================================================
    // Award Stats
    // ========================================================================

    /// Get the stat catalog for this game.
    ///
    /// Award templates reference stats by `key`; the catalog drives UI pickers.
    /// Extraction (`extract_stat_facts`) may emit additional open-set keys
    /// (e.g. per-weapon kills) beyond what's listed here.
    fn stat_definitions(&self) -> Vec<StatDefinition> {
        vec![]
    }

    /// Extract per-player stat facts from a demo's stats JSON.
    ///
    /// Returns an EAV-shaped fact list keyed by Steam ID. Games that don't
    /// support fact extraction return an empty list.
    fn extract_stat_facts(&self, stats_json: &Value) -> Vec<StatFact> {
        let _ = stats_json;
        vec![]
    }
}

/// Extended map metadata for veto display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapMetadata {
    pub id: String,
    pub display_name: String,
    pub image_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub game_modes: Vec<String>,
}

/// Side selection option (e.g., CT vs T for CS2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideOption {
    /// Unique identifier (e.g., "ct", "t").
    pub id: String,
    /// Display name (e.g., "Counter-Terrorist").
    pub display_name: String,
    /// Short name for UI (e.g., "CT").
    pub short_name: String,
}

// ============================================================================
// Evidence Plugin Extension
// ============================================================================

use crate::error::PluginError;
use portal_core::types::evidence::EvidenceType;

/// Extension trait for evidence discovery and validation.
///
/// Games that support demo files or other evidence types should implement
/// this trait to enable automatic discovery and result validation.
#[async_trait]
pub trait EvidencePlugin: TournamentPlugin {
    /// Discover available evidence for a match.
    ///
    /// Searches storage (e.g., S3 bucket) for demo files or other evidence
    /// that might be relevant to the given match context.
    async fn discover_evidence(
        &self,
        match_context: &MatchEvidenceContext,
    ) -> Result<Vec<DiscoveredEvidenceData>, PluginError> {
        // Default: no evidence discovery
        let _ = match_context;
        Ok(Vec::new())
    }

    /// Validate evidence matches the claimed result.
    ///
    /// Parses the evidence (e.g., demo file) and compares the extracted
    /// result with the claimed result to detect discrepancies.
    async fn validate_evidence(
        &self,
        evidence_storage: &EvidenceStorage,
        claimed_result: &GameMatchResult,
    ) -> Result<EvidenceValidationResult, PluginError> {
        // Default: accept without validation
        let _ = (evidence_storage, claimed_result);
        Ok(EvidenceValidationResult {
            is_valid: true,
            confidence: 0.0, // No confidence since we didn't actually validate
            extracted_result: None,
            warnings: vec!["Evidence validation not implemented for this game".to_string()],
            errors: Vec::new(),
        })
    }

    /// Get demo file metadata without fully parsing.
    ///
    /// Reads just the header of a demo file to extract basic metadata
    /// like map name, duration, and scores.
    async fn get_demo_metadata(
        &self,
        storage: &EvidenceStorage,
    ) -> Result<DemoFileMetadata, PluginError> {
        let _ = storage;
        Err(PluginError::NotSupported(
            "Demo metadata extraction not supported for this game".to_string(),
        ))
    }

    /// Check if this game supports evidence discovery.
    fn supports_evidence_discovery(&self) -> bool {
        false
    }

    /// Check if this game supports evidence validation.
    fn supports_evidence_validation(&self) -> bool {
        false
    }

    /// Get the list of supported evidence types.
    fn supported_evidence_types(&self) -> Vec<EvidenceType> {
        Vec::new()
    }

    /// Get the demo file extension for this game (if applicable).
    fn demo_file_extension(&self) -> Option<&str> {
        None
    }

    /// Get the S3 prefix where demos for this game are stored.
    fn demo_storage_prefix(&self) -> Option<&str> {
        None
    }
}
