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
    fn veto_formats(&self) -> Vec<VetoFormat> {
        Vec::new()
    }

    /// Get the default veto format for a match format.
    ///
    /// Returns the recommended veto format ID for the given match format (Bo1, Bo3, etc.).
    fn default_veto_format(&self, match_format: MatchFormat) -> Option<String> {
        let formats = self.veto_formats();
        match match_format {
            MatchFormat::Bo1 => formats.iter().find(|f| f.id.contains("bo1")).map(|f| f.id.clone()),
            MatchFormat::Bo3 => formats.iter().find(|f| f.id.contains("bo3")).map(|f| f.id.clone()),
            MatchFormat::Bo5 => formats.iter().find(|f| f.id.contains("bo5")).map(|f| f.id.clone()),
            MatchFormat::Bo7 => formats.iter().find(|f| f.id.contains("bo7")).map(|f| f.id.clone()),
        }
    }

    /// Validate a map pool for a specific veto format.
    ///
    /// Returns an error if the map pool is insufficient for the veto format.
    fn validate_map_pool_for_veto(&self, maps: &[String], veto_format_id: &str) -> Result<(), String> {
        // First validate the maps exist
        self.validate_map_pool(maps)?;

        // Then check count requirements
        if let Some(format) = self.veto_formats().iter().find(|f| f.id == veto_format_id) {
            if maps.len() < format.min_map_pool {
                return Err(format!(
                    "Map pool requires at least {} maps for {} format, got {}",
                    format.min_map_pool, format.display_name, maps.len()
                ));
            }
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
}

// ============================================================================
// Veto Types
// ============================================================================

/// Map veto format configuration.
///
/// Defines the sequence of actions (bans, picks, decider) for a veto session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoFormat {
    /// Unique identifier for this format (e.g., "bo3_veto").
    pub id: String,
    /// Display name (e.g., "Best of 3 Veto").
    pub display_name: String,
    /// Description of the format.
    pub description: String,
    /// Sequence of veto actions.
    pub sequence: Vec<VetoFormatAction>,
    /// Minimum maps required in the pool.
    pub min_map_pool: usize,
}

impl VetoFormat {
    /// Create a standard Bo1 veto format (6 bans, 1 decider).
    pub fn bo1() -> Self {
        Self {
            id: "bo1_veto".to_string(),
            display_name: "Best of 1 Veto".to_string(),
            description: "Teams alternate banning until one map remains".to_string(),
            sequence: vec![
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 0, action_type: VetoActionType::Decider },
            ],
            min_map_pool: 7,
        }
    }

    /// Create a standard Bo3 veto format (Ban-Ban-Pick-Pick-Ban-Ban-Decider).
    pub fn bo3() -> Self {
        Self {
            id: "bo3_veto".to_string(),
            display_name: "Best of 3 Veto".to_string(),
            description: "Ban-Ban-Pick-Pick-Ban-Ban-Decider".to_string(),
            sequence: vec![
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 2, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 0, action_type: VetoActionType::Decider },
            ],
            min_map_pool: 7,
        }
    }

    /// Create a standard Bo5 veto format (Ban-Ban-Pick-Pick-Pick-Pick-Decider).
    pub fn bo5() -> Self {
        Self {
            id: "bo5_veto".to_string(),
            display_name: "Best of 5 Veto".to_string(),
            description: "Ban-Ban-Pick-Pick-Pick-Pick-Decider".to_string(),
            sequence: vec![
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 2, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 1, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 2, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 0, action_type: VetoActionType::Decider },
            ],
            min_map_pool: 7,
        }
    }

    /// Get the total number of maps selected (picks + deciders).
    pub fn maps_selected(&self) -> usize {
        self.sequence
            .iter()
            .filter(|a| matches!(a.action_type, VetoActionType::Pick | VetoActionType::Decider))
            .count()
    }
}

/// A single action in the veto format sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoFormatAction {
    /// Which team performs this action.
    /// - 0 = automatic (decider)
    /// - 1 = team with first action (coin flip winner or their opponent)
    /// - 2 = team with second action
    pub team: u8,
    /// Type of action.
    pub action_type: VetoActionType,
}

/// Type of veto action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VetoActionType {
    /// Remove a map from the pool.
    Ban,
    /// Select a map to be played.
    Pick,
    /// Last remaining map (automatic selection).
    Decider,
}

impl std::fmt::Display for VetoActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ban => write!(f, "ban"),
            Self::Pick => write!(f, "pick"),
            Self::Decider => write!(f, "decider"),
        }
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
use crate::types::{
    DemoMetadata, DiscoveredEvidence, EvidenceStorage, EvidenceValidation, GameResult,
    MatchContext,
};

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
        match_context: &MatchContext,
    ) -> Result<Vec<DiscoveredEvidence>, PluginError> {
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
        claimed_result: &GameResult,
    ) -> Result<EvidenceValidation, PluginError> {
        // Default: accept without validation
        let _ = (evidence_storage, claimed_result);
        Ok(EvidenceValidation {
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
    ) -> Result<DemoMetadata, PluginError> {
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
    fn supported_evidence_types(&self) -> Vec<crate::types::EvidenceType> {
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
