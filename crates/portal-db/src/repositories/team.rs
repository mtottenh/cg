//! Team repositories.

use crate::entities::{
    NewTeam, NewTeamInvitation, NewTeamMember, PlayerTeamMembershipRow, TeamInvitationRow,
    TeamMemberRow, TeamRow, UpdateTeam, UpdateTeamInvitation, UpdateTeamMember,
};
use crate::error::RepositoryError;
use crate::DbPool;
use portal_core::{PlayerId, TeamId, TeamInvitationId};
use sqlx::Row;
use uuid::Uuid;

/// Repository for team operations.
#[derive(Clone)]
pub struct TeamRepository {
    pool: DbPool,
}

impl TeamRepository {
    /// Create a new team repository.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a team by ID.
    pub async fn find_by_id(&self, id: TeamId) -> Result<Option<TeamRow>, RepositoryError> {
        let team = sqlx::query_as::<_, TeamRow>("SELECT * FROM teams WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await?;

        Ok(team)
    }

    /// Find a team by name.
    pub async fn find_by_name(&self, name: &str) -> Result<Option<TeamRow>, RepositoryError> {
        let team =
            sqlx::query_as::<_, TeamRow>("SELECT * FROM teams WHERE name_normalized = lower($1)")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;

        Ok(team)
    }

    /// Find a team by tag.
    pub async fn find_by_tag(&self, tag: &str) -> Result<Option<TeamRow>, RepositoryError> {
        let team =
            sqlx::query_as::<_, TeamRow>("SELECT * FROM teams WHERE tag_normalized = lower($1)")
                .bind(tag)
                .fetch_optional(&self.pool)
                .await?;

        Ok(team)
    }

    /// Create a new team.
    pub async fn create(&self, new_team: NewTeam) -> Result<TeamRow, RepositoryError> {
        let team = sqlx::query_as::<_, TeamRow>(
            r#"
            INSERT INTO teams (name, tag, created_by, description, logo_url, game_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(&new_team.name)
        .bind(&new_team.tag)
        .bind(new_team.created_by)
        .bind(&new_team.description)
        .bind(&new_team.logo_url)
        .bind(&new_team.game_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, &new_team.name))?;

        Ok(team)
    }

    /// Update a team.
    pub async fn update(&self, id: TeamId, update: UpdateTeam) -> Result<TeamRow, RepositoryError> {
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
                settings = COALESCE($9, settings),
                social_links = COALESCE($10, social_links),
                website_url = COALESCE($11, website_url),
                status = COALESCE($12, status),
                disbanded_at = COALESCE($13, disbanded_at),
                disbanded_reason = COALESCE($14, disbanded_reason)
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id.as_uuid())
        .bind(update.name)
        .bind(update.tag)
        .bind(update.description)
        .bind(update.logo_url)
        .bind(update.banner_url)
        .bind(update.primary_color)
        .bind(update.secondary_color)
        .bind(update.settings)
        .bind(update.social_links)
        .bind(update.website_url)
        .bind(update.status)
        .bind(update.disbanded_at)
        .bind(update.disbanded_reason)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("Team", id))?;

        Ok(team)
    }

    /// List teams for a player.
    pub async fn list_by_player(&self, player_id: PlayerId) -> Result<Vec<TeamRow>, RepositoryError> {
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
        .await?;

        Ok(teams)
    }

    /// Search teams by name or tag.
    pub async fn search(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TeamRow>, RepositoryError> {
        let teams = sqlx::query_as::<_, TeamRow>(
            r#"
            SELECT * FROM teams
            WHERE status = 'active' AND (
                name_normalized LIKE '%' || $1 || '%'
                OR tag_normalized LIKE '%' || $1 || '%'
            )
            ORDER BY name_normalized
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(query.to_lowercase())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(teams)
    }

    /// Count total teams matching a search query.
    pub async fn count_search(&self, query: &str) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM teams
            WHERE status = 'active' AND (
                name_normalized LIKE '%' || $1 || '%'
                OR tag_normalized LIKE '%' || $1 || '%'
            )
            "#,
        )
        .bind(query.to_lowercase())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }

    /// Check if a team name is already taken.
    pub async fn name_exists(&self, name: &str) -> Result<bool, RepositoryError> {
        let row = sqlx::query("SELECT 1 FROM teams WHERE name_normalized = lower($1)")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    /// Check if a team tag is already taken.
    pub async fn tag_exists(&self, tag: &str) -> Result<bool, RepositoryError> {
        let row = sqlx::query("SELECT 1 FROM teams WHERE tag_normalized = lower($1)")
            .bind(tag)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }
}

/// Repository for team member operations.
#[derive(Clone)]
pub struct TeamMemberRepository {
    pool: DbPool,
}

impl TeamMemberRepository {
    /// Create a new team member repository.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a team member by team and player.
    pub async fn find_by_team_and_player(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<Option<TeamMemberRow>, RepositoryError> {
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
        .await?;

        Ok(member)
    }

    /// List all active members of a team.
    pub async fn list_by_team(&self, team_id: TeamId) -> Result<Vec<TeamMemberRow>, RepositoryError> {
        let members = sqlx::query_as::<_, TeamMemberRow>(
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
        .await?;

        Ok(members)
    }

    /// Count captains in a team.
    pub async fn count_captains(&self, team_id: TeamId) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM team_members WHERE team_id = $1 AND role = 'captain' AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }

    /// Count active members in a team.
    pub async fn count_members(&self, team_id: TeamId) -> Result<i64, RepositoryError> {
        let row =
            sqlx::query("SELECT COUNT(*) as count FROM team_members WHERE team_id = $1 AND left_at IS NULL")
                .bind(team_id.as_uuid())
                .fetch_one(&self.pool)
                .await?;

        Ok(row.get("count"))
    }

    /// Add a member to a team.
    pub async fn create(&self, new_member: NewTeamMember) -> Result<TeamMemberRow, RepositoryError> {
        let member = sqlx::query_as::<_, TeamMemberRow>(
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
        .bind(new_member.team_id)
        .bind(new_member.player_id)
        .bind(&new_member.role)
        .bind(new_member.is_founder)
        .bind(new_member.invited_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "team member"))?;

        Ok(member)
    }

    /// Update a team member.
    pub async fn update(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
        update: UpdateTeamMember,
    ) -> Result<TeamMemberRow, RepositoryError> {
        let member = sqlx::query_as::<_, TeamMemberRow>(
            r#"
            WITH updated AS (
                UPDATE team_members SET
                    role = COALESCE($3, role),
                    role_title = COALESCE($4, role_title),
                    primary_position = COALESCE($5, primary_position),
                    secondary_position = COALESCE($6, secondary_position),
                    status = COALESCE($7, status),
                    jersey_number = COALESCE($8, jersey_number),
                    left_at = COALESCE($9, left_at)
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
        .bind(update.role)
        .bind(update.role_title)
        .bind(update.primary_position)
        .bind(update.secondary_position)
        .bind(update.status)
        .bind(update.jersey_number)
        .bind(update.left_at)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("TeamMember", format!("{team_id}/{player_id}")))?;

        Ok(member)
    }

    /// Remove a member from a team (soft delete by setting left_at).
    pub async fn remove(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE team_members SET left_at = NOW() WHERE team_id = $1 AND player_id = $2 AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::not_found(
                "TeamMember",
                format!("{team_id}/{player_id}"),
            ));
        }

        Ok(())
    }

