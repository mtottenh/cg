//! Tournament database entities.
//!
//! These entities map to the tournament-related tables:
//! `tournaments`, `tournament_stages`, `tournament_brackets`,
//! `tournament_registrations`, `tournament_matches`, `tournament_match_games`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// =============================================================================
// TOURNAMENT
// =============================================================================

/// Database row for the `tournaments` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentRow {
    pub id: Uuid,
    pub game_id: Uuid,

    // Optional linkage
    pub league_id: Option<Uuid>,
    pub season_id: Option<Uuid>,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,

    // Format
    pub format: String,
    pub format_settings: serde_json::Value,
    pub participant_type: String,
    pub team_size: Option<i32>,

    // Capacity
    pub min_participants: i32,
    pub max_participants: i32,

    // Registration
    pub registration_type: String,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub check_in_required: bool,

    // Scheduling
    pub scheduling_mode: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub timezone_hint: Option<String>,

    // Match settings
    pub default_match_format: String,
    pub default_map_veto_format: Option<String>,

    // Prize pool
    pub prize_pool: Option<serde_json::Value>,

    // Rules
    pub rules_url: Option<String>,
    pub settings: serde_json::Value,

    // Policies
    pub withdrawal_policy: String,

    // Status
    pub status: String,

    // Ownership
    pub created_by: Uuid,
    pub organization_id: Option<Uuid>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Data for inserting a new tournament.
#[derive(Debug, Clone)]
pub struct NewTournament {
    pub game_id: Uuid,
    pub league_id: Option<Uuid>,
    pub season_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub format: String,
    pub format_settings: serde_json::Value,
    pub participant_type: String,
    pub team_size: Option<i32>,
    pub min_participants: i32,
    pub max_participants: i32,
    pub registration_type: String,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_required: bool,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub scheduling_mode: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub default_match_format: String,
    pub default_map_veto_format: Option<String>,
    pub withdrawal_policy: String,
    pub rules_url: Option<String>,
    pub settings: serde_json::Value,
    pub created_by: Uuid,
}

// =============================================================================
// TOURNAMENT STAGE
// =============================================================================

/// Database row for the `tournament_stages` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentStageRow {
    pub id: Uuid,
    pub tournament_id: Uuid,

    // Identity
    pub name: String,
    pub stage_order: i32,

    // Format
    pub format: String,
    pub format_settings: serde_json::Value,

    // Advancement
    pub advancement_count: Option<i32>,
    pub advancement_rule: String,

    // Match settings
    pub match_format: Option<String>,
    pub map_veto_format: Option<String>,

    // Status
    pub status: String,

    // Timing
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new tournament stage.
#[derive(Debug, Clone)]
pub struct NewTournamentStage {
    pub tournament_id: Uuid,
    pub name: String,
    pub stage_order: i32,
    pub format: String,
    pub format_settings: serde_json::Value,
    pub advancement_count: Option<i32>,
    pub advancement_rule: String,
    pub match_format: Option<String>,
    pub map_veto_format: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TOURNAMENT BRACKET
// =============================================================================

/// Database row for the `tournament_brackets` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentBracketRow {
    pub id: Uuid,
    pub stage_id: Uuid,
    pub tournament_id: Uuid,

    // Identity
    pub name: String,
    pub bracket_type: String,

    // Structure
    pub total_rounds: i32,
    pub current_round: i32,

    // For groups
    pub group_number: Option<i32>,

    // Status
    pub status: String,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new tournament bracket.
#[derive(Debug, Clone)]
pub struct NewTournamentBracket {
    pub stage_id: Uuid,
    pub tournament_id: Uuid,
    pub name: String,
    pub bracket_type: String,
    pub total_rounds: i32,
    pub group_number: Option<i32>,
}

// =============================================================================
// TOURNAMENT REGISTRATION
// =============================================================================

/// Database row for the `tournament_registrations` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentRegistrationRow {
    pub id: Uuid,
    pub tournament_id: Uuid,

    // Participant identity
    pub team_season_id: Option<Uuid>,
    pub player_id: Option<Uuid>,
    pub adhoc_team_id: Option<Uuid>,

    // Denormalized display info
    pub participant_name: String,
    pub participant_logo_url: Option<String>,

    // Registration
    pub registered_by: Uuid,
    pub registered_at: DateTime<Utc>,

    // Check-in
    pub checked_in: bool,
    pub checked_in_at: Option<DateTime<Utc>>,
    pub checked_in_by: Option<Uuid>,

    // Seeding
    pub seed: Option<i32>,
    pub seed_rating: Option<i32>,

    // Status
    pub status: String,

    // Admin
    pub admin_notes: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub withdrawn_at: Option<DateTime<Utc>>,
}

