//! Result review response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::result_review::ResultReview;
use serde::Serialize;
use utoipa::ToSchema;

use super::DemoValidationResultResponse;

/// Unrecognized player in demo.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnrecognizedPlayerResponse {
    /// Steam ID of the player.
    pub steam_id: String,
    /// In-game player name.
    pub player_name: String,
    /// Team side in demo (e.g., "CT", "T").
    pub team_side: String,
    /// Registration side (1 or 2).
    pub registration_side: i32,
}

/// Result review response.
// API DTO mirrors the wire format: the mismatch/acknowledged flags are
// independent booleans in the JSON contract, so an enum refactor doesn't apply.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultReviewResponse {
    /// Review ID.
    pub id: String,
    /// Result claim ID.
    pub result_claim_id: String,
    /// Match ID.
    pub match_id: String,
    /// Whether there's a roster mismatch.
    pub roster_mismatch: bool,
    /// Whether there's a score mismatch.
    pub score_mismatch: bool,
    /// Whether there's a winner mismatch.
    pub winner_mismatch: bool,
    /// Demo link ID (if applicable).
    pub demo_link_id: Option<String>,
    /// Demo validation result (if applicable).
    pub validation_result: Option<DemoValidationResultResponse>,
    /// Unrecognized players from the demo.
    pub unrecognized_players: Vec<UnrecognizedPlayerResponse>,
    /// Current status.
    pub status: String,

    /// Captain 1 registration ID.
    pub captain1_registration_id: String,
    /// Whether captain 1 has acknowledged.
    pub captain1_acknowledged: bool,
    /// When captain 1 acknowledged.
    pub captain1_acknowledged_at: Option<DateTime<Utc>>,
    /// Captain 2 registration ID.
    pub captain2_registration_id: String,
    /// Whether captain 2 has acknowledged.
    pub captain2_acknowledged: bool,
    /// When captain 2 acknowledged.
    pub captain2_acknowledged_at: Option<DateTime<Utc>>,

    /// Admin who reviewed (if resolved).
    pub reviewed_by_user_id: Option<String>,
    /// When admin reviewed.
    pub reviewed_at: Option<DateTime<Utc>>,
    /// Admin notes.
    pub admin_notes: Option<String>,

    /// When the review was created.
    pub created_at: DateTime<Utc>,
}

impl From<ResultReview> for ResultReviewResponse {
    fn from(review: ResultReview) -> Self {
        Self {
            id: review.id.to_string(),
            result_claim_id: review.result_claim_id.to_string(),
            match_id: review.match_id.to_string(),
            roster_mismatch: review.roster_mismatch,
            score_mismatch: review.score_mismatch,
            winner_mismatch: review.winner_mismatch,
            demo_link_id: review.demo_link_id.map(|id| id.to_string()),
            validation_result: review
                .validation_result
                .map(DemoValidationResultResponse::from),
            unrecognized_players: review
                .unrecognized_players
                .into_iter()
                .map(|p| UnrecognizedPlayerResponse {
                    steam_id: p.steam_id,
                    player_name: p.player_name,
                    team_side: p.team_side.to_string(),
                    registration_side: p.registration_side,
                })
                .collect(),
            status: review.status.as_str().to_string(),
            captain1_registration_id: review.captain1_registration_id.to_string(),
            captain1_acknowledged: review.captain1_acknowledged,
            captain1_acknowledged_at: review.captain1_acknowledged_at,
            captain2_registration_id: review.captain2_registration_id.to_string(),
            captain2_acknowledged: review.captain2_acknowledged,
            captain2_acknowledged_at: review.captain2_acknowledged_at,
            reviewed_by_user_id: review.reviewed_by_user_id.map(|id| id.to_string()),
            reviewed_at: review.reviewed_at,
            admin_notes: review.admin_notes,
            created_at: review.created_at,
        }
    }
}

/// Summary of a result review for list views.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultReviewSummaryResponse {
    /// Review ID.
    pub id: String,
    /// Match ID.
    pub match_id: String,
    /// Status.
    pub status: String,
    /// Whether there's a roster mismatch.
    pub roster_mismatch: bool,
    /// Whether there's a score mismatch.
    pub score_mismatch: bool,
    /// Whether there's a winner mismatch.
    pub winner_mismatch: bool,
    /// When the review was created.
    pub created_at: DateTime<Utc>,
}

impl From<ResultReview> for ResultReviewSummaryResponse {
    fn from(review: ResultReview) -> Self {
        Self {
            id: review.id.to_string(),
            match_id: review.match_id.to_string(),
            status: review.status.as_str().to_string(),
            roster_mismatch: review.roster_mismatch,
            score_mismatch: review.score_mismatch,
            winner_mismatch: review.winner_mismatch,
            created_at: review.created_at,
        }
    }
}

/// List of result reviews with pagination.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultReviewListResponse {
    /// List of reviews.
    pub reviews: Vec<ResultReviewSummaryResponse>,
    /// Total count.
    pub total: i64,
}

/// Acknowledgment confirmation response.
#[derive(Debug, Serialize, ToSchema)]
pub struct AcknowledgmentResponse {
    /// Updated review.
    pub review: ResultReviewResponse,
    /// Whether both captains have now acknowledged.
    pub both_acknowledged: bool,
    /// Message.
    pub message: String,
}
