//! League repositories with trait-based design for testability.

use crate::DbPool;
use crate::entities::{
    LeagueInvitationRow, LeagueMemberRow, LeagueMemberWithUserRow, LeagueRow, NewLeague,
    NewLeagueInvitation, NewLeagueMember, UpdateLeague, UpdateLeagueInvitation,
    UserLeagueMembershipRow,
};
use crate::error::RepositoryError;
use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use portal_core::{GameId, LeagueId, LeagueInvitationId, UserId};
use sqlx::Row;

// =============================================================================
// League Repository Trait
// =============================================================================

/// Repository trait for league operations.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait LeagueRepository: Send + Sync {
    /// Find a league by ID.
    async fn find_by_id(&self, id: LeagueId) -> Result<Option<LeagueRow>, RepositoryError>;

    /// Find a league by slug.
    async fn find_by_slug(&self, slug: &str) -> Result<Option<LeagueRow>, RepositoryError>;

    /// Create a new league.
    async fn create(&self, new_league: NewLeague) -> Result<LeagueRow, RepositoryError>;

    /// Update an existing league.
    async fn update(
        &self,
        id: LeagueId,
        update: UpdateLeague,
    ) -> Result<LeagueRow, RepositoryError>;

    /// List leagues for a game.
    async fn list_by_game(
        &self,
        game_id: &GameId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueRow>, RepositoryError>;

    /// Count leagues for a game.
    async fn count_by_game(&self, game_id: &GameId) -> Result<i64, RepositoryError>;

    /// Check if a slug already exists.
    async fn slug_exists(&self, slug: &str) -> Result<bool, RepositoryError>;

    /// Search leagues by name.
    async fn search(
        &self,
        query: &str,
        game_id: Option<GameId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueRow>, RepositoryError>;

    /// Count search results.
    async fn count_search(
        &self,
        query: &str,
        game_id: Option<GameId>,
    ) -> Result<i64, RepositoryError>;
}

// =============================================================================
// League Member Repository Trait
// =============================================================================

/// Repository trait for league member operations.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait LeagueMemberRepository: Send + Sync {
    /// Find a member by league and user.
    async fn find_by_league_and_user(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueMemberRow>, RepositoryError>;

    /// List all members of a league with user info.
    async fn list_by_league(
        &self,
        league_id: LeagueId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueMemberWithUserRow>, RepositoryError>;

    /// Count members in a league.
    async fn count_by_league(&self, league_id: LeagueId) -> Result<i64, RepositoryError>;

    /// Add a member to a league.
    async fn create(&self, new_member: NewLeagueMember)
    -> Result<LeagueMemberRow, RepositoryError>;

    /// Remove a member from a league.
    async fn remove(&self, league_id: LeagueId, user_id: UserId) -> Result<(), RepositoryError>;

    /// Update member role.
    async fn update_membership_type(
        &self,
        league_id: LeagueId,
        user_id: UserId,
        membership_type: &str,
    ) -> Result<LeagueMemberRow, RepositoryError>;

    /// Check if user is a member of a league.
    async fn is_member(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError>;

    /// Check if user is an admin of a league.
    async fn is_admin(&self, league_id: LeagueId, user_id: UserId)
    -> Result<bool, RepositoryError>;

    /// Check if user is admin or moderator of a league.
    async fn is_admin_or_moderator(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError>;

    /// List all league memberships for a user.
    async fn list_memberships_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UserLeagueMembershipRow>, RepositoryError>;

    /// Count admins in a league.
    async fn count_admins(&self, league_id: LeagueId) -> Result<i64, RepositoryError>;
}

// =============================================================================
// League Invitation Repository Trait
// =============================================================================

/// Repository trait for league invitation/application operations.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait LeagueInvitationRepository: Send + Sync {
    /// Find an invitation by ID.
    async fn find_by_id(
        &self,
        id: LeagueInvitationId,
    ) -> Result<Option<LeagueInvitationRow>, RepositoryError>;

    /// Create a new invitation/application.
    async fn create(
        &self,
        invitation: NewLeagueInvitation,
    ) -> Result<LeagueInvitationRow, RepositoryError>;

    /// Update invitation status (accept/reject).
    async fn update_status(
        &self,
        id: LeagueInvitationId,
        update: UpdateLeagueInvitation,
    ) -> Result<LeagueInvitationRow, RepositoryError>;

    /// Find pending invitation for a league and user.
    async fn find_pending(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueInvitationRow>, RepositoryError>;

    /// List pending invitations for a league.
    async fn list_pending_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueInvitationRow>, RepositoryError>;

    /// List pending invitations/applications for a user.
    async fn list_pending_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<LeagueInvitationRow>, RepositoryError>;

    /// Cancel all pending invitations for a user in a league.
    async fn cancel_pending(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<(), RepositoryError>;

    /// Count pending applications for a league.
    async fn count_pending_applications(&self, league_id: LeagueId)
    -> Result<i64, RepositoryError>;
}

// =============================================================================
// PostgreSQL Implementations
// =============================================================================

/// `PostgreSQL` implementation of `LeagueRepository`.
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
    async fn find_by_id(&self, id: LeagueId) -> Result<Option<LeagueRow>, RepositoryError> {
        let league = sqlx::query_as::<_, LeagueRow>("SELECT * FROM leagues WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await?;

        Ok(league)
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<LeagueRow>, RepositoryError> {
        let league = sqlx::query_as::<_, LeagueRow>("SELECT * FROM leagues WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;

        Ok(league)
    }

    async fn create(&self, new_league: NewLeague) -> Result<LeagueRow, RepositoryError> {
        let league = sqlx::query_as::<_, LeagueRow>(
            r"
            INSERT INTO leagues (game_id, name, slug, description, logo_url, access_type, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
        )
        .bind(new_league.game_id)
        .bind(&new_league.name)
        .bind(&new_league.slug)
        .bind(&new_league.description)
        .bind(&new_league.logo_url)
        .bind(&new_league.access_type)
        .bind(new_league.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, &new_league.name))?;

        Ok(league)
    }

    async fn update(
        &self,
        id: LeagueId,
        update: UpdateLeague,
    ) -> Result<LeagueRow, RepositoryError> {
        let league = sqlx::query_as::<_, LeagueRow>(
            r"
            UPDATE leagues SET
                name = COALESCE($2, name),
                slug = COALESCE($3, slug),
                description = COALESCE($4, description),
                logo_url = COALESCE($5, logo_url),
                access_type = COALESCE($6, access_type),
                status = COALESCE($7, status),
                settings = COALESCE($8, settings)
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(update.name)
        .bind(update.slug)
        .bind(update.description)
        .bind(update.logo_url)
        .bind(update.access_type)
        .bind(update.status)
        .bind(update.settings)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("League", id))?;

        Ok(league)
    }

    async fn list_by_game(
        &self,
        game_id: &GameId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueRow>, RepositoryError> {
        let leagues = sqlx::query_as::<_, LeagueRow>(
            r"
            SELECT * FROM leagues
            WHERE game_id = $1 AND status = 'active'
            ORDER BY name
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(game_id.to_string())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(leagues)
    }

    async fn count_by_game(&self, game_id: &GameId) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM leagues WHERE game_id = $1 AND status = 'active'",
        )
        .bind(game_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }

    async fn slug_exists(&self, slug: &str) -> Result<bool, RepositoryError> {
        let row = sqlx::query("SELECT 1 FROM leagues WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    async fn search(
        &self,
        query: &str,
        game_id: Option<GameId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueRow>, RepositoryError> {
        let leagues = match game_id {
            Some(gid) => {
                sqlx::query_as::<_, LeagueRow>(
                    r"
                    SELECT * FROM leagues
                    WHERE status = 'active' AND game_id = $1 AND name ILIKE '%' || $2 || '%'
                    ORDER BY name
                    LIMIT $3 OFFSET $4
                    ",
                )
                .bind(gid.as_uuid())
                .bind(query)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, LeagueRow>(
                    r"
                    SELECT * FROM leagues
                    WHERE status = 'active' AND name ILIKE '%' || $1 || '%'
                    ORDER BY name
                    LIMIT $2 OFFSET $3
                    ",
                )
                .bind(query)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(leagues)
    }

    async fn count_search(
        &self,
        query: &str,
        game_id: Option<GameId>,
    ) -> Result<i64, RepositoryError> {
        let row = match game_id {
            Some(gid) => {
                sqlx::query(
                    r"
                    SELECT COUNT(*) as count FROM leagues
                    WHERE status = 'active' AND game_id = $1 AND name ILIKE '%' || $2 || '%'
                    ",
                )
                .bind(gid.as_uuid())
                .bind(query)
                .fetch_one(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    r"
                    SELECT COUNT(*) as count FROM leagues
                    WHERE status = 'active' AND name ILIKE '%' || $1 || '%'
                    ",
                )
                .bind(query)
                .fetch_one(&self.pool)
                .await?
            }
        };

        Ok(row.get("count"))
    }
}

// =============================================================================
// PostgreSQL League Member Repository
// =============================================================================

/// `PostgreSQL` implementation of `LeagueMemberRepository`.
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
    async fn find_by_league_and_user(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueMemberRow>, RepositoryError> {
        let member = sqlx::query_as::<_, LeagueMemberRow>(
            "SELECT * FROM league_members WHERE league_id = $1 AND user_id = $2",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(member)
    }

    async fn list_by_league(
        &self,
        league_id: LeagueId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueMemberWithUserRow>, RepositoryError> {
        let members = sqlx::query_as::<_, LeagueMemberWithUserRow>(
            r"
            SELECT
                lm.id, lm.league_id, lm.user_id, lm.membership_type, lm.joined_at,
                u.username, u.email
            FROM league_members lm
            INNER JOIN users u ON u.id = lm.user_id
            WHERE lm.league_id = $1
            ORDER BY
                CASE lm.membership_type
                    WHEN 'admin' THEN 1
                    WHEN 'moderator' THEN 2
                    WHEN 'member' THEN 3
                END,
                lm.joined_at
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(league_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(members)
    }

    async fn count_by_league(&self, league_id: LeagueId) -> Result<i64, RepositoryError> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM league_members WHERE league_id = $1")
            .bind(league_id.as_uuid())
            .fetch_one(&self.pool)
            .await?;

        Ok(row.get("count"))
    }

    async fn create(
        &self,
        new_member: NewLeagueMember,
    ) -> Result<LeagueMemberRow, RepositoryError> {
        let member = sqlx::query_as::<_, LeagueMemberRow>(
            r"
            INSERT INTO league_members (league_id, user_id, membership_type)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(new_member.league_id)
        .bind(new_member.user_id)
        .bind(&new_member.membership_type)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "league member"))?;

        Ok(member)
    }

    async fn remove(&self, league_id: LeagueId, user_id: UserId) -> Result<(), RepositoryError> {
        let result =
            sqlx::query("DELETE FROM league_members WHERE league_id = $1 AND user_id = $2")
                .bind(league_id.as_uuid())
                .bind(user_id.as_uuid())
                .execute(&self.pool)
                .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::not_found(
                "LeagueMember",
                format!("{league_id}/{user_id}"),
            ));
        }

        Ok(())
    }

    async fn update_membership_type(
        &self,
        league_id: LeagueId,
        user_id: UserId,
        membership_type: &str,
    ) -> Result<LeagueMemberRow, RepositoryError> {
        let member = sqlx::query_as::<_, LeagueMemberRow>(
            r"
            UPDATE league_members SET membership_type = $3
            WHERE league_id = $1 AND user_id = $2
            RETURNING *
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(membership_type)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            RepositoryError::not_found("LeagueMember", format!("{league_id}/{user_id}"))
        })?;

        Ok(member)
    }

    async fn is_member(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        let row = sqlx::query("SELECT 1 FROM league_members WHERE league_id = $1 AND user_id = $2")
            .bind(league_id.as_uuid())
            .bind(user_id.as_uuid())
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    async fn is_admin(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        let row = sqlx::query(
            "SELECT 1 FROM league_members WHERE league_id = $1 AND user_id = $2 AND membership_type = 'admin'",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    async fn is_admin_or_moderator(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        let row = sqlx::query(
            "SELECT 1 FROM league_members WHERE league_id = $1 AND user_id = $2 AND membership_type IN ('admin', 'moderator')",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    async fn list_memberships_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UserLeagueMembershipRow>, RepositoryError> {
        let memberships = sqlx::query_as::<_, UserLeagueMembershipRow>(
            r"
            SELECT
                l.id as league_id,
                l.name as league_name,
                l.slug as league_slug,
                l.logo_url as league_logo_url,
                l.game_id,
                lm.membership_type,
                lm.joined_at
            FROM league_members lm
            INNER JOIN leagues l ON l.id = lm.league_id
            WHERE lm.user_id = $1 AND l.status = 'active'
            ORDER BY lm.joined_at DESC
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(memberships)
    }

    async fn count_admins(&self, league_id: LeagueId) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM league_members WHERE league_id = $1 AND membership_type = 'admin'",
        )
        .bind(league_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }
}

// =============================================================================
// PostgreSQL League Invitation Repository
// =============================================================================

/// `PostgreSQL` implementation of `LeagueInvitationRepository`.
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
    ) -> Result<Option<LeagueInvitationRow>, RepositoryError> {
        let invitation = sqlx::query_as::<_, LeagueInvitationRow>(
            "SELECT * FROM league_invitations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(invitation)
    }

    async fn create(
        &self,
        invitation: NewLeagueInvitation,
    ) -> Result<LeagueInvitationRow, RepositoryError> {
        let row = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            INSERT INTO league_invitations (league_id, user_id, invitation_type, message, invited_by, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(invitation.league_id)
        .bind(invitation.user_id)
        .bind(&invitation.invitation_type)
        .bind(&invitation.message)
        .bind(invitation.invited_by)
        .bind(invitation.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "league invitation"))?;

        Ok(row)
    }

    async fn update_status(
        &self,
        id: LeagueInvitationId,
        update: UpdateLeagueInvitation,
    ) -> Result<LeagueInvitationRow, RepositoryError> {
        let row = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            UPDATE league_invitations SET
                status = $2,
                responded_by = $3,
                responded_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.status)
        .bind(update.responded_by)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("LeagueInvitation", id))?;

        Ok(row)
    }

    async fn find_pending(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueInvitationRow>, RepositoryError> {
        let row = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            SELECT * FROM league_invitations
            WHERE league_id = $1 AND user_id = $2 AND status = 'pending'
                AND (expires_at IS NULL OR expires_at > NOW())
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn list_pending_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueInvitationRow>, RepositoryError> {
        let rows = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            SELECT * FROM league_invitations
            WHERE league_id = $1 AND status = 'pending'
                AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            ",
        )
        .bind(league_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn list_pending_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<LeagueInvitationRow>, RepositoryError> {
        let rows = sqlx::query_as::<_, LeagueInvitationRow>(
            r"
            SELECT * FROM league_invitations
            WHERE user_id = $1 AND status = 'pending'
                AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn cancel_pending(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r"
            UPDATE league_invitations SET status = 'expired', responded_at = NOW()
            WHERE league_id = $1 AND user_id = $2 AND status = 'pending'
            ",
        )
        .bind(league_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn count_pending_applications(
        &self,
        league_id: LeagueId,
    ) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count FROM league_invitations
            WHERE league_id = $1 AND status = 'pending' AND invitation_type = 'application'
                AND (expires_at IS NULL OR expires_at > NOW())
            ",
        )
        .bind(league_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portal_test::database::TestDb;

    // Helper to create a test user
    async fn create_test_user(pool: &DbPool, suffix: &str) -> uuid::Uuid {
        let user = sqlx::query_as::<_, (uuid::Uuid,)>(
            r#"
            INSERT INTO users (username, email, password_hash)
            VALUES ($1, $2, 'hash')
            RETURNING id
            "#,
        )
        .bind(format!("leagueuser{suffix}"))
        .bind(format!("league{suffix}@example.com"))
        .fetch_one(pool)
        .await
        .unwrap();
        user.0
    }

    // Helper to create a test game
    async fn create_test_game(pool: &DbPool, slug: &str) -> uuid::Uuid {
        let id = uuid::Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO games (id, slug, name, display_name, status)
            VALUES ($1, $2, $3, $4, 'active')
            ON CONFLICT (slug) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(slug)
        .bind(slug)
        .bind(slug.to_uppercase())
        .execute(pool)
        .await
        .unwrap();
        id
    }

    // ===========================================
    // LeagueRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_create_league() {
        let db = TestDb::new().await;
        let repo = PgLeagueRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "createleague").await;
        let game_id = create_test_game(&db.pool, "test_game_create").await;

        let new_league = NewLeague {
            game_id,
            name: "Test League".to_string(),
            slug: "test-league".to_string(),
            description: Some("A test league".to_string()),
            logo_url: None,
            access_type: "open".to_string(),
            created_by: user_id,
        };

        let league = repo.create(new_league).await.unwrap();
        assert_eq!(league.name, "Test League");
        assert_eq!(league.slug, "test-league");
        assert_eq!(league.status, "active");
        assert_eq!(league.access_type, "open");
    }

    #[tokio::test]
    async fn test_find_league_by_id() {
        let db = TestDb::new().await;
        let repo = PgLeagueRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "findbyid").await;
        let game_id = create_test_game(&db.pool, "test_game_find").await;

        let new_league = NewLeague {
            game_id,
            name: "Find By ID League".to_string(),
            slug: "find-by-id-league".to_string(),
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            created_by: user_id,
        };
        let created = repo.create(new_league).await.unwrap();

        let found = repo.find_by_id(LeagueId::from(created.id)).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Find By ID League");

        let not_found = repo
            .find_by_id(LeagueId::from(uuid::Uuid::nil()))
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_league_by_slug() {
        let db = TestDb::new().await;
        let repo = PgLeagueRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "findbyslug").await;
        let game_id = create_test_game(&db.pool, "test_game_slug").await;

        let new_league = NewLeague {
            game_id,
            name: "Slug Test League".to_string(),
            slug: "slug-test-league".to_string(),
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            created_by: user_id,
        };
        repo.create(new_league).await.unwrap();

        let found = repo.find_by_slug("slug-test-league").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Slug Test League");

        let not_found = repo.find_by_slug("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_update_league() {
        let db = TestDb::new().await;
        let repo = PgLeagueRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "updateleague").await;
        let game_id = create_test_game(&db.pool, "test_game_update").await;

        let new_league = NewLeague {
            game_id,
            name: "Original League".to_string(),
            slug: "original-league".to_string(),
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            created_by: user_id,
        };
        let created = repo.create(new_league).await.unwrap();

        let update = UpdateLeague {
            name: Some("Updated League".to_string()),
            description: Some("New description".to_string()),
            access_type: Some("invite_only".to_string()),
            ..Default::default()
        };

        let updated = repo
            .update(LeagueId::from(created.id), update)
            .await
            .unwrap();
        assert_eq!(updated.name, "Updated League");
        assert_eq!(updated.description, Some("New description".to_string()));
        assert_eq!(updated.access_type, "invite_only");
    }

    #[tokio::test]
    async fn test_slug_exists() {
        let db = TestDb::new().await;
        let repo = PgLeagueRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "slugexists").await;
        let game_id = create_test_game(&db.pool, "test_game_slugex").await;

        let new_league = NewLeague {
            game_id,
            name: "Unique Slug League".to_string(),
            slug: "unique-slug-league".to_string(),
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            created_by: user_id,
        };
        repo.create(new_league).await.unwrap();

        assert!(repo.slug_exists("unique-slug-league").await.unwrap());
        assert!(!repo.slug_exists("nonexistent-slug").await.unwrap());
    }

    // ===========================================
    // LeagueMemberRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_add_league_member() {
        let db = TestDb::new().await;
        let league_repo = PgLeagueRepository::new(db.pool.clone());
        let member_repo = PgLeagueMemberRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "addmember").await;
        let game_id = create_test_game(&db.pool, "test_game_member").await;

        let new_league = NewLeague {
            game_id,
            name: "Member Test League".to_string(),
            slug: "member-test-league".to_string(),
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            created_by: user_id,
        };
        let league = league_repo.create(new_league).await.unwrap();

        let new_member = NewLeagueMember {
            league_id: league.id,
            user_id,
            membership_type: "admin".to_string(),
        };

        let member = member_repo.create(new_member).await.unwrap();
        assert_eq!(member.league_id, league.id);
        assert_eq!(member.user_id, user_id);
        assert_eq!(member.membership_type, "admin");
    }

    #[tokio::test]
    async fn test_is_member_is_admin() {
        let db = TestDb::new().await;
        let league_repo = PgLeagueRepository::new(db.pool.clone());
        let member_repo = PgLeagueMemberRepository::new(db.pool.clone());

        let admin_id = create_test_user(&db.pool, "isadmin").await;
        let member_id = create_test_user(&db.pool, "ismember").await;
        let outsider_id = create_test_user(&db.pool, "outsider").await;
        let game_id = create_test_game(&db.pool, "test_game_roles").await;

        let new_league = NewLeague {
            game_id,
            name: "Roles Test League".to_string(),
            slug: "roles-test-league".to_string(),
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            created_by: admin_id,
        };
        let league = league_repo.create(new_league).await.unwrap();
        let league_id = LeagueId::from(league.id);

        // Add admin
        member_repo
            .create(NewLeagueMember {
                league_id: league.id,
                user_id: admin_id,
                membership_type: "admin".to_string(),
            })
            .await
            .unwrap();

        // Add regular member
        member_repo
            .create(NewLeagueMember {
                league_id: league.id,
                user_id: member_id,
                membership_type: "member".to_string(),
            })
            .await
            .unwrap();

        // Test is_member
        assert!(
            member_repo
                .is_member(league_id, UserId::from(admin_id))
                .await
                .unwrap()
        );
        assert!(
            member_repo
                .is_member(league_id, UserId::from(member_id))
                .await
                .unwrap()
        );
        assert!(
            !member_repo
                .is_member(league_id, UserId::from(outsider_id))
                .await
                .unwrap()
        );

        // Test is_admin
        assert!(
            member_repo
                .is_admin(league_id, UserId::from(admin_id))
                .await
                .unwrap()
        );
        assert!(
            !member_repo
                .is_admin(league_id, UserId::from(member_id))
                .await
                .unwrap()
        );
        assert!(
            !member_repo
                .is_admin(league_id, UserId::from(outsider_id))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_remove_league_member() {
        let db = TestDb::new().await;
        let league_repo = PgLeagueRepository::new(db.pool.clone());
        let member_repo = PgLeagueMemberRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "removemember").await;
        let game_id = create_test_game(&db.pool, "test_game_remove").await;

        let new_league = NewLeague {
            game_id,
            name: "Remove Member League".to_string(),
            slug: "remove-member-league".to_string(),
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            created_by: user_id,
        };
        let league = league_repo.create(new_league).await.unwrap();
        let league_id = LeagueId::from(league.id);

        member_repo
            .create(NewLeagueMember {
                league_id: league.id,
                user_id,
                membership_type: "member".to_string(),
            })
            .await
            .unwrap();

        assert!(
            member_repo
                .is_member(league_id, UserId::from(user_id))
                .await
                .unwrap()
        );

        member_repo
            .remove(league_id, UserId::from(user_id))
            .await
            .unwrap();

        assert!(
            !member_repo
                .is_member(league_id, UserId::from(user_id))
                .await
                .unwrap()
        );
    }
}