/// Data for inserting a new tournament registration.
#[derive(Debug, Clone)]
pub struct NewTournamentRegistration {
    pub tournament_id: Uuid,
    pub team_season_id: Option<Uuid>,
    pub player_id: Option<Uuid>,
    pub adhoc_team_id: Option<Uuid>,
    pub participant_name: String,
    pub participant_logo_url: Option<String>,
    pub registered_by: Uuid,
    pub seed_rating: Option<i32>,
}

// =============================================================================
// TOURNAMENT MATCH
// =============================================================================

/// Database row for the `tournament_matches` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentMatchRow {
    pub id: Uuid,
    pub bracket_id: Uuid,
    pub stage_id: Uuid,
    pub tournament_id: Uuid,

    // Position in bracket
    pub round: i32,
    pub match_number: i32,
    pub bracket_position: String,

    // Participants
    pub participant1_registration_id: Option<Uuid>,
    pub participant2_registration_id: Option<Uuid>,

    // Denormalized participant info
    pub participant1_name: Option<String>,
    pub participant1_logo_url: Option<String>,
    pub participant1_seed: Option<i32>,
    pub participant2_name: Option<String>,
    pub participant2_logo_url: Option<String>,
    pub participant2_seed: Option<i32>,

    // Source tracking
    pub participant1_source: Option<serde_json::Value>,
    pub participant2_source: Option<serde_json::Value>,

    // Match format
    pub match_format: String,
    pub maps_required: i32,

    // Scheduling
    pub scheduled_at: Option<DateTime<Utc>>,
    pub schedule_deadline: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,

    // Results
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: Option<Uuid>,
    pub loser_registration_id: Option<Uuid>,

    // Progression
    pub winner_progresses_to: Option<Uuid>,
    pub loser_progresses_to: Option<Uuid>,

    // Status
    pub status: String,

    // Disputes
    pub disputed: bool,
    pub dispute_reason: Option<String>,
    pub dispute_resolved_by: Option<Uuid>,
    pub dispute_resolution: Option<String>,
    pub dispute_resolved_at: Option<DateTime<Utc>>,

    // VOD/Stream
    pub stream_url: Option<String>,
    pub vod_url: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new tournament match.
#[derive(Debug, Clone)]
pub struct NewTournamentMatch {
    pub bracket_id: Uuid,
    pub stage_id: Uuid,
    pub tournament_id: Uuid,
    pub round: i32,
    pub match_number: i32,
    pub bracket_position: String,
    pub participant1_registration_id: Option<Uuid>,
    pub participant2_registration_id: Option<Uuid>,
    pub participant1_name: Option<String>,
    pub participant1_logo_url: Option<String>,
    pub participant1_seed: Option<i32>,
    pub participant2_name: Option<String>,
    pub participant2_logo_url: Option<String>,
    pub participant2_seed: Option<i32>,
    pub participant1_source: Option<serde_json::Value>,
    pub participant2_source: Option<serde_json::Value>,
    pub match_format: String,
    pub maps_required: i32,
    pub winner_progresses_to: Option<Uuid>,
    pub loser_progresses_to: Option<Uuid>,
}

// =============================================================================
// TOURNAMENT MATCH GAME
// =============================================================================

/// Database row for the `tournament_match_games` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentMatchGameRow {
    pub id: Uuid,
    pub match_id: Uuid,

    // Game number in series
    pub game_number: i32,

    // Map selection
    pub map_id: Option<String>,
    pub map_picked_by: Option<Uuid>,
    pub side_selection_by: Option<Uuid>,

    // Results
    pub participant1_score: Option<i32>,
    pub participant2_score: Option<i32>,
    pub winner_registration_id: Option<Uuid>,

    // Timing
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,

    // Status
    pub status: String,

    // Game-specific data
    pub game_data: serde_json::Value,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new tournament match game.
#[derive(Debug, Clone)]
pub struct NewTournamentMatchGame {
    pub match_id: Uuid,
    pub game_number: i32,
    pub map_id: Option<String>,
}

// =============================================================================
// TOURNAMENT MAP POOL
// =============================================================================

/// Database row for the `tournament_map_pools` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentMapPoolRow {
    pub id: Uuid,
    pub tournament_id: Uuid,
    pub stage_id: Option<Uuid>,
    pub maps: Vec<String>,
    pub veto_format_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// =============================================================================
// TOURNAMENT STANDINGS
// =============================================================================

/// Database row for the `tournament_standings` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TournamentStandingRow {
    pub id: Uuid,
    pub bracket_id: Uuid,
    pub registration_id: Uuid,
    pub position: i32,
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,
    pub game_wins: i32,
    pub game_losses: i32,
    pub game_differential: i32,
    pub buchholz_score: Option<f64>,
    pub opponent_match_wins: Option<f64>,
    pub points: i32,
    pub updated_at: DateTime<Utc>,
}
