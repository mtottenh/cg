//! Player builder for tests.

use chrono::Utc;
use fake::Fake;
use fake::faker::name::en::Name;
use portal_db::DbPool;
use portal_db::entities::PlayerRow;
use uuid::Uuid;

use super::UserBuilder;

/// Builder for creating test players.
#[derive(Debug, Clone)]
pub struct PlayerBuilder {
    id: Option<Uuid>,
    user_id: Option<Uuid>,
    display_name: Option<String>,
    avatar_url: Option<String>,
    bio: Option<String>,
    country_code: Option<String>,
    steam_id: Option<String>,
    // If true, create a user automatically
    create_user: bool,
}

impl Default for PlayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerBuilder {
    /// Create a new player builder with random defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: None,
            user_id: None,
            display_name: None,
            avatar_url: None,
            bio: None,
            country_code: None,
            steam_id: None,
            create_user: true,
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Link to an existing user.
    #[must_use]
    pub const fn user_id(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self.create_user = false;
        self
    }

    /// Set the display name.
    #[must_use]
    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the avatar URL.
    #[must_use]
    pub fn avatar(mut self, url: impl Into<String>) -> Self {
        self.avatar_url = Some(url.into());
        self
    }

    /// Set the bio.
    #[must_use]
    pub fn bio(mut self, bio: impl Into<String>) -> Self {
        self.bio = Some(bio.into());
        self
    }

    /// Set the country code.
    #[must_use]
    pub fn country(mut self, code: impl Into<String>) -> Self {
        self.country_code = Some(code.into());
        self
    }

    /// Set the Steam ID.
    #[must_use]
    pub fn steam_id(mut self, steam_id: impl Into<String>) -> Self {
        self.steam_id = Some(steam_id.into());
        self
    }

    /// Build an in-memory player (not persisted).
    /// Note: This requires a `user_id` to be set.
    #[must_use]
    pub fn build(self, user_id: Uuid) -> PlayerRow {
        let now = Utc::now();
        let display_name = self.display_name.unwrap_or_else(|| Name().fake());

        PlayerRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            user_id,
            display_name: display_name.clone(),
            display_name_normalized: display_name.to_lowercase(),
            avatar_url: self.avatar_url,
            banner_url: None,
            bio: self.bio,
            country_code: self.country_code,
            region: None,
            timezone: None,
            social_links: serde_json::json!({}),
            privacy_settings: serde_json::json!({
                "show_online_status": true,
                "show_match_history": true,
                "show_statistics": true,
                "allow_friend_requests": true,
                "allow_team_invites": true
            }),
            notification_settings: serde_json::json!({}),
            ui_preferences: serde_json::json!({}),
            steam_id: self.steam_id,
            steam_id_64: None,
            steam_profile: None,
            looking_for_team: false,
            featured_badge_id: None,
            title: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Build and persist the player to the database.
    ///
    /// If no `user_id` is set, creates a new user automatically.
    pub async fn build_persisted(self, pool: &DbPool) -> PlayerRow {
        // Create user if needed
        let user_id = if let Some(id) = self.user_id {
            id
        } else {
            let user = UserBuilder::new().build_persisted(pool).await;
            user.id
        };

        let player = self.build(user_id);

        sqlx::query_as::<_, PlayerRow>(
            r"
            INSERT INTO players (id, user_id, display_name, avatar_url, bio, country_code, steam_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
        )
        .bind(player.id)
        .bind(player.user_id)
        .bind(&player.display_name)
        .bind(&player.avatar_url)
        .bind(&player.bio)
        .bind(&player.country_code)
        .bind(&player.steam_id)
        .fetch_one(pool)
        .await
        .expect("Failed to create test player")
    }
}