    /// Check if a player is a member of a team.
    pub async fn is_member(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<bool, RepositoryError> {
        let row = sqlx::query(
            "SELECT 1 FROM team_members WHERE team_id = $1 AND player_id = $2 AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    /// Check if a player is a captain of a team.
    pub async fn is_captain(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<bool, RepositoryError> {
        let row = sqlx::query(
            "SELECT 1 FROM team_members WHERE team_id = $1 AND player_id = $2 AND role = 'captain' AND left_at IS NULL",
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    /// List all team memberships for a player (with team details).
    pub async fn list_memberships_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerTeamMembershipRow>, RepositoryError> {
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
        .await?;

        Ok(memberships)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{NewPlayer, NewUser, UpdateTeam, UpdateTeamMember};
    use crate::repositories::{PlayerRepository, UserRepository};
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
        .bind(format!("teamuser{}", suffix))
        .bind(format!("team{}@example.com", suffix))
        .fetch_one(pool)
        .await
        .unwrap();
        user.0
    }

    // Helper to create a test player
    async fn create_test_player(pool: &DbPool, user_id: uuid::Uuid, suffix: &str) -> uuid::Uuid {
        let player = sqlx::query_as::<_, (uuid::Uuid,)>(
            r#"
            INSERT INTO players (user_id, display_name, country_code)
            VALUES ($1, $2, 'US')
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(format!("TeamPlayer{}", suffix))
        .fetch_one(pool)
        .await
        .unwrap();
        player.0
    }

    // Helper to create a complete player (user + player)
    async fn setup_player(pool: &DbPool, suffix: &str) -> uuid::Uuid {
        let user_id = create_test_user(pool, suffix).await;
        create_test_player(pool, user_id, suffix).await
    }

    // ===========================================
    // TeamRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_create_team() {
        let db = TestDb::new().await;
        let repo = TeamRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "create").await;

        let new_team = NewTeam {
            name: "Test Team".to_string(),
            tag: "TST".to_string(),
            created_by: player_id,
            description: Some("A test team".to_string()),
            logo_url: None,
            game_id: None,
        };

        let team = repo.create(new_team).await.unwrap();
        assert_eq!(team.name, "Test Team");
        assert_eq!(team.tag, "TST");
        assert_eq!(team.name_normalized, "test team");
        assert_eq!(team.tag_normalized, "tst");
        assert_eq!(team.status, "active");
    }

    #[tokio::test]
    async fn test_find_team_by_id() {
        let db = TestDb::new().await;
        let repo = TeamRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "findid").await;

        let new_team = NewTeam {
            name: "FindById Team".to_string(),
            tag: "FBT".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let created = repo.create(new_team).await.unwrap();

        let found = repo.find_by_id(TeamId::from(created.id)).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "FindById Team");

        // Not found
        let not_found = repo.find_by_id(TeamId::from(uuid::Uuid::nil())).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_team_by_name() {
        let db = TestDb::new().await;
        let repo = TeamRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "findname").await;

        let new_team = NewTeam {
            name: "UniqueNameTeam".to_string(),
            tag: "UNT".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        repo.create(new_team).await.unwrap();

        // Case-insensitive search
        let found = repo.find_by_name("uniquenameteam").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().tag, "UNT");

        let not_found = repo.find_by_name("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_team_by_tag() {
        let db = TestDb::new().await;
        let repo = TeamRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "findtag").await;

        let new_team = NewTeam {
            name: "Tag Test Team".to_string(),
            tag: "XYZ".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        repo.create(new_team).await.unwrap();

        // Case-insensitive search
        let found = repo.find_by_tag("xyz").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Tag Test Team");
    }

    #[tokio::test]
    async fn test_update_team() {
        let db = TestDb::new().await;
        let repo = TeamRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "update").await;

        let new_team = NewTeam {
            name: "Original Team".to_string(),
            tag: "ORT".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let created = repo.create(new_team).await.unwrap();

        let update = UpdateTeam {
            name: Some("Updated Team".to_string()),
            description: Some("New description".to_string()),
            primary_color: Some("#FF0000".to_string()),
            ..Default::default()
        };

        let updated = repo.update(TeamId::from(created.id), update).await.unwrap();
        assert_eq!(updated.name, "Updated Team");
        assert_eq!(updated.description, Some("New description".to_string()));
        assert_eq!(updated.primary_color, Some("#FF0000".to_string()));
    }

    #[tokio::test]
    async fn test_list_teams_by_player() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "listbyplayer").await;

        // Create two teams
        for i in 1..=2 {
            let new_team = NewTeam {
                name: format!("List Team {}", i),
                tag: format!("LT{}", i),
                created_by: player_id,
                description: None,
                logo_url: None,
                game_id: None,
            };
            let team = team_repo.create(new_team).await.unwrap();

            // Add player as member
            let new_member = NewTeamMember {
                team_id: team.id,
                player_id,
                role: "captain".to_string(),
                is_founder: true,
                invited_by: None,
            };
            member_repo.create(new_member).await.unwrap();
        }

        let teams = team_repo.list_by_player(PlayerId::from(player_id)).await.unwrap();
        assert_eq!(teams.len(), 2);
    }

    #[tokio::test]
    async fn test_team_search() {
        let db = TestDb::new().await;
        let repo = TeamRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "search").await;

        for i in 1..=3 {
            let new_team = NewTeam {
                name: format!("SearchTeam{}", i),
                tag: format!("ST{}", i),
                created_by: player_id,
                description: None,
                logo_url: None,
                game_id: None,
            };
            repo.create(new_team).await.unwrap();
        }

        let results = repo.search("searchteam", 10, 0).await.unwrap();
        assert_eq!(results.len(), 3);

        // Search by tag
        let results = repo.search("st2", 10, 0).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_name_tag_uniqueness() {
        let db = TestDb::new().await;
        let repo = TeamRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "unique").await;

        let new_team = NewTeam {
            name: "UniqueName".to_string(),
            tag: "UTG".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        repo.create(new_team).await.unwrap();

        // Check existence
        assert!(repo.name_exists("uniquename").await.unwrap());
        assert!(repo.name_exists("UNIQUENAME").await.unwrap());
        assert!(!repo.name_exists("othername").await.unwrap());

        assert!(repo.tag_exists("utg").await.unwrap());
        assert!(repo.tag_exists("UTG").await.unwrap());
        assert!(!repo.tag_exists("XXX").await.unwrap());
    }

    // ===========================================
    // TeamMemberRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_add_team_member() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "addmember").await;

        let new_team = NewTeam {
            name: "Member Test Team".to_string(),
            tag: "MTT".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        let new_member = NewTeamMember {
            team_id: team.id,
            player_id,
            role: "captain".to_string(),
            is_founder: true,
            invited_by: None,
        };

        let member = member_repo.create(new_member).await.unwrap();
        assert_eq!(member.team_id, team.id);
        assert_eq!(member.player_id, player_id);
        assert_eq!(member.role, "captain");
        assert!(member.is_founder);
        assert_eq!(member.status, "active");
    }

    #[tokio::test]
    async fn test_find_member() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "findmember").await;

        let new_team = NewTeam {
            name: "Find Member Team".to_string(),
            tag: "FMT".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        let new_member = NewTeamMember {
            team_id: team.id,
            player_id,
            role: "captain".to_string(),
            is_founder: true,
            invited_by: None,
        };
        member_repo.create(new_member).await.unwrap();

        let found = member_repo
            .find_by_team_and_player(TeamId::from(team.id), PlayerId::from(player_id))
            .await
            .unwrap();
        assert!(found.is_some());

        // Not found
        let not_found = member_repo
            .find_by_team_and_player(TeamId::from(team.id), PlayerId::from(uuid::Uuid::nil()))
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_list_team_members() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let captain_id = setup_player(&db.pool, "listcaptain").await;
        let player2_id = setup_player(&db.pool, "listplayer2").await;

        let new_team = NewTeam {
            name: "List Members Team".to_string(),
            tag: "LMT".to_string(),
            created_by: captain_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        // Add captain
        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id: captain_id,
            role: "captain".to_string(),
            is_founder: true,
            invited_by: None,
        }).await.unwrap();

        // Add regular player
        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id: player2_id,
            role: "player".to_string(),
            is_founder: false,
            invited_by: Some(captain_id),
        }).await.unwrap();

