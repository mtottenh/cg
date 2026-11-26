//! `PostgreSQL` implementation of `MatchRescueRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::DbPool;
use crate::entities::tournament::TournamentMatchRow;
use crate::transaction::DbTransaction;
use portal_core::types::TournamentMatchStatus;
use portal_core::{
    DisputeId, DomainError, ResultClaimId, ScheduleProposalId, TournamentMatchId,
    TournamentRegistrationId,
};
use portal_domain::entities::tournament::TournamentMatch;
use portal_domain::repositories::match_rescue::{
    ActiveForfeitInfo, CompletionSagaInfo, MatchRescueRepository, MatchRescueSnapshot,
    MatchResetSummary, PendingClaimInfo, PendingProposalInfo, RescueAudit, ServerSeriesResultInfo,
};

/// `PostgreSQL` implementation of `MatchRescueRepository`.
#[derive(Debug, Clone)]
pub struct PgMatchRescueRepository {
    pool: DbPool,
}

impl PgMatchRescueRepository {
    /// Create a new repository instance.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

fn internal(e: sqlx::Error) -> DomainError {
    DomainError::Internal(e.to_string())
}

fn parse_status(raw: &str) -> Result<TournamentMatchStatus, DomainError> {
    raw.parse()
        .map_err(|_| DomainError::Internal(format!("unknown match status in database: {raw}")))
}

/// Lock the match row and return its current status.
async fn lock_status(
    tx: &mut DbTransaction<'_>,
    id: TournamentMatchId,
) -> Result<TournamentMatchStatus, DomainError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT status FROM tournament_matches WHERE id = $1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_optional(&mut **tx)
            .await
            .map_err(internal)?;
    let raw = raw.ok_or(DomainError::TournamentMatchNotFound(id))?;
    parse_status(&raw)
}

/// Append the admin's action to the match's status log. `triggered_by_system`
/// is set alongside the user id: that pair is how the log marks an admin
/// override (see `MatchStatusLog::is_admin_override`).
async fn log_in_tx(
    tx: &mut DbTransaction<'_>,
    id: TournamentMatchId,
    from: TournamentMatchStatus,
    to: TournamentMatchStatus,
    action: &str,
    audit: &RescueAudit,
) -> Result<(), DomainError> {
    sqlx::query(
        r"
        INSERT INTO match_status_log (
            match_id, from_status, to_status, transition_reason,
            triggered_by_user_id, triggered_by_system, metadata
        ) VALUES ($1, $2, $3, $4, $5, true, $6)
        ",
    )
    .bind(id.as_uuid())
    .bind(from.to_string())
    .bind(to.to_string())
    .bind(truncate(&format!("Admin {action}: {}", audit.reason), 256))
    .bind(audit.admin_id.as_uuid())
    .bind(serde_json::json!({
        "admin_action": action,
        "override_reason": audit.reason,
    }))
    .execute(&mut **tx)
    .await
    .map_err(internal)?;
    Ok(())
}

