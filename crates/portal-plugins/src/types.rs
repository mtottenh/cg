//! Type definitions for the plugin system.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Player & Match Types
// ============================================================================

/// Information about a player for matchmaking decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: Uuid,
    pub rating: i32,
    pub rating_deviation: f64,
    pub games_played: u32,
    pub rank_tier_id: Option<String>,
    pub game_stats: Value,
}

/// Configuration for a match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchConfig {
    pub game_id: String,
    pub match_format: MatchFormat,
    pub map_pool: Vec<String>,
    pub map_pick_ban_format: Option<MapPickBanFormat>,
    pub team_size: u32,
    pub allow_spectators: bool,
    pub custom_settings: Value,
}

/// Match format (best of N).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchFormat {
    Bo1,
    Bo3,
    Bo5,
    Bo7,
}

impl MatchFormat {
    /// Get the number of maps in this format.
    pub const fn map_count(&self) -> u32 {
        match self {
            Self::Bo1 => 1,
            Self::Bo3 => 3,
            Self::Bo5 => 5,
            Self::Bo7 => 7,
        }
    }

    /// Get the number of wins required.
    pub const fn wins_required(&self) -> u32 {
        (self.map_count() / 2) + 1
    }
}

impl std::fmt::Display for MatchFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bo1 => write!(f, "bo1"),
            Self::Bo3 => write!(f, "bo3"),
            Self::Bo5 => write!(f, "bo5"),
            Self::Bo7 => write!(f, "bo7"),
        }
    }
}

// ============================================================================
// Matchmaking Types
// ============================================================================

/// Criteria for matchmaking in this game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchmakingCriteria {
    /// How much rating can differ between players in the same match.
    pub max_rating_difference: i32,
    /// How much rating can differ between teams' average.
    pub max_team_rating_difference: i32,
    /// Maximum queue time before relaxing constraints (seconds).
    pub max_queue_time_seconds: u64,
    /// How much to relax rating requirements per minute of waiting.
    pub rating_relaxation_per_minute: i32,
    /// Minimum games required to use strict matchmaking.
    pub min_games_for_strict_matching: u32,
    /// Whether to allow parties with large rating differences.
    pub allow_wide_party_spread: bool,
    /// Maximum rating spread allowed in a party.
    pub max_party_rating_spread: i32,
}

impl Default for MatchmakingCriteria {
    fn default() -> Self {
        Self {
            max_rating_difference: 500,
            max_team_rating_difference: 200,
            max_queue_time_seconds: 300,
            rating_relaxation_per_minute: 50,
            min_games_for_strict_matching: 10,
            allow_wide_party_spread: false,
            max_party_rating_spread: 800,
        }
    }
}

// ============================================================================
// Statistics Types
// ============================================================================

/// Data from a completed match for stats calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchData {
    pub match_id: Uuid,
    pub game_id: String,
    pub map_id: String,
    pub duration_seconds: u64,
    pub players: Vec<MatchPlayerData>,
    pub teams: Vec<MatchTeamData>,
    pub winner_team_id: Option<u32>,
    pub game_specific_data: Value,
}

/// Player data from a match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPlayerData {
    pub player_id: Uuid,
    pub team_id: u32,
    pub game_specific_stats: Value,
}

/// Team data from a match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchTeamData {
    pub team_id: u32,
    pub score: i32,
    pub rounds_won: Option<u32>,
    pub side_scores: Option<HashMap<String, i32>>,
}

/// A formatted statistic for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayStat {
    pub key: String,
    pub label: String,
    pub value: String,
    pub category: String,
    pub sort_order: i32,
}

// ============================================================================
// Demo Data Types (input for plugin-based stats calculation)
// ============================================================================

/// Game-agnostic representation of demo data.
///
/// The adapter maps domain `Demo`/`DemoPlayer`/`ParsedDemoMetadata` entities
/// into this struct, then calls `GamePlugin::build_match_data_from_demo` so
/// the plugin can transform it into a `MatchData` with the right game-specific
/// stats schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoData {
    pub match_id: Uuid,
    pub game_id: String,
    pub map_name: String,
    pub duration_seconds: u64,
    pub team1_name: String,
    pub team2_name: String,
    pub team1_score: i32,
    pub team2_score: i32,
    pub players: Vec<DemoPlayerData>,
    /// Full raw stats JSON from demo parsing (plugin interprets this).
    pub raw_stats: Value,
}

/// A player's data extracted from a demo.
///
/// Stats are carried as raw JSON because different games have completely different
/// stat schemas (e.g. CS2 has kills/deaths/ADR, AoE2 has villagers/relics,
/// Rocket League has goals/saves/shots). The plugin knows how to interpret the JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoPlayerData {
    /// Portal player UUID (if the demo player was linked to a portal account).
    pub player_id: Option<Uuid>,
    pub player_name: String,
    pub team_name: Option<String>,
    /// All player stats from the demo as raw JSON.
    /// The structure is game-specific — each plugin defines what keys it reads.
    pub stats: Value,
}

// ============================================================================
// Ranking Types
// ============================================================================

/// A participant with their current rating for rating calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedParticipant {
    pub player_id: Uuid,
    pub team_id: u32,
    pub rating: i32,
    pub rating_deviation: f64,
    pub volatility: f64,
    pub is_winner: bool,
}

/// A rating change to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatingChange {
    pub player_id: Uuid,
    pub old_rating: i32,
    pub new_rating: i32,
    pub old_deviation: f64,
    pub new_deviation: f64,
    pub old_volatility: f64,
    pub new_volatility: f64,
}

