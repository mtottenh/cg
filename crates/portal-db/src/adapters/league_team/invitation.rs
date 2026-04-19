//! `PostgreSQL` implementation of `LeagueTeamInvitationRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::entities::league_team::{LeagueTeamInvitationRow, LeagueTeamInvitationWithTeamRow};
use crate::DbPool;
use portal_core::types::LeagueTeamInvitationStatus;
use portal_core::{DomainError, LeagueSeasonId, LeagueTeamInvitationId, LeagueTeamSeasonId, PlayerId};
use portal_domain::entities::league_team::{
    LeagueTeamInvitation, LeagueTeamInvitationWithTeam, LeagueTeamMember,
};
use portal_domain::repositories::league_team::{
    AddLeagueTeamMember, CreateLeagueTeamInvitation, LeagueTeamInvitationRepository,
};

use crate::entities::league_team::LeagueTeamMemberRow;

/// `PostgreSQL` implementation of `LeagueTeamInvitationRepository`.
#[derive(Debug, Clone)]
pub struct PgLeagueTeamInvitationRepository {
    pool: DbPool,
}

impl PgLeagueTeamInvitationRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Default invitation expiration (7 days).
    fn default_expiration() -> DateTime<Utc> {
        Utc::now() + chrono::Duration::days(7)
    }
}

