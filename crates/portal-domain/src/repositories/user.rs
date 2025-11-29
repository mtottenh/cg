//! User and Player repository traits.

use crate::entities::{Player, SocialLinks, User, UserWithCredentials};
use async_trait::async_trait;
use portal_core::{DomainError, PlayerId, UserId};

/// Repository trait for user operations.
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Find a user by ID.
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, DomainError>;

    /// Find a user by email.
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DomainError>;

    /// Find a user by username.
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError>;

    /// Find a user for authentication by username or email.
    /// Returns user data including password hash for credential verification.
    async fn find_for_auth(&self, username_or_email: &str)
        -> Result<Option<UserWithCredentials>, DomainError>;

    /// Create a new user.
    async fn create(&self, cmd: CreateUser) -> Result<User, DomainError>;

    /// Check if a username is taken.
    async fn username_exists(&self, username: &str) -> Result<bool, DomainError>;

    /// Check if an email is taken.
    async fn email_exists(&self, email: &str) -> Result<bool, DomainError>;

    /// Update the last login timestamp for a user.
    async fn update_last_login(&self, id: UserId) -> Result<(), DomainError>;
}

/// Data for creating a new user.
#[derive(Debug, Clone)]
pub struct CreateUser {
    /// Optional ID (if None, a new UUID will be generated).
    pub id: Option<UserId>,
    /// Username.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Hashed password.
    pub password_hash: String,
}

/// Repository trait for player operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PlayerRepository: Send + Sync {
    /// Find a player by ID.
    async fn find_by_id(&self, id: PlayerId) -> Result<Option<Player>, DomainError>;

    /// Find a player by user ID.
    async fn find_by_user_id(&self, user_id: UserId) -> Result<Option<Player>, DomainError>;

    /// Find a player by display name.
    async fn find_by_display_name(&self, name: &str) -> Result<Option<Player>, DomainError>;

    /// Create a new player.
    async fn create(&self, cmd: CreatePlayer) -> Result<Player, DomainError>;

    /// Search players by display name.
    async fn search(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Player>, DomainError>;

    /// Count total players matching a search query.
    async fn count_search(&self, query: &str) -> Result<i64, DomainError>;

    /// Update a player profile.
    async fn update(&self, id: PlayerId, cmd: UpdatePlayer) -> Result<Player, DomainError>;
}

/// Data for creating a new player.
#[derive(Debug, Clone)]
pub struct CreatePlayer {
    /// Player ID (should match the user ID for 1:1 mapping).
    pub id: PlayerId,
    /// User ID.
    pub user_id: UserId,
    /// Display name.
    pub display_name: String,
}

/// Data for updating a player profile.
#[derive(Debug, Clone, Default)]
pub struct UpdatePlayer {
    /// Display name.
    pub display_name: Option<String>,
    /// Avatar URL.
    pub avatar_url: Option<String>,
    /// Banner URL.
    pub banner_url: Option<String>,
    /// Bio.
    pub bio: Option<String>,
    /// Country code (ISO 3166-1 alpha-2).
    pub country_code: Option<String>,
    /// Region.
    pub region: Option<String>,
    /// Timezone.
    pub timezone: Option<String>,
    /// Social media links.
    pub social_links: Option<SocialLinks>,
}

impl UpdatePlayer {
    /// Check if there are any updates to apply.
    #[must_use]
    pub fn has_updates(&self) -> bool {
        self.display_name.is_some()
            || self.avatar_url.is_some()
            || self.banner_url.is_some()
            || self.bio.is_some()
            || self.country_code.is_some()
            || self.region.is_some()
            || self.timezone.is_some()
            || self.social_links.is_some()
    }
}
