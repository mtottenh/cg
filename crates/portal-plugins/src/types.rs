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
    pub fn map_count(&self) -> u32 {
        match self {
            MatchFormat::Bo1 => 1,
            MatchFormat::Bo3 => 3,
            MatchFormat::Bo5 => 5,
            MatchFormat::Bo7 => 7,
        }
    }

    /// Get the number of wins required.
    pub fn wins_required(&self) -> u32 {
        (self.map_count() / 2) + 1
    }
}

impl std::fmt::Display for MatchFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchFormat::Bo1 => write!(f, "bo1"),
            MatchFormat::Bo3 => write!(f, "bo3"),
            MatchFormat::Bo5 => write!(f, "bo5"),
            MatchFormat::Bo7 => write!(f, "bo7"),
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
            TournamentFormatId::SingleElimination => write!(f, "single_elimination"),
            TournamentFormatId::DoubleElimination => write!(f, "double_elimination"),
            TournamentFormatId::RoundRobin => write!(f, "round_robin"),
            TournamentFormatId::Swiss => write!(f, "swiss"),
            TournamentFormatId::GroupStage => write!(f, "group_stage"),
            TournamentFormatId::Custom(s) => write!(f, "{}", s),
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