#[async_trait]
impl LeagueTeamInvitationRepository for PgLeagueTeamInvitationRepository {
    async fn create(
        &self,
        cmd: CreateLeagueTeamInvitation,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();
        let expires_at = Self::default_expiration();

        let row = sqlx::query_as::<_, LeagueTeamInvitationRow>(
            r"
            INSERT INTO league_team_invitations (
                id, team_season_id, player_id, invitation_type, role, message, invited_by, expires_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.team_season_id.as_uuid())
        .bind(cmd.player_id.as_uuid())
        .bind(cmd.invitation_type.to_string())
        .bind(cmd.role.to_string())
        .bind(&cmd.message)
        .bind(cmd.invited_by.map(|u| u.as_uuid()))
        .bind(expires_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamInvitation::from(row))
    }

    async fn find_by_id(
        &self,
        id: LeagueTeamInvitationId,
    ) -> Result<Option<LeagueTeamInvitation>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamInvitationRow>(
            "SELECT * FROM league_team_invitations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeamInvitation::from))
    }

    async fn find_by_id_with_team(
        &self,
        id: LeagueTeamInvitationId,
    ) -> Result<Option<LeagueTeamInvitationWithTeam>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamInvitationWithTeamRow>(
            r"
            SELECT
                i.id, i.team_season_id, i.player_id, i.invitation_type, i.role, i.message, i.invited_by,
                i.status, i.responded_at, i.expires_at, i.created_at,
                t.id as team_id, t.name as team_name, t.tag as team_tag, t.logo_url as team_logo_url,
                s.id as season_id, s.name as season_name,
                l.id as league_id, l.name as league_name
            FROM league_team_invitations i
            JOIN league_team_seasons ts ON ts.id = i.team_season_id
            JOIN league_teams t ON t.id = ts.team_id
            JOIN league_seasons s ON s.id = ts.season_id
            JOIN leagues l ON l.id = s.league_id
            WHERE i.id = $1
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeamInvitationWithTeam::from))
    }

    async fn find_pending_by_team_season(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamInvitation>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamInvitationRow>(
            "SELECT * FROM league_team_invitations WHERE team_season_id = $1 AND status = 'pending' AND expires_at > NOW() ORDER BY created_at DESC",
        )
        .bind(team_season_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeamInvitation::from).collect())
    }

    async fn find_pending_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamInvitationWithTeamRow>(
            r"
            SELECT
                i.id, i.team_season_id, i.player_id, i.invitation_type, i.role, i.message, i.invited_by,
                i.status, i.responded_at, i.expires_at, i.created_at,
                t.id as team_id, t.name as team_name, t.tag as team_tag, t.logo_url as team_logo_url,
                s.id as season_id, s.name as season_name,
                l.id as league_id, l.name as league_name
            FROM league_team_invitations i
            JOIN league_team_seasons ts ON ts.id = i.team_season_id
            JOIN league_teams t ON t.id = ts.team_id
            JOIN league_seasons s ON s.id = ts.season_id
            JOIN leagues l ON l.id = s.league_id
            WHERE i.player_id = $1 AND i.status = 'pending' AND i.expires_at > NOW()
            ORDER BY i.created_at DESC
            ",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeamInvitationWithTeam::from).collect())
    }

    async fn find_pending_for_player_in_season(
        &self,
        player_id: PlayerId,
        season_id: LeagueSeasonId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamInvitationWithTeamRow>(
            r"
            SELECT
                i.id, i.team_season_id, i.player_id, i.invitation_type, i.role, i.message, i.invited_by,
                i.status, i.responded_at, i.expires_at, i.created_at,
                t.id as team_id, t.name as team_name, t.tag as team_tag, t.logo_url as team_logo_url,
                s.id as season_id, s.name as season_name,
                l.id as league_id, l.name as league_name
            FROM league_team_invitations i
            JOIN league_team_seasons ts ON ts.id = i.team_season_id
            JOIN league_teams t ON t.id = ts.team_id
            JOIN league_seasons s ON s.id = ts.season_id
            JOIN leagues l ON l.id = s.league_id
            WHERE i.player_id = $1 AND s.id = $2 AND i.status = 'pending' AND i.expires_at > NOW()
            ORDER BY i.created_at DESC
            ",
        )
        .bind(player_id.as_uuid())
        .bind(season_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeamInvitationWithTeam::from).collect())
    }

    async fn find_existing_pending(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueTeamInvitation>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamInvitationRow>(
            "SELECT * FROM league_team_invitations WHERE team_season_id = $1 AND player_id = $2 AND status = 'pending' AND expires_at > NOW()",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeamInvitation::from))
    }

    async fn update_status(
        &self,
        id: LeagueTeamInvitationId,
        status: LeagueTeamInvitationStatus,
        response_message: Option<String>,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueTeamInvitationRow>(
            r"
            UPDATE league_team_invitations SET
                status = $2,
                responded_at = $3,
                response_message = $4
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(now)
        .bind(response_message)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamInvitation::from(row))
    }

    async fn accept_and_add_member(
        &self,
        invitation_id: LeagueTeamInvitationId,
        member: AddLeagueTeamMember,
    ) -> Result<LeagueTeamMember, DomainError> {
        // Atomic counterpart of `update_status(Accepted) + add_member`.
        // A partial failure in the old two-call version silently
        // dropped the player from the roster while flipping the
        // invitation to Accepted; retries then failed "already used".
        // See audit I5.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let now = Utc::now();

        sqlx::query(
            r"
            UPDATE league_team_invitations SET
                status = $2,
                responded_at = $3,
                response_message = $4
            WHERE id = $1
            ",
        )
        .bind(invitation_id.as_uuid())
        .bind(LeagueTeamInvitationStatus::Accepted.to_string())
        .bind(now)
        .bind(None::<String>)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let member_id = uuid::Uuid::now_v7();
        let row = sqlx::query_as::<_, LeagueTeamMemberRow>(
            r"
            INSERT INTO league_team_members (
                id, team_season_id, player_id, role, position, jersey_number, added_by, joined_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
        )
        .bind(member_id)
        .bind(member.team_season_id.as_uuid())
        .bind(member.player_id.as_uuid())
        .bind(member.role.to_string())
        .bind(&member.position)
        .bind(member.jersey_number)
        .bind(member.added_by.map(|u| u.as_uuid()))
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamMember::from(row))
    }

    async fn cancel_pending_for_player(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        let now = Utc::now();

        sqlx::query(
            r"
            UPDATE league_team_invitations SET
                status = 'cancelled',
                responded_at = $3
            WHERE team_season_id = $1 AND player_id = $2 AND status = 'pending'
            ",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn count_pending_for_player(&self, player_id: PlayerId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_invitations WHERE player_id = $1 AND status = 'pending' AND expires_at > NOW()",
        )
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn expire_old_invitations(&self) -> Result<i64, DomainError> {
        let result = sqlx::query(
            r"
            UPDATE league_team_invitations SET status = 'expired'
            WHERE status = 'pending' AND expires_at <= NOW()
            ",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected() as i64)
    }
}