        let members = member_repo.list_by_team(TeamId::from(team.id)).await.unwrap();
        assert_eq!(members.len(), 2);
        // Captain should be first (role ordering)
        assert_eq!(members[0].role, "captain");
    }

    #[tokio::test]
    async fn test_update_member_role() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "updaterole").await;

        let new_team = NewTeam {
            name: "Update Role Team".to_string(),
            tag: "URT".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id,
            role: "player".to_string(),
            is_founder: false,
            invited_by: None,
        }).await.unwrap();

        let update = UpdateTeamMember {
            role: Some("officer".to_string()),
            role_title: Some("Team Manager".to_string()),
            ..Default::default()
        };

        let updated = member_repo
            .update(TeamId::from(team.id), PlayerId::from(player_id), update)
            .await
            .unwrap();
        assert_eq!(updated.role, "officer");
        assert_eq!(updated.role_title, Some("Team Manager".to_string()));
    }

    #[tokio::test]
    async fn test_remove_member() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let player_id = setup_player(&db.pool, "removemember").await;

        let new_team = NewTeam {
            name: "Remove Member Team".to_string(),
            tag: "RMT".to_string(),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id,
            role: "player".to_string(),
            is_founder: false,
            invited_by: None,
        }).await.unwrap();

        // Remove (soft delete)
        member_repo.remove(TeamId::from(team.id), PlayerId::from(player_id)).await.unwrap();

        // Should no longer find the member
        let found = member_repo
            .find_by_team_and_player(TeamId::from(team.id), PlayerId::from(player_id))
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_count_captains() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let captain1 = setup_player(&db.pool, "countcaptain1").await;
        let captain2 = setup_player(&db.pool, "countcaptain2").await;
        let player = setup_player(&db.pool, "countplayer").await;

        let new_team = NewTeam {
            name: "Count Captains Team".to_string(),
            tag: "CCT".to_string(),
            created_by: captain1,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        // Add two captains
        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id: captain1,
            role: "captain".to_string(),
            is_founder: true,
            invited_by: None,
        }).await.unwrap();

        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id: captain2,
            role: "captain".to_string(),
            is_founder: false,
            invited_by: Some(captain1),
        }).await.unwrap();

        // Add regular player
        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id: player,
            role: "player".to_string(),
            is_founder: false,
            invited_by: Some(captain1),
        }).await.unwrap();

        let count = member_repo.count_captains(TeamId::from(team.id)).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_is_member_is_captain() {
        let db = TestDb::new().await;
        let team_repo = TeamRepository::new(db.pool.clone());
        let member_repo = TeamMemberRepository::new(db.pool.clone());

        let captain_id = setup_player(&db.pool, "iscaptain").await;
        let player_id = setup_player(&db.pool, "isplayer").await;
        let outsider_id = setup_player(&db.pool, "outsider").await;

        let new_team = NewTeam {
            name: "Is Member Team".to_string(),
            tag: "IMT".to_string(),
            created_by: captain_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id: captain_id,
            role: "captain".to_string(),
            is_founder: true,
            invited_by: None,
        }).await.unwrap();

        member_repo.create(NewTeamMember {
            team_id: team.id,
            player_id: player_id,
            role: "player".to_string(),
            is_founder: false,
            invited_by: Some(captain_id),
        }).await.unwrap();

        let team_id = TeamId::from(team.id);

        // Test is_member
        assert!(member_repo.is_member(team_id, PlayerId::from(captain_id)).await.unwrap());
        assert!(member_repo.is_member(team_id, PlayerId::from(player_id)).await.unwrap());
        assert!(!member_repo.is_member(team_id, PlayerId::from(outsider_id)).await.unwrap());

        // Test is_captain
        assert!(member_repo.is_captain(team_id, PlayerId::from(captain_id)).await.unwrap());
        assert!(!member_repo.is_captain(team_id, PlayerId::from(player_id)).await.unwrap());
        assert!(!member_repo.is_captain(team_id, PlayerId::from(outsider_id)).await.unwrap());
    }
}

