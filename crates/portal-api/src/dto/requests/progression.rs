//! Progression request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request to reapply progression with a different winner.
///
/// `participant1_score` / `participant2_score` are optional and must be given
/// together. When given they must be a finished series for the match's
/// best-of and name the new winner as the winner. When omitted, the stored
/// score is kept if the new winner already holds the higher game count and
/// mirrored if it does not (a 2–1 for participant 1 reapplied for participant
/// 2 is recorded 1–2). A stored score that does not determine a winner (e.g.
/// 0–0 on a reverted forfeit) is refused with 400: supply the score.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ReapplyProgressionRequest {
    /// The new winner registration ID
    pub new_winner_registration_id: String,
    /// Games won by participant 1 (optional; requires `participant2_score`)
    #[serde(default)]
    pub participant1_score: Option<i32>,
    /// Games won by participant 2 (optional; requires `participant1_score`)
    #[serde(default)]
    pub participant2_score: Option<i32>,
}

/// Request to process match progression with explicit winner/loser.
///
/// The optional `participant1_score` / `participant2_score` follow the same
/// rules as on [`ReapplyProgressionRequest`]; they are refused on a match
/// decided by forfeit (its walkover score cannot be replaced).
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ProcessProgressionRequest {
    /// The winner registration ID
    pub winner_registration_id: String,
    /// The loser registration ID
    pub loser_registration_id: String,
    /// Games won by participant 1 (optional; requires `participant2_score`)
    #[serde(default)]
    pub participant1_score: Option<i32>,
    /// Games won by participant 2 (optional; requires `participant1_score`)
    #[serde(default)]
    pub participant2_score: Option<i32>,
}

/// Pair up the optional explicit series score of a progression request:
/// both or neither.
pub(crate) fn explicit_scores(
    participant1_score: Option<i32>,
    participant2_score: Option<i32>,
) -> Result<Option<(i32, i32)>, &'static str> {
    match (participant1_score, participant2_score) {
        (Some(p1), Some(p2)) => Ok(Some((p1, p2))),
        (None, None) => Ok(None),
        _ => Err("participant1_score and participant2_score must be given together"),
    }
}
