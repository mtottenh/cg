//! User and Player repositories.

use crate::DbPool;
use crate::entities::{
    NewPlayer, NewPlayerGameProfile, NewUser, PlayerGameProfileRow, PlayerRow, UpdatePlayer,
    UpdateUser, UserRow,
};
use crate::error::RepositoryError;
use portal_core::{PlayerId, UserId};
use sqlx::Row;
use uuid::Uuid;

/// Repository for user operations.
#[derive(Clone)]
pub struct UserRepository {
    pool: DbPool,
}

impl UserRepository {
    /// Create a new user repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a user by ID.
    pub async fn find_by_id(&self, id: UserId) -> Result<Option<UserRow>, RepositoryError> {
        let user = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await?;

        Ok(user)
    }

    /// Find a user by email.
    pub async fn find_by_email(&self, email: &str) -> Result<Option<UserRow>, RepositoryError> {
        let user = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE email = $1")
            .bind(email.to_lowercase())
            .fetch_optional(&self.pool)
            .await?;

        Ok(user)
    }

    /// Find a user by username.
    pub async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRow>, RepositoryError> {
        let user =
            sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE lower(username) = lower($1)")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?;

        Ok(user)
    }

    /// Create a new user.
    pub async fn create(&self, new_user: NewUser) -> Result<UserRow, RepositoryError> {
        let user = sqlx::query_as::<_, UserRow>(
            r"
            INSERT INTO users (username, email, password_hash)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(&new_user.username)
        .bind(new_user.email.to_lowercase())
        .bind(&new_user.password_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, &new_user.username))?;

        Ok(user)
    }

    /// Update a user.
    pub async fn update(&self, id: UserId, update: UpdateUser) -> Result<UserRow, RepositoryError> {
        let user = sqlx::query_as::<_, UserRow>(
            r"
            UPDATE users SET
                email = COALESCE($2, email),
                email_verified = COALESCE($3, email_verified),
                password_hash = COALESCE($4, password_hash),
                status = COALESCE($5, status),
                status_reason = COALESCE($6, status_reason),
                locale = COALESCE($7, locale),
                timezone = COALESCE($8, timezone),
                two_factor_enabled = COALESCE($9, two_factor_enabled)
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(update.email.map(|e| e.to_lowercase()))
        .bind(update.email_verified)
        .bind(update.password_hash)
        .bind(update.status)
        .bind(update.status_reason)
        .bind(update.locale)
        .bind(update.timezone)
        .bind(update.two_factor_enabled)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("User", id))?;

        Ok(user)
    }

    /// Check if a username is already taken.
    pub async fn username_exists(&self, username: &str) -> Result<bool, RepositoryError> {
        let row = sqlx::query("SELECT 1 FROM users WHERE lower(username) = lower($1)")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    /// Check if an email is already taken.
    pub async fn email_exists(&self, email: &str) -> Result<bool, RepositoryError> {
        let row = sqlx::query("SELECT 1 FROM users WHERE email = $1")
            .bind(email.to_lowercase())
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    /// Update last login timestamp.
    pub async fn update_last_login(&self, id: UserId) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// List users with optional filters.
    pub async fn list(
        &self,
        status: Option<&str>,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<UserRow>, RepositoryError> {
        let users = sqlx::query_as::<_, UserRow>(
            r"
            SELECT * FROM users
            WHERE ($1::text IS NULL OR status = $1)
              AND ($2::text IS NULL OR username ILIKE '%' || $2 || '%' OR email ILIKE '%' || $2 || '%')
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            ",
        )
        .bind(status)
        .bind(search)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(users)
    }

    /// Count users matching filters.
    pub async fn count(
        &self,
        status: Option<&str>,
        search: Option<&str>,
    ) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count FROM users
            WHERE ($1::text IS NULL OR status = $1)
              AND ($2::text IS NULL OR username ILIKE '%' || $2 || '%' OR email ILIKE '%' || $2 || '%')
            ",
        )
        .bind(status)
        .bind(search)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }

    /// Disable a user account.
    pub async fn disable(
        &self,
        id: UserId,
        reason: Option<&str>,
    ) -> Result<UserRow, RepositoryError> {
        let user = sqlx::query_as::<_, UserRow>(
            r"
            UPDATE users SET
                status = 'inactive',
                status_reason = $2,
                status_changed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(reason)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("User", id))?;

        Ok(user)
    }

    /// Enable a user account.
    pub async fn enable(&self, id: UserId) -> Result<UserRow, RepositoryError> {
        let user = sqlx::query_as::<_, UserRow>(
            r"
            UPDATE users SET
                status = 'active',
                status_reason = NULL,
                status_changed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("User", id))?;

        Ok(user)
    }

    /// Update user password.
    pub async fn update_password(
        &self,
        id: UserId,
        password_hash: &str,
    ) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            r"
            UPDATE users SET
                password_hash = $2,
                password_changed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(id.as_uuid())
        .bind(password_hash)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::not_found("User", id));
        }

        Ok(())
    }
}

