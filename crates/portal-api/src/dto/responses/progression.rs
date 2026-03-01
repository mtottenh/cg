//! Progression response DTOs.

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use portal_domain::services::tournament::{Advancement, LoserResult, ProgressionResult};

/// Response for bracket progression details.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProgressionResponse {
    /// The match ID that was processed
    pub match_id: Uuid,
    /// Winner advancement info if applicable
    pub winner_advancement: Option<AdvancementResponse>,
    /// Loser routing result
    pub loser_result: LoserResultResponse,
    /// IDs of updated standings
    pub updated_standings_count: usize,
    /// Match IDs that are now ready to start
    pub newly_ready_matches: Vec<Uuid>,
    /// Whether the bracket is now complete
    pub bracket_complete: bool,
    /// Whether the tournament is now complete
    pub tournament_complete: bool,
}

impl From<ProgressionResult> for ProgressionResponse {
    fn from(p: ProgressionResult) -> Self {
        Self {
            match_id: p.match_id.as_uuid(),
            winner_advancement: p.winner_advancement.map(Into::into),
            loser_result: p.loser_result.into(),
            updated_standings_count: p.updated_standings.len(),
            newly_ready_matches: p.newly_ready_matches.iter().map(portal_core::TournamentMatchId::as_uuid).collect(),
            bracket_complete: p.bracket_complete,
            tournament_complete: p.tournament_complete,
        }
    }
}

/// Response for winner advancement.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdvancementResponse {
    /// Target match ID
    pub target_match_id: Uuid,
    /// Position in target match (1 or 2)
    pub target_position: i32,
}

impl From<Advancement> for AdvancementResponse {
    fn from(a: Advancement) -> Self {
        Self {
            target_match_id: a.target_match_id.as_uuid(),
            target_position: a.target_position,
        }
    }
}

/// Response for loser result.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "type")]
pub enum LoserResultResponse {
    /// Loser is eliminated from the tournament
    Eliminated,
    /// Loser drops to another bracket/match
    DropsTo {
        /// Target match ID
        target_match_id: Uuid,
        /// Position in target match
        target_position: i32,
    },
    /// No loser routing applies (round robin, etc.)
    NotApplicable,
}

impl From<LoserResult> for LoserResultResponse {
    fn from(lr: LoserResult) -> Self {
        match lr {
            LoserResult::Eliminated => Self::Eliminated,
            LoserResult::DropsTo {
                target_match_id,
                target_position,
            } => Self::DropsTo {
                target_match_id: target_match_id.as_uuid(),
                target_position,
            },
            LoserResult::NotApplicable => Self::NotApplicable,
        }
    }
}

/// Response for progression log entry.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProgressionLogResponse {
    /// Log entry ID
    pub id: Uuid,
    /// Match ID
    pub match_id: Uuid,
    /// Saga ID if part of saga
    pub saga_id: Option<Uuid>,
    /// Action performed
    pub action: String,
    /// Whether action was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Details as JSON
    pub details: serde_json::Value,
    /// When the action occurred
    pub created_at: DateTime<Utc>,
}
