//! Result review domain entities.
//!
//! The result review system handles validation discrepancies between claimed
//! results and demo evidence, routing issues through appropriate approval workflows.

use chrono::{DateTime, Utc};
use portal_core::{
    DemoMatchLinkId, ResultClaimId, ResultReviewId, TournamentMatchId, TournamentRegistrationId,
    UserId,
};
use serde::{Deserialize, Serialize};

use super::demo_validation::{DemoValidationResult, UnrecognizedPlayer};

// =============================================================================
// RESULT REVIEW STATUS
// =============================================================================

/// Status of a result review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResultReviewStatus {
    /// Roster mismatch only, waiting for both captains to acknowledge.
    #[default]
    PendingAcknowledgment,
    /// Score or winner mismatch, waiting for admin review.
    PendingAdminReview,
    /// Both captains have acknowledged the roster mismatch.
    Acknowledged,
    /// Admin has approved the result despite mismatches.
    Approved,
    /// Admin has rejected the result.
    Rejected,
}

impl ResultReviewStatus {
    /// Returns true if review is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }

    /// Returns true if review is pending any action.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::PendingAcknowledgment | Self::PendingAdminReview)
    }

    /// Get the string representation for database storage.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::PendingAcknowledgment => "pending_acknowledgment",
            Self::PendingAdminReview => "pending_admin_review",
            Self::Acknowledged => "acknowledged",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }
}

impl std::fmt::Display for ResultReviewStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ResultReviewStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending_acknowledgment" => Ok(Self::PendingAcknowledgment),
            "pending_admin_review" => Ok(Self::PendingAdminReview),
            "acknowledged" => Ok(Self::Acknowledged),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            _ => Err(format!("invalid result review status: {s}")),
        }
    }
}

// =============================================================================
// RESULT REVIEW
// =============================================================================

