//! Dispute domain entities.
//!
//! Handles match result disputes and admin resolution workflow.

use chrono::{DateTime, Utc};
use portal_core::{
    DisputeId, DisputeMessageId, EvidenceId, ResultClaimId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// DISPUTE
// =============================================================================

/// A dispute against a match result.
///
/// Created when a team disagrees with a submitted result.
/// Requires admin resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispute {
    pub id: DisputeId,
    pub match_id: TournamentMatchId,
    pub result_claim_id: Option<ResultClaimId>,

    /// Who raised the dispute
    pub disputed_by_registration_id: TournamentRegistrationId,
    pub disputed_by_user_id: UserId,

    /// Dispute details
    pub reason: DisputeReason,
    pub description: String,
    pub evidence_ids: Vec<EvidenceId>,

    /// Original result being disputed
    pub original_winner_registration_id: Option<TournamentRegistrationId>,
    pub original_participant1_score: Option<i32>,
    pub original_participant2_score: Option<i32>,

    /// Status
    pub status: DisputeStatus,
    pub priority: DisputePriority,

    /// Resolution
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by_user_id: Option<UserId>,
    pub resolution: Option<DisputeResolution>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Dispute {
    /// Check if the dispute is pending.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.status, DisputeStatus::Pending)
    }

    /// Check if the dispute is under review.
    #[must_use]
    pub const fn is_under_review(&self) -> bool {
        matches!(self.status, DisputeStatus::UnderReview)
    }

    /// Check if the dispute has been resolved.
    #[must_use]
    pub const fn is_resolved(&self) -> bool {
        matches!(self.status, DisputeStatus::Resolved)
    }

    /// Check if the dispute is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            DisputeStatus::Resolved | DisputeStatus::Cancelled
        )
    }

    /// Check if the dispute can be assigned for review.
    #[must_use]
    pub const fn can_assign(&self) -> bool {
        matches!(self.status, DisputeStatus::Pending)
    }

    /// Check if the dispute can be resolved.
    #[must_use]
    pub const fn can_resolve(&self) -> bool {
        matches!(
            self.status,
            DisputeStatus::Pending | DisputeStatus::UnderReview
        )
    }
}

/// Reason for the dispute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisputeReason {
    /// Submitted score is incorrect.
    WrongScore,
    /// Wrong winner was declared.
    WrongWinner,
    /// Suspected cheating.
    Cheating,
    /// Rule violation occurred.
    RuleViolation,
    /// Technical issue affected the match.
    TechnicalIssue,
    /// Player misconduct.
    PlayerMisconduct,
    /// Other reason (described in description).
    Other,
}

impl std::fmt::Display for DisputeReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongScore => write!(f, "wrong_score"),
            Self::WrongWinner => write!(f, "wrong_winner"),
            Self::Cheating => write!(f, "cheating"),
            Self::RuleViolation => write!(f, "rule_violation"),
            Self::TechnicalIssue => write!(f, "technical_issue"),
            Self::PlayerMisconduct => write!(f, "player_misconduct"),
            Self::Other => write!(f, "other"),
        }
    }
}

impl std::str::FromStr for DisputeReason {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "wrong_score" => Ok(Self::WrongScore),
            "wrong_winner" => Ok(Self::WrongWinner),
            "cheating" => Ok(Self::Cheating),
            "rule_violation" => Ok(Self::RuleViolation),
            "technical_issue" => Ok(Self::TechnicalIssue),
            "player_misconduct" => Ok(Self::PlayerMisconduct),
            "other" => Ok(Self::Other),
            _ => Err(format!("invalid dispute reason: {s}")),
        }
    }
}

/// Status of a dispute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisputeStatus {
    /// Awaiting admin review.
    #[default]
    Pending,
    /// Admin is reviewing.
    UnderReview,
    /// Dispute has been resolved.
    Resolved,
    /// Dispute was cancelled (e.g., by the disputer).
    Cancelled,
}