// ============================================================================
// Map Pick/Ban Types
// ============================================================================

/// Map pick/ban format configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapPickBanFormat {
    pub id: String,
    pub display_name: String,
    /// Sequence of actions: "ban1", "ban2", "pick1", "pick2", etc.
    pub sequence: Vec<MapVetoAction>,
    /// Description of the format.
    pub description: String,
}

/// A single action in the map veto sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapVetoAction {
    /// Which team performs this action (1 or 2, or 0 for random/decider).
    pub team: u8,
    /// Type of action.
    pub action: VetoActionType,
}

/// Type of veto action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VetoActionType {
    Ban,
    Pick,
    Decider,
}

// ============================================================================
// Tournament Types
// ============================================================================

/// Tournament format identifiers that a game supports.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentFormatId {
    SingleElimination,
    DoubleElimination,
    RoundRobin,
    Swiss,
    GroupStage,
    Custom(String),
}

impl std::fmt::Display for TournamentFormatId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SingleElimination => write!(f, "single_elimination"),
            Self::DoubleElimination => write!(f, "double_elimination"),
            Self::RoundRobin => write!(f, "round_robin"),
            Self::Swiss => write!(f, "swiss"),
            Self::GroupStage => write!(f, "group_stage"),
            Self::Custom(s) => write!(f, "{s}"),
        }
    }
}

// ============================================================================
// Lobby Types (placeholder for future phases)
// ============================================================================

/// Lobby state machine trait for game-specific lobby behavior.
pub trait LobbyStateMachine: Send + Sync {
    /// Get the current state identifier.
    fn current_state(&self) -> &str;

    /// Get available transitions from the current state.
    fn available_transitions(&self) -> Vec<String>;

    /// Attempt to transition to a new state.
    fn transition(&mut self, action: &str) -> Result<(), String>;

    /// Get state data as JSON.
    fn state_data(&self) -> Value;
}

// ============================================================================
// Evidence Types
// ============================================================================

/// Context for evidence discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchContext {
    /// Tournament ID
    pub tournament_id: Uuid,
    /// Match ID
    pub match_id: Uuid,
    /// Game identifier (e.g., "cs2")
    pub game_id: String,
    /// Participants in the match
    pub participants: Vec<ParticipantContext>,
    /// When the match was scheduled
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the match started
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the match completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Context for a match participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantContext {
    /// Registration ID
    pub registration_id: Uuid,
    /// Display name
    pub name: String,
    /// Player IDs (for team registration)
    pub player_ids: Vec<Uuid>,
    /// Steam IDs (for CS2, etc.)
    pub steam_ids: Vec<String>,
}

/// Evidence discovered by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredEvidence {
    /// External identifier for this evidence
    pub external_id: String,
    /// Type of evidence
    pub evidence_type: EvidenceType,
    /// Display name
    pub name: String,
    /// Storage location
    pub storage: EvidenceStorage,
    /// File size if known
    pub file_size_bytes: Option<i64>,
    /// Plugin-specific metadata
    pub metadata: Value,
    /// When this was discovered
    pub discovered_at: chrono::DateTime<chrono::Utc>,
    /// Relevance score (0.0 to 1.0, higher = more likely to be the correct demo)
    pub relevance_score: f32,
}

/// Type of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    /// Game replay/demo file
    Demo,
    /// Screenshot image
    Screenshot,
    /// Video recording
    Video,
    /// External link
    Link,
    /// Game server log
    ServerLog,
}

impl std::fmt::Display for EvidenceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Demo => write!(f, "demo"),
            Self::Screenshot => write!(f, "screenshot"),
            Self::Video => write!(f, "video"),
            Self::Link => write!(f, "link"),
            Self::ServerLog => write!(f, "server_log"),
        }
    }
}

/// Storage location for evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvidenceStorage {
    /// Stored in S3
    S3 { bucket: String, key: String },
    /// External URL
    Url { url: String },
    /// Inline content
    Inline { content: String },
}

/// Result of evidence validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceValidation {
    /// Whether the evidence validates the claimed result
    pub is_valid: bool,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Extracted result from the evidence
    pub extracted_result: Option<ExtractedResult>,
    /// Warnings (non-fatal issues)
    pub warnings: Vec<String>,
    /// Errors (reasons for invalid)
    pub errors: Vec<String>,
}

/// Result extracted from evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedResult {
    /// Map identifier
    pub map_id: String,
    /// Score for participant 1
    pub participant1_score: i32,
    /// Score for participant 2
    pub participant2_score: i32,
    /// Duration in seconds
    pub duration_seconds: i64,
    /// Game-specific player statistics
    pub player_stats: Value,
}

/// Metadata from a demo file header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoMetadata {
    /// Map name
    pub map_name: String,
    /// Duration in seconds
    pub duration_seconds: i64,
    /// Number of players
    pub player_count: u32,
    /// Team 1 final score
    pub team1_score: i32,
    /// Team 2 final score
    pub team2_score: i32,
    /// When the demo was recorded
    pub recorded_at: chrono::DateTime<chrono::Utc>,
    /// Server name if available
    pub server_name: Option<String>,
    /// Demo file format version
    pub demo_version: String,
}

/// A claimed game result for evidence validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResult {
    /// Game number in series
    pub game_number: i32,
    /// Map ID
    pub map_id: Option<String>,
    /// Participant 1 score
    pub participant1_score: i32,
    /// Participant 2 score
    pub participant2_score: i32,
}
