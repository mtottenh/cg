//! Match lifecycle entities.
//!
//! These entities support the match state machine and transition logging.

use chrono::{DateTime, Utc};
use portal_core::types::TournamentMatchStatus;
use portal_core::{MatchStatusLogId, TournamentMatchId, UserId};
use serde::{Deserialize, Serialize};

// =============================================================================
// MATCH STATUS LOG
// =============================================================================

/// A log entry recording a match status transition.
///
/// This provides an audit trail of all state changes for a match,
/// including who triggered the transition and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStatusLog {
    /// Unique identifier for this log entry.
    pub id: MatchStatusLogId,

    /// The match that transitioned.
    pub match_id: TournamentMatchId,

    /// Status before the transition.
    pub from_status: TournamentMatchStatus,

    /// Status after the transition.
    pub to_status: TournamentMatchStatus,

    /// Human-readable reason for the transition.
    pub transition_reason: Option<String>,

    /// User who triggered the transition (if not system).
    pub triggered_by_user_id: Option<UserId>,

    /// Whether the transition was triggered by a background job.
    pub triggered_by_system: bool,

    /// Additional context (job name, override reason, etc.).
    pub metadata: serde_json::Value,

    /// When the transition occurred.
    pub transitioned_at: DateTime<Utc>,
}

impl MatchStatusLog {
    /// Check if this was a user-triggered transition.
    #[must_use]
    pub const fn is_user_triggered(&self) -> bool {
        self.triggered_by_user_id.is_some() && !self.triggered_by_system
    }

    /// Check if this was a system-triggered transition.
    #[must_use]
    pub const fn is_system_triggered(&self) -> bool {
        self.triggered_by_system && self.triggered_by_user_id.is_none()
    }

    /// Check if this was an admin override.
    #[must_use]
    pub const fn is_admin_override(&self) -> bool {
        self.triggered_by_user_id.is_some() && self.triggered_by_system
    }
}

// =============================================================================
// TRANSITION TRIGGER
// =============================================================================

/// Who or what triggered a match state transition.
#[derive(Debug, Clone)]
pub enum TransitionTrigger {
    /// A regular user (participant) triggered the transition.
    User(UserId),

    /// A background system job triggered the transition.
    System {
        /// Name of the job (e.g., "check_in_expiry", "match_auto_start").
        job_name: String,
    },

    /// An admin manually triggered the transition.
    Admin {
        /// The admin's user ID.
        user_id: UserId,
        /// Reason for the override.
        override_reason: String,
    },
}

impl TransitionTrigger {
    /// Get the user ID if this was a user or admin trigger.
    #[must_use]
    pub fn user_id(&self) -> Option<UserId> {
        match self {
            Self::User(id) | Self::Admin { user_id: id, .. } => Some(*id),
            Self::System { .. } => None,
        }
    }

    /// Check if this is a system trigger.
    #[must_use]
    pub const fn is_system(&self) -> bool {
        matches!(self, Self::System { .. })
    }

    /// Convert to the database representation.
    #[must_use]
    pub fn to_db_fields(&self) -> (Option<UserId>, bool, serde_json::Value) {
        match self {
            Self::User(user_id) => (Some(*user_id), false, serde_json::json!({})),
            Self::System { job_name } => {
                (None, true, serde_json::json!({ "job_name": job_name }))
            }
            Self::Admin {
                user_id,
                override_reason,
            } => (
                Some(*user_id),
                true,
                serde_json::json!({ "admin_override": true, "reason": override_reason }),
            ),
        }
    }
}

// =============================================================================
// CREATE COMMAND
// =============================================================================

/// Command to create a match status log entry.
#[derive(Debug, Clone)]
pub struct CreateMatchStatusLogCommand {
    /// The match that transitioned.
    pub match_id: TournamentMatchId,

    /// Status before the transition.
    pub from_status: TournamentMatchStatus,

    /// Status after the transition.
    pub to_status: TournamentMatchStatus,

    /// Human-readable reason for the transition.
    pub transition_reason: Option<String>,

    /// Who triggered the transition.
    pub triggered_by: TransitionTrigger,
}
