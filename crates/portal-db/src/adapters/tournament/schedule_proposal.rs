//! PostgreSQL implementation of the schedule proposal repository.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{ScheduleProposalId, TournamentMatchId, TournamentRegistrationId, UserId};
use portal_core::types::ProposalStatus;
use portal_domain::entities::{CreateScheduleProposalCommand, ScheduleProposal};
use portal_domain::repositories::ScheduleProposalRepository;

use crate::DbPool;
use crate::entities::tournament::{NewScheduleProposal, ScheduleProposalRow};

/// PostgreSQL implementation of the schedule proposal repository.
#[derive(Clone)]
pub struct PgScheduleProposalRepository {
    pool: DbPool,
}

impl PgScheduleProposalRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduleProposalRepository for PgScheduleProposalRepository {
    async fn create(
        &self,
        command: CreateScheduleProposalCommand,
    ) -> Result<ScheduleProposal, DomainError> {
        let new_proposal = NewScheduleProposal {
            match_id: command.match_id.into(),
            proposed_by_registration_id: command.proposed_by_registration_id.into(),
            proposed_by_user_id: command.proposed_by_user_id.into(),
            proposed_times: command.proposed_times,
            expires_at: command.expires_at,
            notes: command.notes,
        };

        let row = sqlx::query_as::<_, ScheduleProposalRow>(
            r"
            INSERT INTO schedule_proposals (
                match_id,
                proposed_by_registration_id,
                proposed_by_user_id,
                proposed_times,
                expires_at,
                notes,
                status
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'pending')
            RETURNING *
            ",
        )
        .bind(new_proposal.match_id)
        .bind(new_proposal.proposed_by_registration_id)
        .bind(new_proposal.proposed_by_user_id)
        .bind(&new_proposal.proposed_times)
        .bind(new_proposal.expires_at)
        .bind(&new_proposal.notes)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Schedule proposal database error: {e}")))?;

        Ok(row_to_proposal(row))
    }

    async fn find_by_id(
        &self,
        id: ScheduleProposalId,
    ) -> Result<Option<ScheduleProposal>, DomainError> {
        let row = sqlx::query_as::<_, ScheduleProposalRow>(
            "SELECT * FROM schedule_proposals WHERE id = $1",
        )
        .bind::<uuid::Uuid>(id.into())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Schedule proposal database error: {e}")))?;

        Ok(row.map(row_to_proposal))
    }

    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ScheduleProposal>, DomainError> {
        let rows = sqlx::query_as::<_, ScheduleProposalRow>(
            "SELECT * FROM schedule_proposals WHERE match_id = $1 ORDER BY created_at DESC",
        )
        .bind::<uuid::Uuid>(match_id.into())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Schedule proposal database error: {e}")))?;

        Ok(rows.into_iter().map(row_to_proposal).collect())
    }

    async fn find_pending_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ScheduleProposal>, DomainError> {
        let row = sqlx::query_as::<_, ScheduleProposalRow>(
            r"
            SELECT * FROM schedule_proposals
            WHERE match_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind::<uuid::Uuid>(match_id.into())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Schedule proposal database error: {e}")))?;

        Ok(row.map(row_to_proposal))
    }

    async fn update(&self, proposal: &ScheduleProposal) -> Result<ScheduleProposal, DomainError> {
        let row = sqlx::query_as::<_, ScheduleProposalRow>(
            r"
            UPDATE schedule_proposals
            SET
                selected_time = $1,
                responded_at = $2,
                responded_by_user_id = $3,
                counter_proposal_id = $4,
                status = $5,
                notes = $6,
                rejection_reason = $7,
                updated_at = NOW()
            WHERE id = $8
            RETURNING *
            ",
        )
        .bind(proposal.selected_time)
        .bind(proposal.responded_at)
        .bind(proposal.responded_by_user_id.map(uuid::Uuid::from))
        .bind(proposal.counter_proposal_id.map(uuid::Uuid::from))
        .bind(proposal.status.as_str())
        .bind(&proposal.notes)
        .bind(&proposal.rejection_reason)
        .bind::<uuid::Uuid>(proposal.id.into())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Schedule proposal database error: {e}")))?;

        Ok(row_to_proposal(row))
    }

    async fn find_expired(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ScheduleProposal>, DomainError> {
        let rows = sqlx::query_as::<_, ScheduleProposalRow>(
            r"
            SELECT * FROM schedule_proposals
            WHERE status = 'pending' AND expires_at < $1
            ORDER BY expires_at ASC
            ",
        )
        .bind(before)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Schedule proposal database error: {e}")))?;

        Ok(rows.into_iter().map(row_to_proposal).collect())
    }

    async fn mark_expired(&self, id: ScheduleProposalId) -> Result<ScheduleProposal, DomainError> {
        let row = sqlx::query_as::<_, ScheduleProposalRow>(
            r"
            UPDATE schedule_proposals
            SET status = 'expired', updated_at = NOW()
            WHERE id = $1 AND status = 'pending'
            RETURNING *
            ",
        )
        .bind::<uuid::Uuid>(id.into())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Schedule proposal database error: {e}")))?;

        Ok(row_to_proposal(row))
    }
}

/// Convert a database row to a domain entity.
fn row_to_proposal(row: ScheduleProposalRow) -> ScheduleProposal {
    ScheduleProposal {
        id: ScheduleProposalId::from(row.id),
        match_id: TournamentMatchId::from(row.match_id),
        proposed_by_registration_id: TournamentRegistrationId::from(
            row.proposed_by_registration_id,
        ),
        proposed_by_user_id: UserId::from(row.proposed_by_user_id),
        proposed_times: row.proposed_times,
        selected_time: row.selected_time,
        responded_at: row.responded_at,
        responded_by_user_id: row.responded_by_user_id.map(UserId::from),
        counter_proposal_id: row.counter_proposal_id.map(ScheduleProposalId::from),
        status: parse_proposal_status(&row.status),
        expires_at: row.expires_at,
        notes: row.notes,
        rejection_reason: row.rejection_reason,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Parse proposal status from string.
fn parse_proposal_status(s: &str) -> ProposalStatus {
    match s {
        "pending" => ProposalStatus::Pending,
        "accepted" => ProposalStatus::Accepted,
        "rejected" => ProposalStatus::Rejected,
        "counter_proposed" => ProposalStatus::CounterProposed,
        "expired" => ProposalStatus::Expired,
        "cancelled" => ProposalStatus::Cancelled,
        _ => ProposalStatus::Pending,
    }
}