/// Repository for player operations.
#[derive(Clone)]
pub struct PlayerRepository {
    pool: DbPool,
}

impl PlayerRepository {
    /// Create a new player repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a player by ID.
    pub async fn find_by_id(&self, id: PlayerId) -> Result<Option<PlayerRow>, RepositoryError> {
        let player = sqlx::query_as::<_, PlayerRow>("SELECT * FROM players WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await?;

        Ok(player)
    }

    /// Find a player by user ID.
    pub async fn find_by_user_id(
        &self,
        user_id: UserId,
    ) -> Result<Option<PlayerRow>, RepositoryError> {
        let player = sqlx::query_as::<_, PlayerRow>("SELECT * FROM players WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .fetch_optional(&self.pool)
            .await?;

        Ok(player)
    }

    /// Find a player by display name.
    pub async fn find_by_display_name(
        &self,
        display_name: &str,
    ) -> Result<Option<PlayerRow>, RepositoryError> {
        let player = sqlx::query_as::<_, PlayerRow>(
            "SELECT * FROM players WHERE display_name_normalized = lower($1)",
        )
        .bind(display_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(player)
    }

    /// Create a new player.
    pub async fn create(&self, new_player: NewPlayer) -> Result<PlayerRow, RepositoryError> {
        let player = sqlx::query_as::<_, PlayerRow>(
            r"
            INSERT INTO players (user_id, display_name, avatar_url, country_code)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            ",
        )
        .bind(new_player.user_id)
        .bind(&new_player.display_name)
        .bind(&new_player.avatar_url)
        .bind(&new_player.country_code)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, &new_player.display_name))?;

        Ok(player)
    }

    /// Update a player.
    pub async fn update(
        &self,
        id: PlayerId,
        update: UpdatePlayer,
    ) -> Result<PlayerRow, RepositoryError> {
        let player = sqlx::query_as::<_, PlayerRow>(
            r"
            UPDATE players SET
                display_name = COALESCE($2, display_name),
                avatar_url = COALESCE($3, avatar_url),
                banner_url = COALESCE($4, banner_url),
                bio = COALESCE($5, bio),
                country_code = COALESCE($6, country_code),
                region = COALESCE($7, region),
                timezone = COALESCE($8, timezone),
                social_links = COALESCE($9, social_links),
                privacy_settings = COALESCE($10, privacy_settings),
                notification_settings = COALESCE($11, notification_settings),
                steam_id = COALESCE($12, steam_id),
                steam_id_64 = COALESCE($13, steam_id_64)
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(update.display_name)
        .bind(update.avatar_url)
        .bind(update.banner_url)
        .bind(update.bio)
        .bind(update.country_code)
        .bind(update.region)
        .bind(update.timezone)
        .bind(update.social_links)
        .bind(update.privacy_settings)
        .bind(update.notification_settings)
        .bind(update.steam_id)
        .bind(update.steam_id_64)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("Player", id))?;

        Ok(player)
    }

    /// Search players by display name.
    pub async fn search(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerRow>, RepositoryError> {
        let players = sqlx::query_as::<_, PlayerRow>(
            r"
            SELECT * FROM players
            WHERE display_name_normalized LIKE $1 || '%'
            ORDER BY display_name_normalized
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(query.to_lowercase())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(players)
    }

    /// Count total players matching a search query.
    pub async fn count_search(&self, query: &str) -> Result<i64, RepositoryError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM players WHERE display_name_normalized LIKE $1 || '%'",
        )
        .bind(query.to_lowercase())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }

    /// List players with optional filters.
    pub async fn list(
        &self,
        search: Option<&str>,
        country: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerRow>, RepositoryError> {
        let players = sqlx::query_as::<_, PlayerRow>(
            r"
            SELECT * FROM players
            WHERE ($1::text IS NULL OR display_name_normalized LIKE '%' || lower($1) || '%')
              AND ($2::text IS NULL OR country_code = $2)
            ORDER BY display_name_normalized
            LIMIT $3 OFFSET $4
            ",
        )
        .bind(search)
        .bind(country)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(players)
    }
}

/// Repository for player game profile operations.
#[derive(Clone)]
pub struct PlayerGameProfileRepository {
    pool: DbPool,
}

impl PlayerGameProfileRepository {
    /// Create a new player game profile repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a profile by player and game.
    pub async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: Uuid,
    ) -> Result<Option<PlayerGameProfileRow>, RepositoryError> {
        let profile = sqlx::query_as::<_, PlayerGameProfileRow>(
            "SELECT * FROM player_game_profiles WHERE player_id = $1 AND game_id = $2",
        )
        .bind(player_id.as_uuid())
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(profile)
    }

    /// List all profiles for a player.
    pub async fn list_by_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerGameProfileRow>, RepositoryError> {
        let profiles = sqlx::query_as::<_, PlayerGameProfileRow>(
            "SELECT * FROM player_game_profiles WHERE player_id = $1 ORDER BY matches_played DESC",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(profiles)
    }

    /// Create a new player game profile.
    pub async fn create(
        &self,
        new_profile: NewPlayerGameProfile,
    ) -> Result<PlayerGameProfileRow, RepositoryError> {
        let profile = sqlx::query_as::<_, PlayerGameProfileRow>(
            r"
            INSERT INTO player_game_profiles (player_id, game_id)
            VALUES ($1, $2)
            RETURNING *
            ",
        )
        .bind(new_profile.player_id)
        .bind(new_profile.game_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "player game profile"))?;

        Ok(profile)
    }

    /// Reset a player's rating to default values.
    pub async fn reset_rating(
        &self,
        player_id: PlayerId,
        game_id: Uuid,
    ) -> Result<PlayerGameProfileRow, RepositoryError> {
        let profile = sqlx::query_as::<_, PlayerGameProfileRow>(
            r"
            UPDATE player_game_profiles SET
                rating = 1500,
                rating_deviation = 350,
                volatility = 0.06,
                updated_at = NOW()
            WHERE player_id = $1 AND game_id = $2
            RETURNING *
            ",
        )
        .bind(player_id.as_uuid())
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            RepositoryError::not_found("PlayerGameProfile", format!("{player_id}/{game_id}"))
        })?;

        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{NewPlayer, NewPlayerGameProfile, UpdatePlayer, UpdateUser};
    use portal_test::database::TestDb;
    use uuid::Uuid;