/// `transition_reason` is `VARCHAR(256)`.
fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Delete the veto session of a match and the per-map game rows that were
/// built from its result. Returns whether a session existed.
async fn discard_veto_in_tx(
    tx: &mut DbTransaction<'_>,
    id: TournamentMatchId,
) -> Result<bool, DomainError> {
    let deleted = sqlx::query("DELETE FROM veto_sessions WHERE match_id = $1")
        .bind(id.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(internal)?
        .rows_affected();
    sqlx::query("DELETE FROM tournament_match_games WHERE match_id = $1")
        .bind(id.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(internal)?;
    Ok(deleted > 0)
}

#[async_trait]
impl MatchRescueRepository for PgMatchRescueRepository {
    async fn reset_to_ready(
        &self,
        id: TournamentMatchId,
        allowed_from: &[TournamentMatchStatus],
        logged_from: TournamentMatchStatus,
        audit: &RescueAudit,
    ) -> Result<MatchResetSummary, DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;

        let current = lock_status(&mut tx, id).await?;
        if !allowed_from.contains(&current) {
            return Err(DomainError::Conflict(format!(
                "Match is now {current}; it changed while the reset was running"
            )));
        }

        // A forfeit that decided this match no longer does.
        sqlx::query(
            r"
            UPDATE forfeit_records SET superseded_at = NOW()
            WHERE match_id = $1 AND superseded_at IS NULL
            ",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(internal)?;

        // Unfinished completion sagas would re-record the result this reset
        // takes back; a forfeit's finished saga would make settlement skip
        // the next result (same reasoning as `clear_result`).
        sqlx::query(
            r"
            UPDATE saga_executions SET status = 'compensated', updated_at = NOW()
            WHERE match_id = $1
              AND saga_type = 'match_completion'
              AND (
                  status IN ('failed', 'pending', 'running', 'paused')
                  OR (status = 'completed' AND input_data->>'is_forfeit' = 'true')
              )
            ",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(internal)?;

        let superseded_claims = sqlx::query(
            r"
            UPDATE result_claims SET status = 'superseded', updated_at = NOW()
            WHERE match_id = $1 AND status IN ('pending', 'disputed')
            ",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(internal)?
        .rows_affected();

        let cancelled_disputes = sqlx::query(
            r"
            UPDATE disputes SET status = 'cancelled', updated_at = NOW()
            WHERE match_id = $1 AND status IN ('pending', 'under_review')
            ",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(internal)?
        .rows_affected();

        let cancelled_proposals = sqlx::query(
            r"
            UPDATE schedule_proposals
            SET status = 'cancelled', responded_at = NOW(), responded_by_user_id = $2,
                updated_at = NOW()
            WHERE match_id = $1 AND status = 'pending'
            ",
        )
        .bind(id.as_uuid())
        .bind(audit.admin_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(internal)?
        .rows_affected();

        let veto_discarded = discard_veto_in_tx(&mut tx, id).await?;

        // Declared lineups stay; only the lock goes, so captains can change
        // them again before the replay.
        sqlx::query(
            r"
            UPDATE match_lineups
            SET status = CASE WHEN declared_at IS NULL THEN 'draft' ELSE 'submitted' END,
                locked_at = NULL,
                updated_at = NOW()
            WHERE match_id = $1 AND status = 'locked'
            ",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(internal)?;

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                status = 'ready',
                scheduled_at = NULL,
                started_at = NULL,
                completed_at = NULL,
                check_in_opens_at = NULL,
                check_in_deadline = NULL,
                participant1_checked_in_at = NULL,
                participant2_checked_in_at = NULL,
                participant1_checked_in_by = NULL,
                participant2_checked_in_by = NULL,
                participant1_score = 0,
                participant2_score = 0,
                winner_registration_id = NULL,
                loser_registration_id = NULL,
                disputed = false,
                dispute_reason = NULL,
                veto_required = false,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;

        log_in_tx(
            &mut tx,
            id,
            logged_from,
            TournamentMatchStatus::Ready,
            "reset",
            audit,
        )
        .await?;

        tx.commit().await.map_err(internal)?;

        Ok(MatchResetSummary {
            match_: TournamentMatch::from(row),
            superseded_claims,
            cancelled_disputes,
            cancelled_proposals,
            veto_discarded,
        })
    }

    async fn set_check_in_window(
        &self,
        id: TournamentMatchId,
        deadline: DateTime<Utc>,
        audit: &RescueAudit,
    ) -> Result<(TournamentMatch, TournamentMatchStatus), DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;

        let from = lock_status(&mut tx, id).await?;
        if !matches!(
            from,
            TournamentMatchStatus::Ready
                | TournamentMatchStatus::Scheduled
                | TournamentMatchStatus::CheckingIn
        ) {
            return Err(DomainError::Conflict(format!(
                "Match is now {from}; the check-in window can only be set while it is ready, \
                 scheduled or checking in"
            )));
        }

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                status = 'checking_in',
                scheduled_at = COALESCE(scheduled_at, NOW()),
                check_in_opens_at = COALESCE(check_in_opens_at, NOW()),
                check_in_deadline = $2,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(deadline)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;

        log_in_tx(
            &mut tx,
            id,
            from,
            TournamentMatchStatus::CheckingIn,
            "check-in window",
            audit,
        )
        .await?;

        tx.commit().await.map_err(internal)?;
        Ok((TournamentMatch::from(row), from))
    }

    async fn discard_veto_for_restart(
        &self,
        id: TournamentMatchId,
        audit: &RescueAudit,
    ) -> Result<(TournamentMatch, TournamentMatchStatus), DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;

        let from = lock_status(&mut tx, id).await?;
        if !matches!(
            from,
            TournamentMatchStatus::CheckingIn
                | TournamentMatchStatus::PickBan
                | TournamentMatchStatus::InProgress
        ) {
            return Err(DomainError::Conflict(format!(
                "Match is now {from}; the veto can only be restarted while it is checking in, \
                 in pick/ban or in progress"
            )));
        }

        discard_veto_in_tx(&mut tx, id).await?;

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                status = 'pick_ban',
                veto_required = false,
                started_at = NULL,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;

        log_in_tx(
            &mut tx,
            id,
            from,
            TournamentMatchStatus::PickBan,
            "veto restart",
            audit,
        )
        .await?;

        tx.commit().await.map_err(internal)?;
        Ok((TournamentMatch::from(row), from))
    }

    async fn has_live_result_claim(&self, id: TournamentMatchId) -> Result<bool, DomainError> {
        sqlx::query_scalar(
            r"
            SELECT EXISTS (
                SELECT 1 FROM result_claims
                WHERE match_id = $1 AND status IN ('pending', 'confirmed', 'disputed')
            )
            ",
        )
        .bind(id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(internal)
    }

    async fn log_admin_action(
        &self,
        id: TournamentMatchId,
        status: TournamentMatchStatus,
        action: &str,
        audit: &RescueAudit,
    ) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;
        log_in_tx(&mut tx, id, status, status, action, audit).await?;
        tx.commit().await.map_err(internal)
    }

    async fn snapshot(&self, id: TournamentMatchId) -> Result<MatchRescueSnapshot, DomainError> {
        let match_uuid = id.as_uuid();

        let active_forfeit = sqlx::query(
            r"
            SELECT forfeit_type, reason, triggered_by_system
            FROM forfeit_records
            WHERE match_id = $1 AND superseded_at IS NULL
            ORDER BY forfeited_at DESC, id DESC
            LIMIT 1
            ",
        )
        .bind(match_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .map(|row| ActiveForfeitInfo {
            forfeit_type: row.get("forfeit_type"),
            reason: row.get("reason"),
            triggered_by_system: row.get("triggered_by_system"),
        });

        let pending_proposal = sqlx::query(
            r"
            SELECT id, proposed_by_registration_id, expires_at
            FROM schedule_proposals
            WHERE match_id = $1 AND status = 'pending'
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            ",
        )
        .bind(match_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .map(|row| PendingProposalInfo {
            id: ScheduleProposalId::from_uuid(row.get::<Uuid, _>("id")),
            proposed_by_registration_id: TournamentRegistrationId::from_uuid(
                row.get::<Uuid, _>("proposed_by_registration_id"),
            ),
            expires_at: row.get("expires_at"),
        });

        let pending_claim = sqlx::query(
            r"
            SELECT id, source, auto_confirm_at
            FROM result_claims
            WHERE match_id = $1 AND status = 'pending'
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            ",
        )
        .bind(match_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .map(|row| PendingClaimInfo {
            id: ResultClaimId::from_uuid(row.get::<Uuid, _>("id")),
            source: row.get("source"),
            auto_confirm_at: row.get("auto_confirm_at"),
        });

        let open_dispute_id: Option<Uuid> = sqlx::query_scalar(
            r"
            SELECT id FROM disputes
            WHERE match_id = $1 AND status IN ('pending', 'under_review')
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            ",
        )
        .bind(match_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;

        let saga = sqlx::query(
            r"
            SELECT status,
                   CASE WHEN status = 'completed' THEN NULL
                        ELSE step_history->-1->>'name' END AS failed_step,
                   updated_at
            FROM saga_executions
            WHERE match_id = $1 AND saga_type = 'match_completion'
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            ",
        )
        .bind(match_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .map(|row| CompletionSagaInfo {
            status: row.get("status"),
            failed_step: row.get("failed_step"),
            updated_at: row.get("updated_at"),
        });

        let server_series_result = sqlx::query(
            r"
            SELECT COALESCE((e.payload ->> 'team1_series_score')::int, 0) AS p1,
                   COALESCE((e.payload ->> 'team2_series_score')::int, 0) AS p2,
                   e.received_at,
                   EXISTS (SELECT 1 FROM result_claims c
                           WHERE c.match_id = $1 AND c.source = 'server') AS recorded
            FROM server_events e
            JOIN server_reservations r ON r.id = e.reservation_id
            WHERE r.match_id = $1 AND e.event_type = 'series_end'
            ORDER BY e.received_at DESC
            LIMIT 1
            ",
        )
        .bind(match_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .map(|row| ServerSeriesResultInfo {
            participant1_score: row.get("p1"),
            participant2_score: row.get("p2"),
            recorded_as_claim: row.get("recorded"),
            received_at: row.get("received_at"),
        });

        Ok(MatchRescueSnapshot {
            active_forfeit,
            pending_proposal,
            pending_claim,
            open_dispute_id: open_dispute_id.map(DisputeId::from_uuid),
            saga,
            server_series_result,
        })
    }
}
