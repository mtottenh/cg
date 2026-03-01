//! Result submission response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::result_claim::{GameResult, ResultClaim};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// RESULT CLAIM RESPONSES
// =============================================================================

/// Response DTO for a result claim.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultClaimResponse {
    /// Claim ID.
    pub id: String,
    /// Match ID.
    pub match_id: String,
    /// Registration ID of who submitted the claim.
    pub submitted_by_registration_id: String,
    /// User ID of who submitted the claim.
    pub submitted_by_user_id: String,
    /// Registration ID of claimed winner.
    pub claimed_winner_registration_id: String,
    /// Score for participant 1.
    pub claimed_participant1_score: i32,
    /// Score for participant 2.
    pub claimed_participant2_score: i32,
    /// Game-by-game results.
    pub game_results: Vec<GameResultResponse>,
    /// Current claim status.
    pub status: String,
    /// When the claim was confirmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_at: Option<DateTime<Utc>>,
    /// Registration ID of who confirmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_by_registration_id: Option<String>,
    /// User ID of who confirmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_by_user_id: Option<String>,
    /// When auto-confirmation will occur.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_confirm_at: Option<DateTime<Utc>>,
    /// Whether this was auto-confirmed.
    pub was_auto_confirmed: bool,
    /// Evidence IDs attached to this claim.
    pub evidence_ids: Vec<String>,
    /// Demo match link IDs attached to this claim (from demo catalog).
    pub demo_link_ids: Vec<String>,
    /// Submitter's notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitter_notes: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<ResultClaim> for ResultClaimResponse {
    fn from(c: ResultClaim) -> Self {
        Self {
            id: c.id.to_string(),
            match_id: c.match_id.to_string(),
            submitted_by_registration_id: c.submitted_by_registration_id.to_string(),
            submitted_by_user_id: c.submitted_by_user_id.to_string(),
            claimed_winner_registration_id: c.claimed_winner_registration_id.to_string(),
            claimed_participant1_score: c.claimed_participant1_score,
            claimed_participant2_score: c.claimed_participant2_score,
            game_results: c.game_results.into_iter().map(Into::into).collect(),
            status: c.status.to_string(),
            confirmed_at: c.confirmed_at,
            confirmed_by_registration_id: c.confirmed_by_registration_id.map(|id| id.to_string()),
            confirmed_by_user_id: c.confirmed_by_user_id.map(|id| id.to_string()),
            auto_confirm_at: c.auto_confirm_at,
            was_auto_confirmed: c.was_auto_confirmed,
            evidence_ids: c.evidence_ids.into_iter().map(|id| id.to_string()).collect(),
            demo_link_ids: c.demo_link_ids.into_iter().map(|id| id.to_string()).collect(),
            submitter_notes: c.submitter_notes,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

// =============================================================================
// GAME RESULT RESPONSES
// =============================================================================

/// Response DTO for a single game result in a series.
#[derive(Debug, Serialize, ToSchema)]
pub struct GameResultResponse {
    /// Game number (1-indexed).
    pub game_number: i32,
    /// Map ID played.
    pub map_id: String,
    /// Score for participant 1.
    pub participant1_score: i32,
    /// Score for participant 2.
    pub participant2_score: i32,
    /// Registration ID of game winner.
    pub winner_registration_id: String,
    /// When the game started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When the game completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Game duration in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i64>,
    /// Evidence IDs for this game.
    pub evidence_ids: Vec<String>,
    /// Demo match link ID for this game (from demo catalog).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demo_link_id: Option<String>,
}

impl From<GameResult> for GameResultResponse {
    fn from(g: GameResult) -> Self {
        Self {
            game_number: g.game_number,
            map_id: g.map_id,
            participant1_score: g.participant1_score,
            participant2_score: g.participant2_score,
            winner_registration_id: g.winner_registration_id.to_string(),
            started_at: g.started_at,
            completed_at: g.completed_at,
            duration_seconds: g.duration_seconds,
            evidence_ids: g.evidence_ids.into_iter().map(|id| id.to_string()).collect(),
            demo_link_id: g.demo_link_id.map(|id| id.to_string()),
        }
    }
}

// =============================================================================
// RESULT SUBMISSION RESPONSE
// =============================================================================

/// Response after submitting a result claim.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultClaimSubmissionResponse {
    /// The created result claim.
    pub claim: ResultClaimResponse,
    /// Whether any previous pending claims were superseded.
    pub superseded_previous: bool,
    /// When auto-confirmation will occur if not manually confirmed/disputed.
    pub auto_confirm_at: DateTime<Utc>,
}

/// Response after confirming a result claim.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultConfirmationResponse {
    /// The confirmed result claim.
    pub claim: ResultClaimResponse,
    /// Updated match status.
    pub match_status: String,
    /// Whether bracket was advanced.
    pub bracket_advanced: bool,
    /// Whether a review is pending (demo validation found issues).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_pending: Option<bool>,
    /// The review ID if one was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_id: Option<String>,
}

/// Response after disputing a result claim.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultDisputeResponse {
    /// The disputed result claim.
    pub claim: ResultClaimResponse,
    /// Updated match status.
    pub match_status: String,
    /// Whether admin intervention is required.
    pub requires_admin: bool,
}