    // ===========================================
    // UserRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_create_user() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            password_hash: Some("hashed_password".to_string()),
        };

        let user = repo.create(new_user).await.unwrap();

        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.status, "active"); // Default from migration
        assert!(!user.email_verified);
    }

    #[tokio::test]
    async fn test_find_user_by_id() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "findbyid".to_string(),
            email: "findbyid@example.com".to_string(),
            password_hash: None,
        };
        let created = repo.create(new_user).await.unwrap();

        let found = repo.find_by_id(UserId::from(created.id)).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "findbyid");

        // Test not found
        let not_found = repo.find_by_id(UserId::from(Uuid::nil())).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_user_by_email() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "emailtest".to_string(),
            email: "EmailTest@Example.COM".to_string(),
            password_hash: None,
        };
        repo.create(new_user).await.unwrap();

        // Email search should be case-insensitive
        let found = repo.find_by_email("emailtest@example.com").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "emailtest");

        let not_found = repo.find_by_email("notexist@example.com").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_user_by_username() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "TestUserName".to_string(),
            email: "username@example.com".to_string(),
            password_hash: None,
        };
        repo.create(new_user).await.unwrap();

        // Username search should be case-insensitive
        let found = repo.find_by_username("testusername").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().email, "username@example.com");

        let not_found = repo.find_by_username("notexist").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_update_user() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "updatetest".to_string(),
            email: "update@example.com".to_string(),
            password_hash: None,
        };
        let created = repo.create(new_user).await.unwrap();

        let update = UpdateUser {
            email: Some("newemail@example.com".to_string()),
            email_verified: Some(true),
            status: Some("active".to_string()),
            locale: Some("fr-FR".to_string()),
            ..Default::default()
        };

        let updated = repo.update(UserId::from(created.id), update).await.unwrap();
        assert_eq!(updated.email, "newemail@example.com");
        assert!(updated.email_verified);
        assert_eq!(updated.status, "active");
        assert_eq!(updated.locale, Some("fr-FR".to_string()));
    }

    #[tokio::test]
    async fn test_username_exists() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "ExistingUser".to_string(),
            email: "existing@example.com".to_string(),
            password_hash: None,
        };
        repo.create(new_user).await.unwrap();

        // Should be case-insensitive
        assert!(repo.username_exists("existinguser").await.unwrap());
        assert!(repo.username_exists("EXISTINGUSER").await.unwrap());
        assert!(!repo.username_exists("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_email_exists() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "emailexists".to_string(),
            email: "Existing@Example.COM".to_string(),
            password_hash: None,
        };
        repo.create(new_user).await.unwrap();

        // Should be case-insensitive (stored as lowercase)
        assert!(repo.email_exists("existing@example.com").await.unwrap());
        assert!(!repo.email_exists("notexist@example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_disable_enable_user() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "disabletest".to_string(),
            email: "disable@example.com".to_string(),
            password_hash: None,
        };
        let created = repo.create(new_user).await.unwrap();
        let user_id = UserId::from(created.id);

        // Disable with reason
        let disabled = repo.disable(user_id, Some("Test reason")).await.unwrap();
        assert_eq!(disabled.status, "inactive");
        assert_eq!(disabled.status_reason, Some("Test reason".to_string()));

        // Enable
        let enabled = repo.enable(user_id).await.unwrap();
        assert_eq!(enabled.status, "active");
        assert!(enabled.status_reason.is_none());
    }

    #[tokio::test]
    async fn test_list_users_with_filters() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        // Create multiple users
        for i in 1..=5 {
            let new_user = NewUser {
                username: format!("listuser{i}"),
                email: format!("list{i}@example.com"),
                password_hash: None,
            };
            repo.create(new_user).await.unwrap();
        }

        // Search by username (filters to only our test users)
        let searched = repo.list(None, Some("listuser"), 10, 0).await.unwrap();
        assert_eq!(searched.len(), 5);

        // Search by specific username
        let searched = repo.list(None, Some("listuser3"), 10, 0).await.unwrap();
        assert_eq!(searched.len(), 1);

        // Test pagination with filter
        let page1 = repo.list(None, Some("listuser"), 2, 0).await.unwrap();
        let page2 = repo.list(None, Some("listuser"), 2, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 2);
    }

    #[tokio::test]
    async fn test_update_password() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "pwdtest".to_string(),
            email: "pwd@example.com".to_string(),
            password_hash: Some("old_hash".to_string()),
        };
        let created = repo.create(new_user).await.unwrap();

        repo.update_password(UserId::from(created.id), "new_hash")
            .await
            .unwrap();

        let updated = repo
            .find_by_id(UserId::from(created.id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.password_hash, Some("new_hash".to_string()));
        assert!(updated.password_changed_at.is_some());
    }

    #[tokio::test]
    async fn test_update_last_login() {
        let db = TestDb::new().await;
        let repo = UserRepository::new(db.pool.clone());

        let new_user = NewUser {
            username: "logintest".to_string(),
            email: "login@example.com".to_string(),
            password_hash: None,
        };
        let created = repo.create(new_user).await.unwrap();
        assert!(created.last_login_at.is_none());

        repo.update_last_login(UserId::from(created.id))
            .await
            .unwrap();

        let updated = repo
            .find_by_id(UserId::from(created.id))
            .await
            .unwrap()
            .unwrap();
        assert!(updated.last_login_at.is_some());
    }

    // ===========================================
    // PlayerRepository Tests
    // ===========================================

    async fn create_test_user(repo: &UserRepository, suffix: &str) -> UserRow {
        let new_user = NewUser {
            username: format!("playeruser{suffix}"),
            email: format!("player{suffix}@example.com"),
            password_hash: None,
        };
        repo.create(new_user).await.unwrap()
    }

    #[tokio::test]
    async fn test_create_player() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());

        let user = create_test_user(&user_repo, "create").await;

        let new_player = NewPlayer {
            user_id: user.id,
            display_name: "TestPlayer".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
            country_code: Some("US".to_string()),
        };

        let player = player_repo.create(new_player).await.unwrap();
        assert_eq!(player.display_name, "TestPlayer");
        assert_eq!(player.display_name_normalized, "testplayer");
        assert_eq!(player.country_code, Some("US".to_string()));
    }

    #[tokio::test]
    async fn test_find_player_by_user_id() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());

        let user = create_test_user(&user_repo, "finduser").await;

        let new_player = NewPlayer {
            user_id: user.id,
            display_name: "FindByUserPlayer".to_string(),
            avatar_url: None,
            country_code: None,
        };
        player_repo.create(new_player).await.unwrap();

        let found = player_repo
            .find_by_user_id(UserId::from(user.id))
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().display_name, "FindByUserPlayer");

        // Not found
        let not_found = player_repo
            .find_by_user_id(UserId::from(Uuid::nil()))
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_player_search() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());

        // Create multiple players
        for i in 1..=3 {
            let user = create_test_user(&user_repo, &format!("search{i}")).await;
            let new_player = NewPlayer {
                user_id: user.id,
                display_name: format!("SearchPlayer{i}"),
                avatar_url: None,
                country_code: None,
            };
            player_repo.create(new_player).await.unwrap();
        }

        // Search by prefix
        let results = player_repo.search("searchplayer", 10, 0).await.unwrap();
        assert_eq!(results.len(), 3);

        // Specific search
        let results = player_repo.search("searchplayer1", 10, 0).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_update_player() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());

        let user = create_test_user(&user_repo, "update").await;
        let new_player = NewPlayer {
            user_id: user.id,
            display_name: "OriginalName".to_string(),
            avatar_url: None,
            country_code: None,
        };
        let player = player_repo.create(new_player).await.unwrap();

        let update = UpdatePlayer {
            display_name: Some("UpdatedName".to_string()),
            bio: Some("Test bio".to_string()),
            country_code: Some("CA".to_string()),
            ..Default::default()
        };

        let updated = player_repo
            .update(PlayerId::from(player.id), update)
            .await
            .unwrap();
        assert_eq!(updated.display_name, "UpdatedName");
        assert_eq!(updated.bio, Some("Test bio".to_string()));
        assert_eq!(updated.country_code, Some("CA".to_string()));
    }

    // ===========================================
    // PlayerGameProfileRepository Tests
    // ===========================================

    async fn create_test_player(
        user_repo: &UserRepository,
        player_repo: &PlayerRepository,
        suffix: &str,
    ) -> (UserRow, PlayerRow) {
        let user = create_test_user(user_repo, suffix).await;
        let new_player = NewPlayer {
            user_id: user.id,
            display_name: format!("Player{suffix}"),
            avatar_url: None,
            country_code: None,
        };
        let player = player_repo.create(new_player).await.unwrap();
        (user, player)
    }

    async fn create_test_game(pool: &DbPool, slug: &str) -> Uuid {
        let game_id = Uuid::now_v7();
        // fetch_one + RETURNING: on slug conflict the existing row keeps its
        // own id, so returning the locally generated uuid would violate FKs.
        let (game_id,): (Uuid,) = sqlx::query_as(
            r"
            INSERT INTO games (id, slug, display_name, short_name, plugin_id, plugin_version,
                              team_size_min, team_size_max, team_size_default)
            VALUES ($1, $2, $3, $4, $5, '1.0.0', 1, 5, 5)
            ON CONFLICT (slug) DO UPDATE SET slug = EXCLUDED.slug
            RETURNING id
            ",
        )
        .bind(game_id)
        .bind(slug)
        .bind(format!("{slug} Game"))
        .bind(slug)
        .bind(format!("{slug}_plugin"))
        .fetch_one(pool)
        .await
        .unwrap();
        game_id
    }

    #[tokio::test]
    async fn test_create_player_game_profile() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());
        let profile_repo = PlayerGameProfileRepository::new(db.pool.clone());

        let (_, player) = create_test_player(&user_repo, &player_repo, "profile").await;
        let game_id = create_test_game(&db.pool, "cs2").await;

        let new_profile = NewPlayerGameProfile {
            player_id: player.id,
            game_id,
        };

        let profile = profile_repo.create(new_profile).await.unwrap();
        assert_eq!(profile.player_id, player.id);
        assert_eq!(profile.game_id, game_id);
        assert_eq!(profile.rating, 1500); // Default Glicko-2 rating
        assert_eq!(profile.rating_deviation, 350);
        assert_eq!(profile.matches_played, 0);
    }

    #[tokio::test]
    async fn test_reset_rating() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());
        let profile_repo = PlayerGameProfileRepository::new(db.pool.clone());

        let (_, player) = create_test_player(&user_repo, &player_repo, "reset").await;
        let game_id = create_test_game(&db.pool, "aoe4").await;

        let new_profile = NewPlayerGameProfile {
            player_id: player.id,
            game_id,
        };
        profile_repo.create(new_profile).await.unwrap();

        // Simulate rating change
        sqlx::query("UPDATE player_game_profiles SET rating = 1800, rating_deviation = 100 WHERE player_id = $1")
            .bind(player.id)
            .execute(&db.pool)
            .await
            .unwrap();

        // Reset
        let reset = profile_repo
            .reset_rating(PlayerId::from(player.id), game_id)
            .await
            .unwrap();
        assert_eq!(reset.rating, 1500);
        assert_eq!(reset.rating_deviation, 350);
    }

    #[tokio::test]
    async fn test_find_profile_by_player_and_game() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());
        let profile_repo = PlayerGameProfileRepository::new(db.pool.clone());

        let (_, player) = create_test_player(&user_repo, &player_repo, "findprofile").await;
        let game_id = create_test_game(&db.pool, "rl").await;

        // Not found initially
        let not_found = profile_repo
            .find_by_player_and_game(PlayerId::from(player.id), game_id)
            .await
            .unwrap();
        assert!(not_found.is_none());

        // Create profile
        let new_profile = NewPlayerGameProfile {
            player_id: player.id,
            game_id,
        };
        profile_repo.create(new_profile).await.unwrap();

        // Now found
        let found = profile_repo
            .find_by_player_and_game(PlayerId::from(player.id), game_id)
            .await
            .unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_list_profiles_by_player() {
        let db = TestDb::new().await;
        let user_repo = UserRepository::new(db.pool.clone());
        let player_repo = PlayerRepository::new(db.pool.clone());
        let profile_repo = PlayerGameProfileRepository::new(db.pool.clone());

        let (_, player) = create_test_player(&user_repo, &player_repo, "listprofiles").await;
        let game1_id = create_test_game(&db.pool, "game1").await;
        let game2_id = create_test_game(&db.pool, "game2").await;

        for game_id in [game1_id, game2_id] {
            let new_profile = NewPlayerGameProfile {
                player_id: player.id,
                game_id,
            };
            profile_repo.create(new_profile).await.unwrap();
        }

        let profiles = profile_repo
            .list_by_player(PlayerId::from(player.id))
            .await
            .unwrap();
        assert_eq!(profiles.len(), 2);
    }
}
