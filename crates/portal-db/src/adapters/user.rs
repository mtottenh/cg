//! User and Player repository adapters.

use crate::entities::{PlayerRow, UserRow};
use crate::DbPool;
use async_trait::async_trait;
use portal_core::{DomainError, PlayerId, UserId};
use portal_domain::entities::user::UserStatus as DomainUserStatus;
use portal_domain::entities::{Player, SocialLinks, User, UserWithCredentials};
use portal_domain::repositories::{
    CreatePlayer, CreateUser, PlayerRepository, UpdatePlayer, UserRepository,
};
use sqlx::Row;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        Self {
            id: UserId::from(row.id),
            username: row.username,
            email: row.email,
            email_verified: row.email_verified,
            status: row.status.parse().unwrap_or_default(),
            locale: row.locale.unwrap_or_else(|| "en-US".to_string()),
            timezone: row.timezone.unwrap_or_else(|| "UTC".to_string()),
            two_factor_enabled: row.two_factor_enabled,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_login_at: row.last_login_at,
        }
    }
}

impl From<UserRow> for UserWithCredentials {
    fn from(row: UserRow) -> Self {
        Self {
            id: UserId::from(row.id),
            username: row.username,
            email: row.email,
            password_hash: row.password_hash,
            status: row.status.parse().unwrap_or(DomainUserStatus::Active),
        }
    }
}

