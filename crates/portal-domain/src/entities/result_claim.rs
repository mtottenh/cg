//! Result claim domain entities.
//!
//! The result claim system handles match result submission and confirmation.
//! One team submits a result, and the opponent confirms, disputes, or lets it
//! auto-confirm after a timeout.

use chrono::{DateTime, Utc};
use portal_core::{
    DemoMatchLinkId, EvidenceId, ResultClaimId, TournamentMatchId, TournamentRegistrationId, UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// RESULT CLAIM
// =============================================================================

/// A result claim for a match.
///
/// Represents a submitted match result awaiting confirmation from the opponent.
/// The claim includes overall scores and game-by-game results for series matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultClaim {
    pub id: ResultClaimId,
    pub match_id: TournamentMatchId,

    /// Who submitted the claim
    pub submitted_by_registration_id: TournamentRegistrationId,
    pub submitted_by_user_id: UserId,

    /// Claimed result
    pub claimed_winner_registration_id: TournamentRegistrationId,
    pub claimed_participant1_score: i32,
    pub claimed_participant2_score: i32,

    /// Game-by-game results (for series)
    pub game_results: Vec<GameResult>,

    /// Current status
    pub status: ClaimStatus,

    /// Confirmation info
    pub confirmed_at: Option<DateTime<Utc>>,
    pub confirmed_by_registration_id: Option<TournamentRegistrationId>,
    pub confirmed_by_user_id: Option<UserId>,

    /// Auto-confirmation
    pub auto_confirm_at: Option<DateTime<Utc>>,
    pub was_auto_confirmed: bool,

    /// Evidence links
    pub evidence_ids: Vec<EvidenceId>,

    /// Demo catalog links (references to demo_match_links table)
    pub demo_link_ids: Vec<DemoMatchLinkId>,

    /// Notes
    pub submitter_notes: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ResultClaim {
    /// Check if the claim is pending confirmation.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.status, ClaimStatus::Pending)
    }

    /// Check if the claim has been confirmed.
    #[must_use]
    pub const fn is_confirmed(&self) -> bool {
        matches!(self.status, ClaimStatus::Confirmed)
    }

    /// Check if the claim has been disputed.
    #[must_use]
    pub const fn is_disputed(&self) -> bool {
        matches!(self.status, ClaimStatus::Disputed)
    }

    /// Check if the claim is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            ClaimStatus::Confirmed
                | ClaimStatus::Disputed
                | ClaimStatus::Superseded
                | ClaimStatus::Cancelled
        )
    }

    /// Check if auto-confirmation is due.
    #[must_use]
    pub fn is_auto_confirm_due(&self) -> bool {
        if !self.is_pending() {
            return false;
        }
        if let Some(auto_confirm_at) = self.auto_confirm_at {
            Utc::now() >= auto_confirm_at
        } else {
            false
        }
    }

    /// Check if a user can confirm this claim.
    ///
    /// The user must be from the opponent team (not the submitter).
    #[must_use]
    pub fn can_be_confirmed_by(&self, registration_id: TournamentRegistrationId) -> bool {
        self.is_pending() && registration_id != self.submitted_by_registration_id
    }

    /// Get the series score as (participant1, participant2) tuple.
    #[must_use]
    pub const fn scores(&self) -> (i32, i32) {
        (
            self.claimed_participant1_score,
            self.claimed_participant2_score,
        )
    }

    /// Get the total number of games in this claim.
    #[must_use]
    pub fn game_count(&self) -> usize {
        self.game_results.len()
    }
}

/// Status of a result claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    /// Awaiting opponent confirmation
    #[default]
    Pending,
    /// Confirmed by opponent or auto-confirmed
    Confirmed,
    /// Disputed by opponent
    Disputed,
    /// Superseded by a newer claim
    Superseded,
    /// Cancelled by submitter
    Cancelled,
}

impl ClaimStatus {
    /// Check if transition to target status is allowed.
    #[must_use]
    pub const fn can_transition_to(&self, target: Self) -> bool {
        matches!(
            (self, target),
            (
                Self::Pending,
                Self::Confirmed | Self::Disputed | Self::Superseded | Self::Cancelled
            )
        )
    }
}

