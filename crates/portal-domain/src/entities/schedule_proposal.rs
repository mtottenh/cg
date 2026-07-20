//! Schedule proposal domain entity.

use chrono::{DateTime, Utc};
use portal_core::ids::{ScheduleProposalId, TournamentMatchId, TournamentRegistrationId, UserId};
use portal_core::types::ProposalStatus;

/// A schedule proposal for a match.
///
/// Represents a team's proposal for match times that the opponent
/// can accept, reject, or counter-propose.
#[derive(Debug, Clone)]
pub struct ScheduleProposal {
    pub id: ScheduleProposalId,
    pub match_id: TournamentMatchId,

    /// Who proposed this schedule
    pub proposed_by_registration_id: TournamentRegistrationId,
    pub proposed_by_user_id: UserId,

    /// Proposed time slots (1-5 options)
    pub proposed_times: Vec<DateTime<Utc>>,

    /// Selected time (when accepted)
    pub selected_time: Option<DateTime<Utc>>,

    /// Response tracking
    pub responded_at: Option<DateTime<Utc>>,
    pub responded_by_user_id: Option<UserId>,

    /// Counter-proposal reference (if this proposal was counter-proposed)
    pub counter_proposal_id: Option<ScheduleProposalId>,

    /// Current status
    pub status: ProposalStatus,

    /// When this proposal expires
    pub expires_at: DateTime<Utc>,

    /// Admin notes
    pub notes: Option<String>,

    /// Reason provided by the responder when rejecting
    pub rejection_reason: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ScheduleProposal {
    /// Check if this proposal can be responded to.
    pub fn can_respond(&self) -> bool {
        self.status == ProposalStatus::Pending && self.expires_at > Utc::now()
    }

    /// Check if this proposal has expired.
    pub fn is_expired(&self) -> bool {
        self.status == ProposalStatus::Pending && self.expires_at <= Utc::now()
    }

    /// Check if the given time is one of the proposed times.
    pub fn contains_time(&self, time: &DateTime<Utc>) -> bool {
        self.proposed_times.iter().any(|t| t == time)
    }
}

/// Command to create a new schedule proposal.
#[derive(Debug, Clone)]
pub struct CreateScheduleProposalCommand {
    pub match_id: TournamentMatchId,
    pub proposed_by_registration_id: TournamentRegistrationId,
    pub proposed_by_user_id: UserId,
    pub proposed_times: Vec<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub notes: Option<String>,
}

/// Command to accept a schedule proposal.
#[derive(Debug, Clone)]
pub struct AcceptProposalCommand {
    pub proposal_id: ScheduleProposalId,
    pub selected_time: DateTime<Utc>,
    pub accepted_by_user_id: UserId,
}

/// Command to reject a schedule proposal.
#[derive(Debug, Clone)]
pub struct RejectProposalCommand {
    pub proposal_id: ScheduleProposalId,
    pub rejected_by_user_id: UserId,
    pub reason: Option<String>,
}

/// Command to counter-propose with new times.
#[derive(Debug, Clone)]
pub struct CounterProposeCommand {
    pub original_proposal_id: ScheduleProposalId,
    pub match_id: TournamentMatchId,
    pub proposed_by_registration_id: TournamentRegistrationId,
    pub proposed_by_user_id: UserId,
    pub proposed_times: Vec<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub notes: Option<String>,
}