impl From<PlayerRow> for Player {
    fn from(row: PlayerRow) -> Self {
        // Parse social_links from JSONB, falling back to default if parsing fails
        let social_links: SocialLinks =
            serde_json::from_value(row.social_links).unwrap_or_default();

        Self {
            id: PlayerId::from(row.id),
            user_id: UserId::from(row.user_id),
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            banner_url: row.banner_url,
            bio: row.bio,
            country_code: row.country_code,
            region: row.region,
            timezone: row.timezone,
            social_links,
            steam_id: row.steam_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// User Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `UserRepository` trait.
#[derive(Clone)]
pub struct PgUserRepository {
    pool: DbPool,
}

impl PgUserRepository {
    /// Create a new `PostgreSQL` user repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, DomainError> {
        let user = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(user.map(User::from))
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DomainError> {
        let user = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE email = $1")
            .bind(email.to_lowercase())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(user.map(User::from))
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        let user =
            sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE lower(username) = lower($1)")
                .bind(username)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(user.map(User::from))
    }

    async fn create(&self, cmd: CreateUser) -> Result<User, DomainError> {
        let user = match cmd.id {
            Some(id) => {
                sqlx::query_as::<_, UserRow>(
                    r"
                    INSERT INTO users (id, username, email, password_hash)
                    VALUES ($1, $2, $3, $4)
                    RETURNING *
                    ",
                )
                .bind(id.as_uuid())
                .bind(&cmd.username)
                .bind(cmd.email.to_lowercase())
                .bind(&cmd.password_hash)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?
            }
            None => {
                sqlx::query_as::<_, UserRow>(
                    r"
                    INSERT INTO users (username, email, password_hash)
                    VALUES ($1, $2, $3)
                    RETURNING *
                    ",
                )
                .bind(&cmd.username)
                .bind(cmd.email.to_lowercase())
                .bind(&cmd.password_hash)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?
            }
        };

        Ok(User::from(user))
    }

    async fn username_exists(&self, username: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM users WHERE lower(username) = lower($1)")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn email_exists(&self, email: &str) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 FROM users WHERE email = $1")
            .bind(email.to_lowercase())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn find_for_auth(
        &self,
        username_or_email: &str,
    ) -> Result<Option<UserWithCredentials>, DomainError> {
        // Try to find by email first (if input contains @), otherwise by username
        let user = sqlx::query_as::<_, UserRow>(
            r"
            SELECT * FROM users
            WHERE email = $1 OR lower(username) = lower($1)
            ",
        )
        .bind(username_or_email.to_lowercase())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(user.map(UserWithCredentials::from))
    }

    async fn update_last_login(&self, id: UserId) -> Result<(), DomainError> {
        sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}

// =============================================================================
// Player Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `PlayerRepository` trait.
#[derive(Clone)]
pub struct PgPlayerRepository {
    pool: DbPool,
}

impl PgPlayerRepository {
    /// Create a new `PostgreSQL` player repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlayerRepository for PgPlayerRepository {
    async fn find_by_id(&self, id: PlayerId) -> Result<Option<Player>, DomainError> {
        let player = sqlx::query_as::<_, PlayerRow>("SELECT * FROM players WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(player.map(Player::from))
    }

    async fn find_by_user_id(&self, user_id: UserId) -> Result<Option<Player>, DomainError> {
        let player = sqlx::query_as::<_, PlayerRow>("SELECT * FROM players WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(player.map(Player::from))
    }

    async fn find_by_display_name(&self, name: &str) -> Result<Option<Player>, DomainError> {
        let player = sqlx::query_as::<_, PlayerRow>(
            "SELECT * FROM players WHERE display_name_normalized = lower($1)",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(player.map(Player::from))
    }

    async fn create(&self, cmd: CreatePlayer) -> Result<Player, DomainError> {
        let player = sqlx::query_as::<_, PlayerRow>(
            r"
            INSERT INTO players (id, user_id, display_name)
            VALUES ($1, $2, $3)
            RETURNING *
            ",
        )
        .bind(cmd.id.as_uuid())
        .bind(cmd.user_id.as_uuid())
        .bind(&cmd.display_name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Player::from(player))
    }

    async fn search(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Player>, DomainError> {
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
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(players.into_iter().map(Player::from).collect())
    }

    async fn count_search(&self, query: &str) -> Result<i64, DomainError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM players WHERE display_name_normalized LIKE $1 || '%'",
        )
        .bind(query.to_lowercase())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.get("count"))
    }

    async fn update(&self, id: PlayerId, cmd: UpdatePlayer) -> Result<Player, DomainError> {
        // Build dynamic update query
        let mut set_clauses = Vec::new();
        let mut param_index = 2; // $1 is the ID

        if cmd.display_name.is_some() {
            set_clauses.push(format!("display_name = ${param_index}"));
            param_index += 1;
        }
        if cmd.avatar_url.is_some() {
            set_clauses.push(format!("avatar_url = ${param_index}"));
            param_index += 1;
        }
        if cmd.banner_url.is_some() {
            set_clauses.push(format!("banner_url = ${param_index}"));
            param_index += 1;
        }
        if cmd.bio.is_some() {
            set_clauses.push(format!("bio = ${param_index}"));
            param_index += 1;
        }
        if cmd.country_code.is_some() {
            set_clauses.push(format!("country_code = ${param_index}"));
            param_index += 1;
        }
        if cmd.region.is_some() {
            set_clauses.push(format!("region = ${param_index}"));
            param_index += 1;
        }
        if cmd.timezone.is_some() {
            set_clauses.push(format!("timezone = ${param_index}"));
            param_index += 1;
        }
        if cmd.social_links.is_some() {
            set_clauses.push(format!("social_links = ${param_index}"));
            param_index += 1;
        }
        if cmd.steam_id.is_some() {
            set_clauses.push(format!("steam_id = ${param_index}"));
            param_index += 1;
            set_clauses.push(format!("steam_id_64 = ${param_index}"));
            param_index += 1;
        }

        if set_clauses.is_empty() {
            // No updates to apply, just return the current player
            return self
                .find_by_id(id)
                .await?
                .ok_or_else(|| DomainError::PlayerNotFound(id.to_string()));
        }

        // Always update updated_at
        set_clauses.push("updated_at = NOW()".to_string());

        let query = format!(
            "UPDATE players SET {} WHERE id = $1 RETURNING *",
            set_clauses.join(", ")
        );

        // Build query with dynamic parameters
        let mut query_builder = sqlx::query_as::<_, PlayerRow>(&query).bind(id.as_uuid());

        if let Some(display_name) = &cmd.display_name {
            query_builder = query_builder.bind(display_name);
        }
        if let Some(avatar_url) = &cmd.avatar_url {
            query_builder = query_builder.bind(avatar_url);
        }
        if let Some(banner_url) = &cmd.banner_url {
            query_builder = query_builder.bind(banner_url);
        }
        if let Some(bio) = &cmd.bio {
            query_builder = query_builder.bind(bio);
        }
        if let Some(country_code) = &cmd.country_code {
            query_builder = query_builder.bind(country_code);
        }
        if let Some(region) = &cmd.region {
            query_builder = query_builder.bind(region);
        }
        if let Some(timezone) = &cmd.timezone {
            query_builder = query_builder.bind(timezone);
        }
        if let Some(social_links) = &cmd.social_links {
            let json_value = serde_json::to_value(social_links)
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            query_builder = query_builder.bind(json_value);
        }
        if let Some(steam_id) = &cmd.steam_id {
            query_builder = query_builder.bind(steam_id);
            let steam_id_64: i64 = steam_id.parse().unwrap_or(0);
            query_builder = query_builder.bind(steam_id_64);
        }

        let player = query_builder
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::PlayerNotFound(id.to_string()))?;

        Ok(Player::from(player))
    }
}
