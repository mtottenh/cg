//! Forfeit domain entities.
//!
//! Handles forfeit processing for no-show, withdrawal, disqualification,
//! and technical default scenarios.

use chrono::{DateTime, Utc};
use portal_core::types::MatchFormat;
use portal_core::{ForfeitRecordId, TournamentMatchId, TournamentRegistrationId, UserId};
use serde::{Deserialize, Serialize};

// =============================================================================
// FORFEIT RECORD
// =============================================================================

/// Record of a forfeit.
///
/// Created when a team cannot or will not play a match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForfeitRecord {
    pub id: ForfeitRecordId,
    pub match_id: TournamentMatchId,
    pub forfeiting_registration_id: TournamentRegistrationId,
    pub forfeit_type: ForfeitType,
    pub reason: Option<String>,
    pub triggered_by_user_id: Option<UserId>,
    pub triggered_by_system: bool,
    pub forfeited_at: DateTime<Utc>,
}

impl ForfeitRecord {
    /// Check if this was a system-triggered forfeit (e.g., check-in timeout).
    #[must_use]
    pub const fn is_system_triggered(&self) -> bool {
        self.triggered_by_system
    }

    /// Check if this was a no-show forfeit.
    #[must_use]
    pub const fn is_no_show(&self) -> bool {
        matches!(self.forfeit_type, ForfeitType::NoShow)
    }

    /// Check if this was a withdrawal.
    #[must_use]
    pub const fn is_withdrawal(&self) -> bool {
        matches!(self.forfeit_type, ForfeitType::Withdrawal)
    }

    /// Check if this was a disqualification.
    #[must_use]
    pub const fn is_disqualification(&self) -> bool {
        matches!(self.forfeit_type, ForfeitType::Disqualification)
    }
}

/// Type of forfeit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForfeitType {
    /// Team failed to check in for the match.
    NoShow,
    /// Team voluntarily withdrew from the tournament.
    Withdrawal,
    /// Team was disqualified for rule violations.
    Disqualification,
    /// Forfeit due to technical issues outside team's control.
    TechnicalDefault,
}

impl ForfeitType {
    /// Get the default walkover score for this forfeit type.
    ///
    /// Returns (winner_score, loser_score).
    #[must_use]
    pub const fn default_score(&self, match_format: MatchFormat) -> (i32, i32) {
        let winner_score = match_format.wins_required();
        (winner_score, 0)
    }

    /// Check if this forfeit type should trigger tournament-wide elimination.
    #[must_use]
    pub const fn eliminates_from_tournament(&self) -> bool {
        matches!(self, Self::Withdrawal | Self::Disqualification)
    }
}

impl std::fmt::Display for ForfeitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoShow => write!(f, "no_show"),
            Self::Withdrawal => write!(f, "withdrawal"),
            Self::Disqualification => write!(f, "disqualification"),
            Self::TechnicalDefault => write!(f, "technical_default"),
        }
    }
}

impl std::str::FromStr for ForfeitType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "no_show" => Ok(Self::NoShow),
            "withdrawal" => Ok(Self::Withdrawal),
            "disqualification" => Ok(Self::Disqualification),
            "technical_default" => Ok(Self::TechnicalDefault),
            _ => Err(format!("invalid forfeit type: {s}")),
        }
    }
}

// =============================================================================
// FORFEIT TRIGGER
// =============================================================================

/// What triggered the forfeit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForfeitTrigger {
    /// System-triggered (e.g., check-in timeout).
    System { reason: String },
    /// Triggered by the team themselves (withdrawal).
    User(UserId),
    /// Admin-triggered (disqualification).
    Admin { user_id: UserId, reason: String },
}

impl ForfeitTrigger {
    /// Check if this is a system-triggered forfeit.
    #[must_use]
    pub const fn is_system(&self) -> bool {
        matches!(self, Self::System { .. })
    }