impl DisputeStatus {
    /// Check if transition to target status is allowed.
    #[must_use]
    pub const fn can_transition_to(&self, target: Self) -> bool {
        matches!(
            (self, target),
            (
                Self::Pending,
                Self::UnderReview | Self::Resolved | Self::Cancelled
            ) | (Self::UnderReview, Self::Resolved | Self::Pending)
        )
    }
}

impl std::fmt::Display for DisputeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::UnderReview => write!(f, "under_review"),
            Self::Resolved => write!(f, "resolved"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for DisputeStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "under_review" => Ok(Self::UnderReview),
            "resolved" => Ok(Self::Resolved),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid dispute status: {s}")),
        }
    }
}

/// Priority of a dispute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisputePriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

impl std::fmt::Display for DisputePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Normal => write!(f, "normal"),
            Self::High => write!(f, "high"),
            Self::Urgent => write!(f, "urgent"),
        }
    }
}

impl std::str::FromStr for DisputePriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "urgent" => Ok(Self::Urgent),
            _ => Err(format!("invalid dispute priority: {s}")),
        }
    }
}

// =============================================================================
// DISPUTE RESOLUTION
// =============================================================================

/// Resolution details for a dispute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeResolution {
    pub resolution_type: ResolutionType,
    pub notes: String,
    pub new_winner_registration_id: Option<TournamentRegistrationId>,
    pub new_participant1_score: Option<i32>,
    pub new_participant2_score: Option<i32>,
}

/// Type of resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionType {
    /// Original result stands.
    Upheld,
    /// Result is reversed to new winner.
    Overturned,
    /// Match must be replayed.
    Rematch,
    /// Scores are adjusted but may or may not change winner.
    Adjusted,
    /// Both teams disqualified.
    DoubleDq,
}

impl ResolutionType {
    /// Check if this resolution changes the match result.
    #[must_use]
    pub const fn changes_result(&self) -> bool {
        matches!(self, Self::Overturned | Self::Adjusted | Self::DoubleDq)
    }

    /// Check if this resolution requires a rematch.
    #[must_use]
    pub const fn requires_rematch(&self) -> bool {
        matches!(self, Self::Rematch)
    }
}

impl std::fmt::Display for ResolutionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Upheld => write!(f, "upheld"),
            Self::Overturned => write!(f, "overturned"),
            Self::Rematch => write!(f, "rematch"),
            Self::Adjusted => write!(f, "adjusted"),
            Self::DoubleDq => write!(f, "double_dq"),
        }
    }
}

impl std::str::FromStr for ResolutionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "upheld" => Ok(Self::Upheld),
            "overturned" => Ok(Self::Overturned),
            "rematch" => Ok(Self::Rematch),
            "adjusted" => Ok(Self::Adjusted),
            "double_dq" => Ok(Self::DoubleDq),
            _ => Err(format!("invalid resolution type: {s}")),
        }
    }
}

// =============================================================================
// DISPUTE MESSAGE
// =============================================================================

/// A message in a dispute thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeMessage {
    pub id: DisputeMessageId,
    pub dispute_id: DisputeId,
    pub author_user_id: UserId,
    pub author_type: AuthorType,
    pub message: String,
    pub evidence_ids: Vec<EvidenceId>,
    pub is_internal: bool,
    pub created_at: DateTime<Utc>,
}

impl DisputeMessage {
    /// Check if this message is visible to participants.
    #[must_use]
    pub const fn is_public(&self) -> bool {
        !self.is_internal
    }

    /// Check if this is a system-generated message.
    #[must_use]
    pub const fn is_system_message(&self) -> bool {
        matches!(self.author_type, AuthorType::System)
    }
}

/// Type of message author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorType {
    /// Message from a match participant.
    Participant,
    /// Message from an admin.
    Admin,
    /// System-generated message.
    System,
}

impl std::fmt::Display for AuthorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Participant => write!(f, "participant"),
            Self::Admin => write!(f, "admin"),
            Self::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for AuthorType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "participant" => Ok(Self::Participant),
            "admin" => Ok(Self::Admin),
            "system" => Ok(Self::System),
            _ => Err(format!("invalid author type: {s}")),
        }
    }
}