/// Repository for team invitation operations.
#[derive(Clone)]
pub struct TeamInvitationRepository {
    pool: DbPool,
}

impl TeamInvitationRepository {
    /// Create a new team invitation repository.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Create a new invitation.
    pub async fn create(
        &self,
        invitation: NewTeamInvitation,
    ) -> Result<TeamInvitationRow, RepositoryError> {
        let row = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            INSERT INTO team_invitations (team_id, player_id, type, role, message, invited_by)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(invitation.team_id)
        .bind(invitation.player_id)
        .bind(&invitation.invitation_type)
        .bind(&invitation.role)
        .bind(&invitation.message)
        .bind(invitation.invited_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "team invitation"))?;

        Ok(row)
    }

    /// Find an invitation by ID.
    pub async fn find_by_id(
        &self,
        id: TeamInvitationId,
    ) -> Result<Option<TeamInvitationRow>, RepositoryError> {
        let row = sqlx::query_as::<_, TeamInvitationRow>(
            "SELECT * FROM team_invitations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Find all pending invitations for a team.
    pub async fn find_pending_by_team(
        &self,
        team_id: TeamId,
    ) -> Result<Vec<TeamInvitationRow>, RepositoryError> {
        let rows = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            SELECT * FROM team_invitations
            WHERE team_id = $1 AND status = 'pending' AND expires_at > NOW()
            ORDER BY created_at DESC
            "#,
        )
        .bind(team_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Find all pending invitations for a player.
    pub async fn find_pending_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<TeamInvitationRow>, RepositoryError> {
        let rows = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            SELECT * FROM team_invitations
            WHERE player_id = $1 AND status = 'pending' AND expires_at > NOW()
            ORDER BY created_at DESC
            "#,
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Check if there's an existing pending invitation for this player/team.
    pub async fn find_existing_pending(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<Option<TeamInvitationRow>, RepositoryError> {
        let row = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            SELECT * FROM team_invitations
            WHERE team_id = $1 AND player_id = $2 AND status = 'pending' AND expires_at > NOW()
            "#,
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Update invitation status.
    pub async fn update_status(
        &self,
        id: TeamInvitationId,
        update: UpdateTeamInvitation,
    ) -> Result<TeamInvitationRow, RepositoryError> {
        let row = sqlx::query_as::<_, TeamInvitationRow>(
            r#"
            UPDATE team_invitations SET
                status = $2,
                response_message = COALESCE($3, response_message),
                responded_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id.as_uuid())
        .bind(&update.status)
        .bind(&update.response_message)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("TeamInvitation", id))?;

        Ok(row)
    }

    /// Cancel all pending invitations for a player on a specific team.
    pub async fn cancel_pending_for_player(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE team_invitations SET status = 'cancelled', responded_at = NOW()
            WHERE team_id = $1 AND player_id = $2 AND status = 'pending'
            "#,
        )
        .bind(team_id.as_uuid())
        .bind(player_id.as_uuid())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Count pending invitations for a player.
    pub async fn count_pending_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM team_invitations
            WHERE player_id = $1 AND status = 'pending' AND expires_at > NOW()
            "#,
        )
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }
}
