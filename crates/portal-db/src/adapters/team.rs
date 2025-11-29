//! Team repository adapters.

use crate::entities::{PlayerTeamMembershipRow, TeamInvitationRow, TeamMemberRow, TeamRow};
use crate::DbPool;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use portal_core::types::{InvitationStatus, TeamRole};
use portal_core::{DomainError, PlayerId, TeamId, TeamInvitationId};
use portal_domain::entities::team::{PlayerTeamMembership, Team, TeamInvitation, TeamMember};
use portal_domain::repositories::team::{
    AddMember, CreateInvitation, CreateTeam, TeamInvitationRepository, TeamMemberRepository,
    TeamRepository, UpdateTeam,
};
use sqlx::Row;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<TeamRow> for Team {
    fn from(row: TeamRow) -> Self {
        Self {
            id: TeamId::from(row.id),
            name: row.name,
            tag: row.tag,
            description: row.description,
            logo_url: row.logo_url,
            banner_url: row.banner_url,
            primary_color: row.primary_color,
            secondary_color: row.secondary_color,
            created_by: PlayerId::from(row.created_by),
            game_id: row.game_id,
            status: row.status.parse().unwrap_or_default(),
            disbanded_at: row.disbanded_at,
            disbanded_reason: row.disbanded_reason,
            total_matches: row.total_matches,
            total_wins: row.total_wins,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<TeamMemberRow> for TeamMember {
    fn from(row: TeamMemberRow) -> Self {
        Self {
            team_id: TeamId::from(row.team_id),
            player_id: PlayerId::from(row.player_id),
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            role: row.role.parse().unwrap_or_default(),
            role_title: row.role_title,
            is_founder: row.is_founder,
            primary_position: row.primary_position,
            secondary_position: row.secondary_position,
            status: row.status.parse().unwrap_or_default(),
            jersey_number: row.jersey_number,
            invited_by: row.invited_by.map(PlayerId::from),
            joined_at: row.joined_at,
            left_at: row.left_at,
        }
    }
}

impl From<TeamInvitationRow> for TeamInvitation {
    fn from(row: TeamInvitationRow) -> Self {
        Self {
            id: TeamInvitationId::from(row.id),
            team_id: TeamId::from(row.team_id),
            player_id: PlayerId::from(row.player_id),
            invitation_type: row.invitation_type.parse().unwrap_or_default(),
            role: row.role.parse().unwrap_or_default(),
            message: row.message,
            invited_by: row.invited_by.map(PlayerId::from),
            status: row.status.parse().unwrap_or_default(),
            responded_at: row.responded_at,
            response_message: row.response_message,
            expires_at: row.expires_at,
            created_at: row.created_at,
        }
    }
}

impl From<PlayerTeamMembershipRow> for PlayerTeamMembership {
    fn from(row: PlayerTeamMembershipRow) -> Self {
        Self {
            team_id: TeamId::from(row.team_id),
            team_name: row.team_name,
            team_tag: row.team_tag,
            team_logo_url: row.team_logo_url,
            role: row.role.parse().unwrap_or_default(),
            joined_at: row.joined_at,
        }
    }
}

// =============================================================================
// Team Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain TeamRepository trait.
#[derive(Clone)]
pub struct PgTeamRepository {
    pool: DbPool,
}

impl PgTeamRepository {
    /// Create a new PostgreSQL team repository.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TeamRepository for PgTeamRepository {
    async fn find_by_id(&self, id: TeamId) -> Result<Option<Team>, DomainError> {
        let team = sqlx::query_as::<_, TeamRow>("SELECT * FROM teams WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(team.map(Team::from))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Team>, DomainError> {
        let team =
            sqlx::query_as::<_, TeamRow>("SELECT * FROM teams WHERE name_normalized = lower($1)")
                .bind(name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(team.map(Team::from))
    }

    async fn find_by_tag(&self, tag: &str) -> Result<Option<Team>, DomainError> {
        let team =
            sqlx::query_as::<_, TeamRow>("SELECT * FROM teams WHERE tag_normalized = lower($1)")
                .bind(tag)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(team.map(Team::from))
    }

    async fn create(&self, cmd: CreateTeam) -> Result<Team, DomainError> {
        let team = sqlx::query_as::<_, TeamRow>(
            r#"
            INSERT INTO teams (name, tag, created_by, description, logo_url, game_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(&cmd.name)
        .bind(&cmd.tag)
        .bind(cmd.created_by.as_uuid())
        .bind(&cmd.description)
        .bind(&cmd.logo_url)
        .bind(&cmd.game_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Team::from(team))
    }

    async fn update(&self, id: TeamId, update: UpdateTeam) -> Result<Team, DomainError> {
        let team = sqlx::query_as::<_, TeamRow>(
            r#"
            UPDATE teams SET
                name = COALESCE($2, name),
                tag = COALESCE($3, tag),
                description = COALESCE($4, description),
                logo_url = COALESCE($5, logo_url),
                banner_url = COALESCE($6, banner_url),
                primary_color = COALESCE($7, primary_color),
                secondary_color = COALESCE($8, secondary_color),
                website_url = COALESCE($9, website_url),
                status = COALESCE($10, status),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.tag)
        .bind(&update.description)
        .bind(&update.logo_url)
        .bind(&update.banner_url)
        .bind(&update.primary_color)
        .bind(&update.secondary_color)
        .bind(&update.website_url)
        .bind(&update.status)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::TeamNotFound(id.to_string()))?;

        Ok(Team::from(team))
    }

    async fn list_by_player(&self, player_id: PlayerId) -> Result<Vec<Team>, DomainError> {
        let teams = sqlx::query_as::<_, TeamRow>(
            r#"
            SELECT t.* FROM teams t
            INNER JOIN team_members tm ON tm.team_id = t.id
            WHERE tm.player_id = $1 AND tm.left_at IS NULL
            ORDER BY t.name
            "#,
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(teams.into_iter().map(Team::from).collect())
    }

    async fn list(
        &self,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Team>, i64), DomainError> {
        let (teams, total) = match search.as_deref() {
            Some(query) if !query.is_empty() => {
                let pattern = format!("%{}%", query.to_lowercase());
                let teams = sqlx::query_as::<_, TeamRow>(
                    r#"
                    SELECT * FROM teams
                    WHERE status = 'active'
                      AND (name_normalized LIKE $1 OR tag_normalized LIKE $1)
                    ORDER BY name
                    LIMIT $2 OFFSET $3
                    "#,
                )
                .bind(&pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

                let count_row = sqlx::query(
                    r#"
                    SELECT COUNT(*) as count FROM teams
                    WHERE status = 'active'
                      AND (name_normalized LIKE $1 OR tag_normalized LIKE $1)
                    "#,
                )
                .bind(&pattern)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

                let total: i64 = count_row.get("count");
                (teams, total)
            }
            _ => {
                let teams = sqlx::query_as::<_, TeamRow>(
                    r#"
                    SELECT * FROM teams
                    WHERE status = 'active'
                    ORDER BY name
                    LIMIT $1 OFFSET $2
                    "#,
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

                let count_row = sqlx::query(
                    "SELECT COUNT(*) as count FROM teams WHERE status = 'active'",
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

                let total: i64 = count_row.get("count");
                (teams, total)
            }
        };

        Ok((teams.into_iter().map(Team::from).collect(), total))
    }

    async fn name_exists(&self, name: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM teams WHERE name_normalized = lower($1)")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn tag_exists(&self, tag: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM teams WHERE tag_normalized = lower($1)")
            .bind(tag)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }
}

// =============================================================================
// Team Member Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain TeamMemberRepository trait.
#[derive(Clone)]
pub struct PgTeamMemberRepository {
    pool: DbPool,
}

impl PgTeamMemberRepository {
    /// Create a new PostgreSQL team member repository.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TeamMemberRepository for PgTeamMemberRepository {
    async fn find_member(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<Option<TeamMember>, DomainError> {
        let member = sqlx::query_as::<_, TeamMemberRow>(
            r#"
            SELECT
                tm.id, tm.team_id, tm.player_id,
                p.display_name, p.avatar_url,
                tm.role, tm.role_title, tm.is_founder,
                tm.primary_position, tm.secondary_position,
                tm.status, tm.jersey_number, tm.invited_by,
                tm.joined_at, tm.left_at
            FROM team_members tm
            JOIN players p ON p.id = tm.player_id
            WHERE tm.team_id = $1 AND tm.player_id = $2 AND tm.left_at IS NULL
            "#,
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(member.map(TeamMember::from))
    }

    async fn list_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, DomainError> {
        let members = sqlx::query_as::<_, TeamMemberRow>(
            r#"
            SELECT
                tm.id,
                tm.team_id,
                tm.player_id,
                p.display_name,
                p.avatar_url,
                tm.role,
                tm.role_title,
                tm.is_founder,
                tm.primary_position,
                tm.secondary_position,
                tm.status,
                tm.jersey_number,
                tm.invited_by,
                tm.joined_at,
                tm.left_at
            FROM team_members tm
            INNER JOIN players p ON p.id = tm.player_id
            WHERE tm.team_id = $1 AND tm.left_at IS NULL
            ORDER BY
                CASE tm.role
                    WHEN 'captain' THEN 1
                    WHEN 'officer' THEN 2
                    WHEN 'player' THEN 3
                    WHEN 'substitute' THEN 4
                    WHEN 'coach' THEN 5
                    WHEN 'manager' THEN 6
                END,
                tm.joined_at
            "#,
        )
        .bind(team_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(members.into_iter().map(TeamMember::from).collect())
    }

    async fn count_captains(&self, team_id: TeamId) -> Result<i64, DomainError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM team_members WHERE team_id = $1 AND role = 'captain' AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.get("count"))
    }

    async fn count_members(&self, team_id: TeamId) -> Result<i64, DomainError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM team_members WHERE team_id = $1 AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.get("count"))
    }

    async fn add_member(&self, member: AddMember) -> Result<TeamMember, DomainError> {
        let row = sqlx::query_as::<_, TeamMemberRow>(
            r#"
            WITH inserted AS (
                INSERT INTO team_members (team_id, player_id, role, is_founder, invited_by)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING *
            )
            SELECT
                i.id, i.team_id, i.player_id,
                p.display_name, p.avatar_url,
                i.role, i.role_title, i.is_founder,
                i.primary_position, i.secondary_position,
                i.status, i.jersey_number, i.invited_by,
                i.joined_at, i.left_at
            FROM inserted i
            JOIN players p ON p.id = i.player_id
            "#,
        )
        .bind(member.team_id.as_uuid())
        .bind(member.player_id.as_uuid())
        .bind(member.role.to_string())
        .bind(member.is_founder)
        .bind(member.invited_by.map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TeamMember::from(row))
    }

    async fn update_role(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
        new_role: TeamRole,
    ) -> Result<TeamMember, DomainError> {
        let row = sqlx::query_as::<_, TeamMemberRow>(
            r#"
            WITH updated AS (
                UPDATE team_members SET
                    role = $3,
                    updated_at = NOW()
                WHERE team_id = $1 AND player_id = $2 AND left_at IS NULL
                RETURNING *
            )
            SELECT
                u.id, u.team_id, u.player_id,
                p.display_name, p.avatar_url,
                u.role, u.role_title, u.is_founder,
                u.primary_position, u.secondary_position,
                u.status, u.jersey_number, u.invited_by,
                u.joined_at, u.left_at
            FROM updated u
            JOIN players p ON p.id = u.player_id
            "#,
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .bind(new_role.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::NotTeamMember)?;

        Ok(TeamMember::from(row))
    }

    async fn remove_member(&self, team_id: TeamId, player_id: PlayerId) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE team_members SET left_at = NOW() WHERE team_id = $1 AND player_id = $2 AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotTeamMember);
        }

        Ok(())
    }

    async fn is_member(&self, team_id: TeamId, player_id: PlayerId) -> Result<bool, DomainError> {
        let row = sqlx::query(
            "SELECT 1 FROM team_members WHERE team_id = $1 AND player_id = $2 AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn is_captain(&self, team_id: TeamId, player_id: PlayerId) -> Result<bool, DomainError> {
        let row = sqlx::query(
            "SELECT 1 FROM team_members WHERE team_id = $1 AND player_id = $2 AND role = 'captain' AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn list_memberships_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerTeamMembership>, DomainError> {
        let memberships = sqlx::query_as::<_, PlayerTeamMembershipRow>(
            r#"
            SELECT
                t.id as team_id,
                t.name as team_name,
                t.tag as team_tag,
                t.logo_url as team_logo_url,
                tm.role,
                tm.joined_at
            FROM team_members tm
            INNER JOIN teams t ON t.id = tm.team_id
            WHERE tm.player_id = $1 AND tm.left_at IS NULL AND t.status = 'active'
            ORDER BY tm.joined_at DESC
            "#,
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(memberships
            .into_iter()
            .map(PlayerTeamMembership::from)
            .collect())
    }
}

// =============================================================================
// Team Invitation Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain TeamInvitationRepository trait.
#[derive(Clone)]
pub struct PgTeamInvitationRepository {
    pool: DbPool,
}

impl PgTeamInvitationRepository {
    /// Create a new PostgreSQL team invitation repository.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TeamInvitationRepository for PgTeamInvitationRepository {
    async fn create(&self, invitation: CreateInvitation) -> Result<TeamInvitation, DomainError> {
        // Default expiration is 7 days
        let expires_at = Utc::now() + Duration::days(7);

        let row = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            INSERT INTO team_invitations (team_id, player_id, type, role, message, invited_by, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(invitation.team_id.as_uuid())
        .bind(invitation.player_id.as_uuid())
        .bind(&invitation.invitation_type)
        .bind(invitation.role.to_string())
        .bind(&invitation.message)
        .bind(invitation.invited_by.map(|id| id.as_uuid()))
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TeamInvitation::from(row))
    }

    async fn find_by_id(
        &self,
        id: TeamInvitationId,
    ) -> Result<Option<TeamInvitation>, DomainError> {
        let row = sqlx::query_as::<_, TeamInvitationRow>(
            "SELECT * FROM team_invitations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TeamInvitation::from))
    }

    async fn find_pending_by_team(
        &self,
        team_id: TeamId,
    ) -> Result<Vec<TeamInvitation>, DomainError> {
        let rows = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            SELECT * FROM team_invitations
            WHERE team_id = $1 AND status = 'pending' AND expires_at > NOW()
            ORDER BY created_at DESC
            "#,
        )
        .bind(team_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TeamInvitation::from).collect())
    }

    async fn find_pending_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<TeamInvitation>, DomainError> {
        let rows = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            SELECT * FROM team_invitations
            WHERE player_id = $1 AND status = 'pending' AND expires_at > NOW()
            ORDER BY created_at DESC
            "#,
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TeamInvitation::from).collect())
    }

    async fn find_existing_pending(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<Option<TeamInvitation>, DomainError> {
        let row = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            SELECT * FROM team_invitations
            WHERE team_id = $1 AND player_id = $2 AND status = 'pending' AND expires_at > NOW()
            LIMIT 1
            "#,
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TeamInvitation::from))
    }

    async fn update_status(
        &self,
        id: TeamInvitationId,
        status: InvitationStatus,
        response_message: Option<String>,
    ) -> Result<TeamInvitation, DomainError> {
        let row = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            UPDATE team_invitations SET
                status = $2,
                responded_at = NOW(),
                response_message = $3
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(&response_message)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::InvitationInvalid)?;

        Ok(TeamInvitation::from(row))
    }

    async fn cancel_pending_for_player(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE team_invitations SET
                status = 'cancelled',
                responded_at = NOW()
            WHERE team_id = $1 AND player_id = $2 AND status = 'pending'
            "#,
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn count_pending_for_player(&self, player_id: PlayerId) -> Result<i64, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM team_invitations
            WHERE player_id = $1 AND status = 'pending' AND expires_at > NOW()
            "#,
        )
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.get("count"))
    }
}