    /// Get the user ID if available.
    #[must_use]
    pub fn user_id(&self) -> Option<UserId> {
        match self {
            Self::System { .. } => None,
            Self::User(id) => Some(*id),
            Self::Admin { user_id, .. } => Some(*user_id),
        }
    }

    /// Get the reason if available.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::System { reason } | Self::Admin { reason, .. } => Some(reason),
            Self::User(_) => None,
        }
    }
}

// =============================================================================
// COMMAND TYPES
// =============================================================================

/// Command to process a forfeit.
#[derive(Debug, Clone)]
pub struct ProcessForfeitCommand {
    pub match_id: TournamentMatchId,
    pub forfeiting_registration_id: TournamentRegistrationId,
    pub forfeit_type: ForfeitType,
    pub reason: Option<String>,
    pub triggered_by: ForfeitTrigger,
}

/// Command to withdraw from a tournament.
#[derive(Debug, Clone)]
pub struct WithdrawFromTournamentCommand {
    pub registration_id: TournamentRegistrationId,
    pub reason: Option<String>,
    pub withdrawn_by: UserId,
}

/// Command to disqualify a team.
#[derive(Debug, Clone)]
pub struct DisqualifyCommand {
    pub registration_id: TournamentRegistrationId,
    pub reason: String,
    pub disqualified_by: UserId,
}

// =============================================================================
// FORFEIT RESULT
// =============================================================================

/// Result of forfeit processing.
#[derive(Debug, Clone)]
pub struct ForfeitResult {
    /// The match that was forfeited.
    pub match_id: TournamentMatchId,
    /// The forfeit record created.
    pub forfeit_record: ForfeitRecord,
    /// The winner (opponent who gets the walkover).
    pub winner_registration_id: Option<TournamentRegistrationId>,
    /// Whether progression was triggered.
    pub progression_triggered: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forfeit_type_display() {
        assert_eq!(ForfeitType::NoShow.to_string(), "no_show");
        assert_eq!(ForfeitType::Withdrawal.to_string(), "withdrawal");
        assert_eq!(
            ForfeitType::Disqualification.to_string(),
            "disqualification"
        );
        assert_eq!(
            ForfeitType::TechnicalDefault.to_string(),
            "technical_default"
        );
    }

    #[test]
    fn test_forfeit_type_from_str() {
        assert_eq!(
            "no_show".parse::<ForfeitType>().unwrap(),
            ForfeitType::NoShow
        );
        assert_eq!(
            "withdrawal".parse::<ForfeitType>().unwrap(),
            ForfeitType::Withdrawal
        );
        assert!("invalid".parse::<ForfeitType>().is_err());
    }

    #[test]
    fn test_forfeit_type_default_score() {
        let bo3 = MatchFormat::Bo3;
        assert_eq!(ForfeitType::NoShow.default_score(bo3), (2, 0));

        let bo5 = MatchFormat::Bo5;
        assert_eq!(ForfeitType::Withdrawal.default_score(bo5), (3, 0));
    }

    #[test]
    fn test_forfeit_type_eliminates_from_tournament() {
        assert!(!ForfeitType::NoShow.eliminates_from_tournament());
        assert!(ForfeitType::Withdrawal.eliminates_from_tournament());
        assert!(ForfeitType::Disqualification.eliminates_from_tournament());
        assert!(!ForfeitType::TechnicalDefault.eliminates_from_tournament());
    }

    #[test]
    fn test_forfeit_trigger_user_id() {
        let system = ForfeitTrigger::System {
            reason: "timeout".to_string(),
        };
        assert!(system.user_id().is_none());

        let user_id = UserId::new();
        let user = ForfeitTrigger::User(user_id);
        assert_eq!(user.user_id(), Some(user_id));

        let admin = ForfeitTrigger::Admin {
            user_id,
            reason: "rule violation".to_string(),
        };
        assert_eq!(admin.user_id(), Some(user_id));
    }
}
