//! `PostgreSQL` implementation of `LeagueTeamMemberRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::entities::league_team::{
    LeagueTeamMemberRow, LeagueTeamMemberWithPlayerRow, PlayerLeagueTeamMembershipRow,
};
use crate::DbPool;
use portal_core::types::{LeagueTeamMemberStatus, LeagueTeamRole};
use portal_core::{DomainError, LeagueSeasonId, LeagueTeamMemberId, LeagueTeamSeasonId, PlayerId};
use portal_domain::entities::league_team::{
    LeagueTeamMember, LeagueTeamMemberWithPlayer, PlayerLeagueTeamMembership,
};
use portal_domain::repositories::league_team::{AddLeagueTeamMember, LeagueTeamMemberRepository};

/// `PostgreSQL` implementation of `LeagueTeamMemberRepository`.
#[derive(Debug, Clone)]
pub struct PgLeagueTeamMemberRepository {
    pool: DbPool,
}

impl PgLeagueTeamMemberRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LeagueTeamMemberRepository for PgLeagueTeamMemberRepository {
    async fn find_by_id(
        &self,
        id: LeagueTeamMemberId,
    ) -> Result<Option<LeagueTeamMember>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamMemberRow>(
            "SELECT * FROM league_team_members WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeamMember::from))
    }

    async fn find_member(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueTeamMember>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamMemberRow>(
            "SELECT * FROM league_team_members WHERE team_season_id = $1 AND player_id = $2 AND left_at IS NULL",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeamMember::from))
    }

    async fn add_member(&self, cmd: AddLeagueTeamMember) -> Result<LeagueTeamMember, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        // Note: season_id is auto-populated by trigger from team_season_id
        let row = sqlx::query_as::<_, LeagueTeamMemberRow>(
            r"
            INSERT INTO league_team_members (
                id, team_season_id, player_id, role, position, jersey_number, added_by, joined_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.team_season_id.as_uuid())
        .bind(cmd.player_id.as_uuid())
        .bind(cmd.role.to_string())
        .bind(&cmd.position)
        .bind(cmd.jersey_number)
        .bind(cmd.added_by.map(|u| u.as_uuid()))
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamMember::from(row))
    }

    async fn update_role(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        new_role: LeagueTeamRole,
    ) -> Result<LeagueTeamMember, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamMemberRow>(
            r"
            UPDATE league_team_members SET role = $3
            WHERE team_season_id = $1 AND player_id = $2 AND left_at IS NULL
            RETURNING *
            ",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .bind(new_role.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamMember::from(row))
    }

    async fn update_status(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        status: LeagueTeamMemberStatus,
    ) -> Result<LeagueTeamMember, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamMemberRow>(
            r"
            UPDATE league_team_members SET status = $3
            WHERE team_season_id = $1 AND player_id = $2 AND left_at IS NULL
            RETURNING *
            ",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeamMember::from(row))
    }

    async fn remove_member(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        let now = Utc::now();

        sqlx::query(
            r"
            UPDATE league_team_members SET
                left_at = $3,
                status = 'left'
            WHERE team_season_id = $1 AND player_id = $2 AND left_at IS NULL
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

    async fn list_members(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamMember>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamMemberRow>(
            "SELECT * FROM league_team_members WHERE team_season_id = $1 AND left_at IS NULL ORDER BY joined_at ASC",
        )
        .bind(team_season_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeamMember::from).collect())
    }

    async fn list_members_with_players(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamMemberWithPlayer>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamMemberWithPlayerRow>(
            r"
            SELECT
                m.id, m.team_season_id, m.player_id, m.role, m.position, m.jersey_number,
                m.status, m.joined_at, m.left_at,
                p.display_name, p.avatar_url
            FROM league_team_members m
            JOIN players p ON p.id = m.player_id
            WHERE m.team_season_id = $1 AND m.left_at IS NULL
            ORDER BY m.joined_at ASC
            ",
        )
        .bind(team_season_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeamMemberWithPlayer::from).collect())
    }

    async fn count_by_role(
        &self,
        team_season_id: LeagueTeamSeasonId,
        role: LeagueTeamRole,
    ) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_members WHERE team_season_id = $1 AND role = $2 AND left_at IS NULL",
        )
        .bind(team_season_id.as_uuid())
        .bind(role.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn count_captains(&self, team_season_id: LeagueTeamSeasonId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_members WHERE team_season_id = $1 AND role = 'captain' AND left_at IS NULL",
        )
        .bind(team_season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn count_active_members(&self, team_season_id: LeagueTeamSeasonId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_members WHERE team_season_id = $1 AND left_at IS NULL AND status = 'active'",
        )
        .bind(team_season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn count_primary_members(&self, team_season_id: LeagueTeamSeasonId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_members WHERE team_season_id = $1 AND role IN ('captain', 'player') AND left_at IS NULL",
        )
        .bind(team_season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn count_substitutes(&self, team_season_id: LeagueTeamSeasonId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_members WHERE team_season_id = $1 AND role = 'substitute' AND left_at IS NULL",
        )
        .bind(team_season_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn is_member(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_members WHERE team_season_id = $1 AND player_id = $2 AND left_at IS NULL",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn is_captain(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_team_members WHERE team_season_id = $1 AND player_id = $2 AND role = 'captain' AND left_at IS NULL",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn find_primary_team_in_season(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueTeamSeasonId>, DomainError> {
        let result: Option<(uuid::Uuid,)> = sqlx::query_as(
            r"
            SELECT m.team_season_id
            FROM league_team_members m
            WHERE m.season_id = $1
              AND m.player_id = $2
              AND m.role IN ('captain', 'player')
              AND m.left_at IS NULL
            LIMIT 1
            ",
        )
        .bind(season_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.map(|(id,)| LeagueTeamSeasonId::from_uuid(id)))
    }

    async fn list_memberships_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerLeagueTeamMembership>, DomainError> {
        let rows = sqlx::query_as::<_, PlayerLeagueTeamMembershipRow>(
            "SELECT * FROM v_player_league_teams WHERE player_id = $1 ORDER BY joined_at DESC",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(PlayerLeagueTeamMembership::from).collect())
    }

    async fn list_memberships_in_season(
        &self,
        player_id: PlayerId,
        season_id: LeagueSeasonId,
    ) -> Result<Vec<PlayerLeagueTeamMembership>, DomainError> {
        let rows = sqlx::query_as::<_, PlayerLeagueTeamMembershipRow>(
            "SELECT * FROM v_player_league_teams WHERE player_id = $1 AND season_id = $2 ORDER BY joined_at DESC",
        )
        .bind(player_id.as_uuid())
        .bind(season_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(PlayerLeagueTeamMembership::from).collect())
    }
}