/// A review record for a result claim with validation issues.
// Each bool is an independent validation-trigger flag mirroring a DB column.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultReview {
    pub id: ResultReviewId,
    pub result_claim_id: ResultClaimId,
    pub match_id: TournamentMatchId,

    // Review triggers
    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,

    // Demo validation details
    pub demo_link_id: Option<DemoMatchLinkId>,
    pub validation_result: Option<DemoValidationResult>,
    pub unrecognized_players: Vec<UnrecognizedPlayer>,

    // Status tracking
    pub status: ResultReviewStatus,

    // Captain acknowledgments
    pub captain1_registration_id: TournamentRegistrationId,
    pub captain1_acknowledged: bool,
    pub captain1_acknowledged_at: Option<DateTime<Utc>>,
    pub captain1_acknowledged_by_user_id: Option<UserId>,

    pub captain2_registration_id: TournamentRegistrationId,
    pub captain2_acknowledged: bool,
    pub captain2_acknowledged_at: Option<DateTime<Utc>>,
    pub captain2_acknowledged_by_user_id: Option<UserId>,

    // Admin resolution
    pub reviewed_by_user_id: Option<UserId>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ResultReview {
    /// Create a new review for roster mismatch only.
    ///
    /// Status will be `PendingAcknowledgment` since only captain acknowledgment is needed.
    #[must_use]
    pub fn for_roster_mismatch(
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        demo_link_id: Option<DemoMatchLinkId>,
        validation_result: DemoValidationResult,
        unrecognized_players: Vec<UnrecognizedPlayer>,
        captain1_registration_id: TournamentRegistrationId,
        captain2_registration_id: TournamentRegistrationId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: ResultReviewId::new(),
            result_claim_id,
            match_id,
            roster_mismatch: true,
            score_mismatch: false,
            winner_mismatch: false,
            demo_link_id,
            validation_result: Some(validation_result),
            unrecognized_players,
            status: ResultReviewStatus::PendingAcknowledgment,
            captain1_registration_id,
            captain1_acknowledged: false,
            captain1_acknowledged_at: None,
            captain1_acknowledged_by_user_id: None,
            captain2_registration_id,
            captain2_acknowledged: false,
            captain2_acknowledged_at: None,
            captain2_acknowledged_by_user_id: None,
            reviewed_by_user_id: None,
            reviewed_at: None,
            admin_notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new review for score/winner mismatch.
    ///
    /// Status will be `PendingAdminReview` since admin approval is required.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn for_score_mismatch(
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        demo_link_id: Option<DemoMatchLinkId>,
        validation_result: DemoValidationResult,
        score_mismatch: bool,
        winner_mismatch: bool,
        unrecognized_players: Vec<UnrecognizedPlayer>,
        captain1_registration_id: TournamentRegistrationId,
        captain2_registration_id: TournamentRegistrationId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: ResultReviewId::new(),
            result_claim_id,
            match_id,
            roster_mismatch: !unrecognized_players.is_empty(),
            score_mismatch,
            winner_mismatch,
            demo_link_id,
            validation_result: Some(validation_result),
            unrecognized_players,
            status: ResultReviewStatus::PendingAdminReview,
            captain1_registration_id,
            captain1_acknowledged: false,
            captain1_acknowledged_at: None,
            captain1_acknowledged_by_user_id: None,
            captain2_registration_id,
            captain2_acknowledged: false,
            captain2_acknowledged_at: None,
            captain2_acknowledged_by_user_id: None,
            reviewed_by_user_id: None,
            reviewed_at: None,
            admin_notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Check if both captains have acknowledged.
    #[must_use]
    pub const fn both_captains_acknowledged(&self) -> bool {
        self.captain1_acknowledged && self.captain2_acknowledged
    }

    /// Check if this review only has roster mismatches (no score/winner issues).
    #[must_use]
    pub const fn is_roster_only(&self) -> bool {
        self.roster_mismatch && !self.score_mismatch && !self.winner_mismatch
    }

    /// Check if this review requires admin action.
    #[must_use]
    pub const fn requires_admin(&self) -> bool {
        self.score_mismatch || self.winner_mismatch
    }

    /// Check if the review has any mismatch.
    #[must_use]
    pub const fn has_any_mismatch(&self) -> bool {
        self.roster_mismatch || self.score_mismatch || self.winner_mismatch
    }

    /// Get the captain side (1 or 2) for a given registration ID.
    ///
    /// Returns `None` if the registration ID is not a captain.
    #[must_use]
    pub fn get_captain_side(&self, registration_id: TournamentRegistrationId) -> Option<i32> {
        if registration_id == self.captain1_registration_id {
            Some(1)
        } else if registration_id == self.captain2_registration_id {
            Some(2)
        } else {
            None
        }
    }

    /// Check if the given captain has already acknowledged.
    #[must_use]
    pub const fn is_captain_acknowledged(&self, captain_side: i32) -> bool {
        match captain_side {
            1 => self.captain1_acknowledged,
            2 => self.captain2_acknowledged,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portal_core::ResultClaimId;

    #[test]
    fn test_result_review_status_display() {
        assert_eq!(
            ResultReviewStatus::PendingAcknowledgment.to_string(),
            "pending_acknowledgment"
        );
        assert_eq!(
            ResultReviewStatus::PendingAdminReview.to_string(),
            "pending_admin_review"
        );
        assert_eq!(ResultReviewStatus::Acknowledged.to_string(), "acknowledged");
        assert_eq!(ResultReviewStatus::Approved.to_string(), "approved");
        assert_eq!(ResultReviewStatus::Rejected.to_string(), "rejected");
    }

    #[test]
    fn test_result_review_status_parse() {
        assert_eq!(
            "pending_acknowledgment"
                .parse::<ResultReviewStatus>()
                .unwrap(),
            ResultReviewStatus::PendingAcknowledgment
        );
        assert_eq!(
            "pending_admin_review"
                .parse::<ResultReviewStatus>()
                .unwrap(),
            ResultReviewStatus::PendingAdminReview
        );
        assert!("invalid".parse::<ResultReviewStatus>().is_err());
    }

    #[test]
    fn test_result_review_status_is_terminal() {
        assert!(!ResultReviewStatus::PendingAcknowledgment.is_terminal());
        assert!(!ResultReviewStatus::PendingAdminReview.is_terminal());
        assert!(!ResultReviewStatus::Acknowledged.is_terminal());
        assert!(ResultReviewStatus::Approved.is_terminal());
        assert!(ResultReviewStatus::Rejected.is_terminal());
    }

    #[test]
    fn test_result_review_status_is_pending() {
        assert!(ResultReviewStatus::PendingAcknowledgment.is_pending());
        assert!(ResultReviewStatus::PendingAdminReview.is_pending());
        assert!(!ResultReviewStatus::Acknowledged.is_pending());
        assert!(!ResultReviewStatus::Approved.is_pending());
        assert!(!ResultReviewStatus::Rejected.is_pending());
    }

    #[test]
    fn test_for_roster_mismatch() {
        let claim_id = ResultClaimId::new();
        let match_id = TournamentMatchId::new();
        let captain1_id = TournamentRegistrationId::new();
        let captain2_id = TournamentRegistrationId::new();
        let validation = DemoValidationResult::valid((16, 10), (16, 10));

        let review = ResultReview::for_roster_mismatch(
            claim_id,
            match_id,
            None,
            validation,
            vec![],
            captain1_id,
            captain2_id,
        );

        assert!(review.roster_mismatch);
        assert!(!review.score_mismatch);
        assert!(!review.winner_mismatch);
        assert_eq!(review.status, ResultReviewStatus::PendingAcknowledgment);
        assert!(!review.requires_admin());
    }

    #[test]
    fn test_for_score_mismatch() {
        let claim_id = ResultClaimId::new();
        let match_id = TournamentMatchId::new();
        let captain1_id = TournamentRegistrationId::new();
        let captain2_id = TournamentRegistrationId::new();
        let validation = DemoValidationResult::invalid("Score mismatch", (16, 10));

        let review = ResultReview::for_score_mismatch(
            claim_id,
            match_id,
            None,
            validation,
            true,
            false,
            vec![],
            captain1_id,
            captain2_id,
        );

        assert!(!review.roster_mismatch);
        assert!(review.score_mismatch);
        assert!(!review.winner_mismatch);
        assert_eq!(review.status, ResultReviewStatus::PendingAdminReview);
        assert!(review.requires_admin());
    }

    #[test]
    fn test_get_captain_side() {
        let claim_id = ResultClaimId::new();
        let match_id = TournamentMatchId::new();
        let captain1_id = TournamentRegistrationId::new();
        let captain2_id = TournamentRegistrationId::new();
        let validation = DemoValidationResult::valid((16, 10), (16, 10));

        let review = ResultReview::for_roster_mismatch(
            claim_id,
            match_id,
            None,
            validation,
            vec![],
            captain1_id,
            captain2_id,
        );

        assert_eq!(review.get_captain_side(captain1_id), Some(1));
        assert_eq!(review.get_captain_side(captain2_id), Some(2));
        assert_eq!(
            review.get_captain_side(TournamentRegistrationId::new()),
            None
        );
    }
}