impl std::fmt::Display for ClaimStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Confirmed => write!(f, "confirmed"),
            Self::Disputed => write!(f, "disputed"),
            Self::Superseded => write!(f, "superseded"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for ClaimStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "confirmed" => Ok(Self::Confirmed),
            "disputed" => Ok(Self::Disputed),
            "superseded" => Ok(Self::Superseded),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid claim status: {s}")),
        }
    }
}

// =============================================================================
// GAME RESULT
// =============================================================================

/// Result for a single game in a series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResult {
    pub game_number: i32,
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: TournamentRegistrationId,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub evidence_ids: Vec<EvidenceId>,
    /// Demo match link ID for this specific game (from demo catalog).
    pub demo_link_id: Option<DemoMatchLinkId>,
}

impl GameResult {
    /// Check if participant 1 won this game.
    #[must_use]
    pub const fn participant1_won(&self) -> bool {
        self.participant1_score > self.participant2_score
    }

    /// Check if participant 2 won this game.
    #[must_use]
    pub const fn participant2_won(&self) -> bool {
        self.participant2_score > self.participant1_score
    }

    /// Get the winning score.
    #[must_use]
    pub const fn winning_score(&self) -> i32 {
        if self.participant1_score > self.participant2_score {
            self.participant1_score
        } else {
            self.participant2_score
        }
    }

    /// Get the losing score.
    #[must_use]
    pub const fn losing_score(&self) -> i32 {
        if self.participant1_score < self.participant2_score {
            self.participant1_score
        } else {
            self.participant2_score
        }
    }
}

/// Input for creating a game result (from API request).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResultInput {
    pub game_number: i32,
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub duration_seconds: Option<i64>,
    pub evidence_ids: Vec<EvidenceId>,
    /// Optional demo match link ID for this specific game.
    pub demo_link_id: Option<DemoMatchLinkId>,
}

// =============================================================================
// COMMAND TYPES
// =============================================================================

/// Command to create a result claim.
#[derive(Debug, Clone)]
pub struct CreateResultClaimCommand {
    pub match_id: TournamentMatchId,
    pub submitted_by_registration_id: TournamentRegistrationId,
    pub submitted_by_user_id: UserId,
    pub claimed_winner_registration_id: TournamentRegistrationId,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub game_results: Vec<GameResultInput>,
    pub evidence_ids: Vec<EvidenceId>,
    pub demo_link_ids: Vec<DemoMatchLinkId>,
    pub notes: Option<String>,
    pub auto_confirm_timeout_seconds: i64,
}

/// Command to confirm a result claim.
#[derive(Debug, Clone)]
pub struct ConfirmResultClaimCommand {
    pub claim_id: ResultClaimId,
    pub confirmed_by_registration_id: TournamentRegistrationId,
    pub confirmed_by_user_id: UserId,
}

/// Command to dispute a result claim.
#[derive(Debug, Clone)]
pub struct DisputeResultClaimCommand {
    pub claim_id: ResultClaimId,
    pub disputed_by_registration_id: TournamentRegistrationId,
    pub disputed_by_user_id: UserId,
    pub reason: String,
    pub evidence_ids: Vec<EvidenceId>,
}

/// Command to cancel a result claim.
#[derive(Debug, Clone)]
pub struct CancelResultClaimCommand {
    pub claim_id: ResultClaimId,
    pub cancelled_by_user_id: UserId,
}

// =============================================================================
// VALIDATION
// =============================================================================

/// Validation error for result claims.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ResultValidationError {
    #[error("Winner must be a match participant")]
    InvalidWinner,

    #[error("Scores don't match the claimed winner")]
    ScoreWinnerMismatch,

    #[error("Insufficient games: expected at least {required}, got {provided}")]
    InsufficientGames { required: u32, provided: u32 },

    #[error("Game scores don't sum to series score")]
    GameScoresMismatch,

    #[error("Game number {0} is not sequential")]
    NonSequentialGameNumber(i32),

    #[error("Scores must be non-negative")]
    NegativeScore,

    #[error("Game cannot end in a tie")]
    TiedGame,

    #[error("Plugin validation failed: {0}")]
    PluginValidation(String),
}