// =============================================================================
// COMMAND TYPES
// =============================================================================

/// Command to raise a dispute.
#[derive(Debug, Clone)]
pub struct RaiseDisputeCommand {
    pub match_id: TournamentMatchId,
    pub result_claim_id: Option<ResultClaimId>,
    pub reason: DisputeReason,
    pub description: String,
    pub evidence_ids: Vec<EvidenceId>,
    pub disputed_by_registration_id: TournamentRegistrationId,
    pub disputed_by_user_id: UserId,
}

/// Command to add a message to a dispute.
#[derive(Debug, Clone)]
pub struct AddDisputeMessageCommand {
    pub dispute_id: DisputeId,
    pub message: String,
    pub evidence_ids: Vec<EvidenceId>,
    pub author_user_id: UserId,
    pub author_type: AuthorType,
    pub is_internal: bool,
}

/// Command to assign a dispute for review.
#[derive(Debug, Clone)]
pub struct AssignDisputeCommand {
    pub dispute_id: DisputeId,
    pub assigned_by: UserId,
}

/// Command to resolve a dispute.
#[derive(Debug, Clone)]
pub struct ResolveDisputeCommand {
    pub dispute_id: DisputeId,
    pub resolution_type: ResolutionType,
    pub notes: String,
    pub new_winner_registration_id: Option<TournamentRegistrationId>,
    pub new_participant1_score: Option<i32>,
    pub new_participant2_score: Option<i32>,
    pub resolved_by: UserId,
}

// =============================================================================
// RESULT TYPES
// =============================================================================

/// Result of dispute resolution.
#[derive(Debug, Clone)]
pub struct DisputeResolutionResult {
    /// The resolved dispute.
    pub dispute: Dispute,
    /// Progression changes made (if any).
    pub progression_changes: Option<ProgressionChanges>,
}

/// Changes made to bracket progression.
#[derive(Debug, Clone)]
pub struct ProgressionChanges {
    /// Matches that had progression reverted.
    pub reverted_matches: Vec<TournamentMatchId>,
    /// Matches that were updated with new results.
    pub updated_matches: Vec<TournamentMatchId>,
    /// New path for the correct winner.
    pub new_winner_path: Vec<TournamentMatchId>,
}

/// Dispute with its message thread.
#[derive(Debug, Clone)]
pub struct DisputeWithThread {
    pub dispute: Dispute,
    pub messages: Vec<DisputeMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispute_reason_display() {
        assert_eq!(DisputeReason::WrongScore.to_string(), "wrong_score");
        assert_eq!(DisputeReason::Cheating.to_string(), "cheating");
    }

    #[test]
    fn test_dispute_reason_from_str() {
        assert_eq!(
            "wrong_score".parse::<DisputeReason>().unwrap(),
            DisputeReason::WrongScore
        );
        assert!("invalid".parse::<DisputeReason>().is_err());
    }

    #[test]
    fn test_dispute_status_transitions() {
        assert!(DisputeStatus::Pending.can_transition_to(DisputeStatus::UnderReview));
        assert!(DisputeStatus::Pending.can_transition_to(DisputeStatus::Resolved));
        assert!(DisputeStatus::UnderReview.can_transition_to(DisputeStatus::Resolved));
        assert!(!DisputeStatus::Resolved.can_transition_to(DisputeStatus::Pending));
    }

    #[test]
    fn test_resolution_type_properties() {
        assert!(!ResolutionType::Upheld.changes_result());
        assert!(ResolutionType::Overturned.changes_result());
        assert!(ResolutionType::Rematch.requires_rematch());
        assert!(!ResolutionType::Adjusted.requires_rematch());
    }

    #[test]
    fn test_author_type_display() {
        assert_eq!(AuthorType::Participant.to_string(), "participant");
        assert_eq!(AuthorType::Admin.to_string(), "admin");
        assert_eq!(AuthorType::System.to_string(), "system");
    }
}
