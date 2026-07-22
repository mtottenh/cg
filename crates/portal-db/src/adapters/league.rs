//! League repository adapters.

use crate::DbPool;
use crate::entities::{
    LeagueInvitationRow, LeagueMemberRow, LeagueMemberWithUserRow, LeagueRow,
    UserLeagueMembershipRow,
};
use async_trait::async_trait;
use portal_core::{DomainError, GameId, LeagueId, LeagueInvitationId, LeagueMemberId, UserId};
use portal_domain::entities::league::{
    League, LeagueAccessType, LeagueInvitation, LeagueInvitationStatus, LeagueInvitationType,
    LeagueMember, LeagueMemberWithUser, LeagueMembershipType, LeagueStatus, UserLeagueMembership,
};
use portal_domain::repositories::league::{
    AddLeagueMember, CreateLeague, CreateLeagueInvitation, LeagueInvitationRepository,
    LeagueMemberRepository, LeagueRepository, UpdateLeague,
};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<LeagueRow> for League {
    fn from(row: LeagueRow) -> Self {
        Self {
            id: LeagueId::from(row.id),
            game_id: GameId::from(row.game_id),
            name: row.name,
            slug: row.slug,
            description: row.description,
            logo_url: row.logo_url,
            access_type: LeagueAccessType::from_str(&row.access_type)
                .unwrap_or(LeagueAccessType::Open),
            status: LeagueStatus::from_str(&row.status).unwrap_or(LeagueStatus::Active),
            current_season_id: row.current_season_id.map(portal_core::LeagueSeasonId::from),
            settings: row.settings,
            created_by: UserId::from(row.created_by),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<LeagueMemberRow> for LeagueMember {
    fn from(row: LeagueMemberRow) -> Self {
        Self {
            id: LeagueMemberId::from(row.id),
            league_id: LeagueId::from(row.league_id),
            user_id: UserId::from(row.user_id),
            membership_type: LeagueMembershipType::from_str(&row.membership_type)
                .unwrap_or(LeagueMembershipType::Member),
            joined_at: row.joined_at,
        }
    }
}

impl From<LeagueMemberWithUserRow> for LeagueMemberWithUser {
    fn from(row: LeagueMemberWithUserRow) -> Self {
        Self {
            id: LeagueMemberId::from(row.id),
            league_id: LeagueId::from(row.league_id),
            user_id: UserId::from(row.user_id),
            membership_type: LeagueMembershipType::from_str(&row.membership_type)
                .unwrap_or(LeagueMembershipType::Member),
            joined_at: row.joined_at,
            username: row.username,
            email: row.email,
        }
    }
}

impl From<LeagueInvitationRow> for LeagueInvitation {
    fn from(row: LeagueInvitationRow) -> Self {
        Self {
            id: LeagueInvitationId::from(row.id),
            league_id: LeagueId::from(row.league_id),
            user_id: UserId::from(row.user_id),
            invitation_type: LeagueInvitationType::from_str(&row.invitation_type)
                .unwrap_or(LeagueInvitationType::Invite),
            status: LeagueInvitationStatus::from_str(&row.status)
                .unwrap_or(LeagueInvitationStatus::Pending),
            message: row.message,
            invited_by: row.invited_by.map(UserId::from),
            responded_by: row.responded_by.map(UserId::from),
            responded_at: row.responded_at,
            expires_at: row.expires_at,
            created_at: row.created_at,
        }
    }
}

impl From<UserLeagueMembershipRow> for UserLeagueMembership {
    fn from(row: UserLeagueMembershipRow) -> Self {
        Self {
            league_id: LeagueId::from(row.league_id),
            league_name: row.league_name,
            league_slug: row.league_slug,
            league_logo_url: row.league_logo_url,
            game_id: GameId::from(row.game_id),
            membership_type: LeagueMembershipType::from_str(&row.membership_type)
                .unwrap_or(LeagueMembershipType::Member),
            joined_at: row.joined_at,
        }
    }
}

// =============================================================================
// League Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `LeagueRepository` trait.
#[derive(Clone)]
pub struct PgLeagueRepository {
    pool: DbPool,
}

impl PgLeagueRepository {
    /// Create a new `PostgreSQL` league repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LeagueRepository for PgLeagueRepository {
    async fn find_by_id(&self, id: LeagueId) -> Result<Option<League>, DomainError> {
        let league = sqlx::query_as::<_, LeagueRow>("SELECT * FROM leagues WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(league.map(League::from))
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<League>, DomainError> {
        let league = sqlx::query_as::<_, LeagueRow>("SELECT * FROM leagues WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(league.map(League::from))
    }

    async fn create(&self, cmd: CreateLeague) -> Result<League, DomainError> {
        let settings = cmd
            .settings
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        let league = sqlx::query_as::<_, LeagueRow>(
            r"
            INSERT INTO leagues (game_id, name, slug, description, logo_url, access_type, settings, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
        )
        .bind(cmd.game_id.as_uuid())
        .bind(&cmd.name)
        .bind(&cmd.slug)
        .bind(&cmd.description)
        .bind(&cmd.logo_url)
        .bind(&cmd.access_type)
        .bind(&settings)
        .bind(cmd.created_by.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(League::from(league))
    }

    async fn update(&self, id: LeagueId, update: UpdateLeague) -> Result<League, DomainError> {
        let league = sqlx::query_as::<_, LeagueRow>(
            r"
            UPDATE leagues SET
                name = COALESCE($2, name),
                slug = COALESCE($3, slug),
                description = COALESCE($4, description),
                logo_url = COALESCE($5, logo_url),
                access_type = COALESCE($6, access_type),
                status = COALESCE($7, status),
                settings = COALESCE($8, settings),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.slug)
        .bind(&update.description)
        .bind(&update.logo_url)
        .bind(&update.access_type)
        .bind(&update.status)
        .bind(&update.settings)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::LeagueNotFound(id))?;

        Ok(League::from(league))
    }

    async fn list_by_game(
        &self,
        game_id: &GameId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<League>, DomainError> {
        let leagues = sqlx::query_as::<_, LeagueRow>(
            r"
            SELECT * FROM leagues
            WHERE game_id = $1 AND status = 'active'
            ORDER BY name
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(game_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(leagues.into_iter().map(League::from).collect())
    }

    async fn count_by_game(&self, game_id: &GameId) -> Result<i64, DomainError> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM leagues WHERE game_id = $1 AND status = 'active'")
                .bind(game_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn slug_exists(&self, slug: &str) -> Result<bool, DomainError> {
        let exists: (bool,) =
            sqlx::query_as("SELECT EXISTS(SELECT 1 FROM leagues WHERE slug = $1)")
                .bind(slug)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(exists.0)
    }

    async fn search(
        &self,
        query: &str,
        game_id: Option<GameId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<League>, DomainError> {
        let pattern = format!("%{}%", query.to_lowercase());

        let leagues = match game_id {
            Some(gid) => {
                sqlx::query_as::<_, LeagueRow>(
                    r"
                    SELECT * FROM leagues
                    WHERE game_id = $1 AND status = 'active'
                      AND LOWER(name) LIKE $2
                    ORDER BY name
                    LIMIT $3 OFFSET $4
                    ",
                )
                .bind(gid.as_uuid())
                .bind(&pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, LeagueRow>(
                    r"
                    SELECT * FROM leagues
                    WHERE status = 'active' AND LOWER(name) LIKE $1
                    ORDER BY name
                    LIMIT $2 OFFSET $3
                    ",
                )
                .bind(&pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(leagues.into_iter().map(League::from).collect())
    }

    async fn count_search(&self, query: &str, game_id: Option<GameId>) -> Result<i64, DomainError> {
        let pattern = format!("%{}%", query.to_lowercase());

        let count: (i64,) =
            match game_id {
                Some(gid) => {
                    sqlx::query_as(
                        r"
                    SELECT COUNT(*) FROM leagues
                    WHERE game_id = $1 AND status = 'active' AND LOWER(name) LIKE $2
                    ",
                    )
                    .bind(gid.as_uuid())
                    .bind(&pattern)
                    .fetch_one(&self.pool)
                    .await
                }
                None => sqlx::query_as(
                    "SELECT COUNT(*) FROM leagues WHERE status = 'active' AND LOWER(name) LIKE $1",
                )
                .bind(&pattern)
                .fetch_one(&self.pool)
                .await,
            }
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }
}

// =============================================================================
// League Member Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `LeagueMemberRepository` trait.
#[derive(Clone)]
pub struct PgLeagueMemberRepository {
    pool: DbPool,
}

impl PgLeagueMemberRepository {
    /// Create a new `PostgreSQL` league member repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LeagueMemberRepository for PgLeagueMemberRepository {
    async fn find_member(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueMember>, DomainError> {
        let member = sqlx::query_as::<_, LeagueMemberRow>(
            "SELECT * FROM league_members WHERE league_id = $1 AND user_id = $2",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(member.map(LeagueMember::from))
    }

    async fn list_members(
        &self,
        league_id: LeagueId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueMemberWithUser>, DomainError> {
        let members = sqlx::query_as::<_, LeagueMemberWithUserRow>(
            r"
            SELECT lm.id, lm.league_id, lm.user_id, lm.membership_type, lm.joined_at,
                   u.username, u.email
            FROM league_members lm
            INNER JOIN users u ON u.id = lm.user_id
            WHERE lm.league_id = $1
            ORDER BY lm.membership_type, lm.joined_at
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(league_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(members
            .into_iter()
            .map(LeagueMemberWithUser::from)
            .collect())
    }

    async fn count_members(&self, league_id: LeagueId) -> Result<i64, DomainError> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM league_members WHERE league_id = $1")
                .bind(league_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }

    async fn add_member(&self, member: AddLeagueMember) -> Result<LeagueMember, DomainError> {
        let row = sqlx::query_as::<_, LeagueMemberRow>(
            r"
            INSERT INTO league_members (league_id, user_id, membership_type)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(member.league_id.as_uuid())
        .bind(member.user_id.as_uuid())
        .bind(member.membership_type.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueMember::from(row))
    }

    async fn remove_member(&self, league_id: LeagueId, user_id: UserId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM league_members WHERE league_id = $1 AND user_id = $2")
            .bind(league_id.as_uuid())
            .bind(user_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn update_membership_type(
        &self,
        league_id: LeagueId,
        user_id: UserId,
        membership_type: LeagueMembershipType,
    ) -> Result<LeagueMember, DomainError> {
        let row = sqlx::query_as::<_, LeagueMemberRow>(
            r"
            UPDATE league_members
            SET membership_type = $3
            WHERE league_id = $1 AND user_id = $2
            RETURNING *
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(membership_type.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::Internal("Member not found".to_string()))?;

        Ok(LeagueMember::from(row))
    }

    async fn is_member(&self, league_id: LeagueId, user_id: UserId) -> Result<bool, DomainError> {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM league_members WHERE league_id = $1 AND user_id = $2)",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(exists.0)
    }

    async fn is_admin(&self, league_id: LeagueId, user_id: UserId) -> Result<bool, DomainError> {
        let exists: (bool,) = sqlx::query_as(
            r"
            SELECT EXISTS(
                SELECT 1 FROM league_members
                WHERE league_id = $1 AND user_id = $2 AND membership_type = 'admin'
            )
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(exists.0)
    }

    async fn is_admin_or_moderator(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, DomainError> {
        let exists: (bool,) = sqlx::query_as(
            r"
            SELECT EXISTS(
                SELECT 1 FROM league_members
                WHERE league_id = $1 AND user_id = $2 AND membership_type IN ('admin', 'moderator')
            )
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(exists.0)
    }

    async fn list_memberships_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UserLeagueMembership>, DomainError> {
        let memberships = sqlx::query_as::<_, UserLeagueMembershipRow>(
            r"
            SELECT l.id as league_id, l.name as league_name, l.slug as league_slug,
                   l.logo_url as league_logo_url, l.game_id,
                   lm.membership_type, lm.joined_at
            FROM league_members lm
            INNER JOIN leagues l ON l.id = lm.league_id
            WHERE lm.user_id = $1 AND l.status = 'active'
            ORDER BY lm.joined_at DESC
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(memberships
            .into_iter()
            .map(UserLeagueMembership::from)
            .collect())
    }

    async fn count_admins(&self, league_id: LeagueId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_members WHERE league_id = $1 AND membership_type = 'admin'",
        )
        .bind(league_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }
}

// =============================================================================
// League Invitation Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `LeagueInvitationRepository` trait.
#[derive(Clone)]
pub struct PgLeagueInvitationRepository {
    pool: DbPool,
}

impl PgLeagueInvitationRepository {
    /// Create a new `PostgreSQL` league invitation repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LeagueInvitationRepository for PgLeagueInvitationRepository {
    async fn find_by_id(
        &self,
        id: LeagueInvitationId,
    ) -> Result<Option<LeagueInvitation>, DomainError> {
        let invitation = sqlx::query_as::<_, LeagueInvitationRow>(
            "SELECT * FROM league_invitations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(invitation.map(LeagueInvitation::from))
    }

    async fn create(
        &self,
        invitation: CreateLeagueInvitation,
    ) -> Result<LeagueInvitation, DomainError> {
        let row = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            INSERT INTO league_invitations (league_id, user_id, invitation_type, message, invited_by, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(invitation.league_id.as_uuid())
        .bind(invitation.user_id.as_uuid())
        .bind(&invitation.invitation_type)
        .bind(&invitation.message)
        .bind(invitation.invited_by.map(|u| u.as_uuid()))
        .bind(invitation.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueInvitation::from(row))
    }

    async fn update_status(
        &self,
        id: LeagueInvitationId,
        status: LeagueInvitationStatus,
        responded_by: UserId,
    ) -> Result<LeagueInvitation, DomainError> {
        let row = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            UPDATE league_invitations
            SET status = $2, responded_by = $3, responded_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.as_str())
        .bind(responded_by.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::Internal("Invitation not found".to_string()))?;

        Ok(LeagueInvitation::from(row))
    }

    async fn accept_and_add_member(
        &self,
        invitation_id: LeagueInvitationId,
        responded_by: UserId,
        member: AddLeagueMember,
    ) -> Result<LeagueMember, DomainError> {
        // Atomic counterpart of `update_status(Accepted) + add_member`.
        // Both writes commit together or neither does, so a failure of the
        // membership insert can no longer leave the invitation marked
        // `accepted` with the user outside the league. See audit I5 / the
        // league-team equivalent in adapters/league_team/invitation.rs.
        //
        // The membership insert is deliberately NOT an upsert: an existing
        // membership row means this invitation is not the reason the user is
        // in the league, and the whole operation must roll back rather than
        // silently consume the invitation.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let updated = sqlx::query(
            r"
            UPDATE league_invitations
            SET status = $2, responded_by = $3, responded_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(invitation_id.as_uuid())
        .bind(LeagueInvitationStatus::Accepted.as_str())
        .bind(responded_by.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if updated.rows_affected() == 0 {
            return Err(DomainError::Internal("Invitation not found".to_string()));
        }

        let row = sqlx::query_as::<_, LeagueMemberRow>(
            r"
            INSERT INTO league_members (league_id, user_id, membership_type)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(member.league_id.as_uuid())
        .bind(member.user_id.as_uuid())
        .bind(member.membership_type.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueMember::from(row))
    }

    async fn find_pending(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueInvitation>, DomainError> {
        let invitation = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            SELECT * FROM league_invitations
            WHERE league_id = $1 AND user_id = $2 AND status = 'pending'
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(invitation.map(LeagueInvitation::from))
    }

    async fn list_pending_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueInvitation>, DomainError> {
        let invitations = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            SELECT * FROM league_invitations
            WHERE league_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            ",
        )
        .bind(league_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(invitations
            .into_iter()
            .map(LeagueInvitation::from)
            .collect())
    }

    async fn list_pending_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<LeagueInvitation>, DomainError> {
        let invitations = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            SELECT * FROM league_invitations
            WHERE user_id = $1 AND status = 'pending' AND invitation_type = 'invite'
            ORDER BY created_at DESC
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(invitations
            .into_iter()
            .map(LeagueInvitation::from)
            .collect())
    }

    async fn cancel_pending(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE league_invitations
            SET status = 'expired'
            WHERE league_id = $1 AND user_id = $2 AND status = 'pending'
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn count_pending_applications(&self, league_id: LeagueId) -> Result<i64, DomainError> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*) FROM league_invitations
            WHERE league_id = $1 AND status = 'pending' AND invitation_type = 'application'
            ",
        )
        .bind(league_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0)
    }
}
