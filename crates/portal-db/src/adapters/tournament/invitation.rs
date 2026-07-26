//! `PostgreSQL` implementation of `TournamentInvitationRepository`.
//!
//! Backs `registration_type = 'invite_only'` (audit P-27). Before this
//! existed the setting had no storage and no enforcement.

use async_trait::async_trait;

use crate::DbPool;
use crate::entities::tournament::TournamentInvitationRow;
use portal_core::{DomainError, LeagueTeamSeasonId, TournamentId, TournamentInvitationId, UserId};
use portal_domain::entities::tournament::TournamentInvitation;
use portal_domain::repositories::tournament::{
    CreateTournamentInvitation, TournamentInvitationRepository,
};

/// `PostgreSQL` implementation of `TournamentInvitationRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentInvitationRepository {
    pool: DbPool,
}

impl PgTournamentInvitationRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentInvitationRepository for PgTournamentInvitationRepository {
    async fn find_by_id(
        &self,
        id: TournamentInvitationId,
    ) -> Result<Option<TournamentInvitation>, DomainError> {
        let row = sqlx::query_as::<_, TournamentInvitationRow>(
            "SELECT * FROM tournament_invitations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentInvitation::from))
    }

    async fn find_for_user(
        &self,
        tournament_id: TournamentId,
        user_id: UserId,
    ) -> Result<Option<TournamentInvitation>, DomainError> {
        let row = sqlx::query_as::<_, TournamentInvitationRow>(
            r"
            SELECT * FROM tournament_invitations
            WHERE tournament_id = $1 AND user_id = $2 AND status <> 'revoked'
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentInvitation::from))
    }

    async fn find_for_team_season(
        &self,
        tournament_id: TournamentId,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Option<TournamentInvitation>, DomainError> {
        let row = sqlx::query_as::<_, TournamentInvitationRow>(
            r"
            SELECT * FROM tournament_invitations
            WHERE tournament_id = $1 AND team_season_id = $2 AND status <> 'revoked'
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(team_season_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentInvitation::from))
    }

    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentInvitation>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentInvitationRow>(
            r"
            SELECT * FROM tournament_invitations
            WHERE tournament_id = $1
            ORDER BY created_at DESC, id DESC
            ",
        )
        .bind(tournament_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentInvitation::from).collect())
    }

    async fn create(
        &self,
        invitation: CreateTournamentInvitation,
    ) -> Result<TournamentInvitation, DomainError> {
        let row = sqlx::query_as::<_, TournamentInvitationRow>(
            r"
            INSERT INTO tournament_invitations
                (tournament_id, user_id, team_season_id, message, invited_by, status)
            VALUES ($1, $2, $3, $4, $5, 'pending')
            RETURNING *
            ",
        )
        .bind(invitation.tournament_id.as_uuid())
        .bind(invitation.user_id.map(|id| id.as_uuid()))
        .bind(invitation.team_season_id.map(|id| id.as_uuid()))
        .bind(invitation.message)
        .bind(invitation.invited_by.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match &e {
            // Partial unique index on (tournament, target) where the
            // invitation is not revoked — re-inviting someone who already
            // has a live invitation is a conflict, not a 500.
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                DomainError::Conflict("Target already has an invitation".to_string())
            }
            _ => DomainError::Internal(e.to_string()),
        })?;

        Ok(TournamentInvitation::from(row))
    }

    async fn mark_accepted(
        &self,
        id: TournamentInvitationId,
    ) -> Result<TournamentInvitation, DomainError> {
        let row = sqlx::query_as::<_, TournamentInvitationRow>(
            r"
            UPDATE tournament_invitations
            SET status = 'accepted', accepted_at = COALESCE(accepted_at, NOW())
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::TournamentInvitationNotFound(id))?;

        Ok(TournamentInvitation::from(row))
    }

    async fn revoke(
        &self,
        id: TournamentInvitationId,
    ) -> Result<TournamentInvitation, DomainError> {
        let row = sqlx::query_as::<_, TournamentInvitationRow>(
            r"
            UPDATE tournament_invitations
            SET status = 'revoked', revoked_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::TournamentInvitationNotFound(id))?;

        Ok(TournamentInvitation::from(row))
    }
}
