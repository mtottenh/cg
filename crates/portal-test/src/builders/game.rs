//! Game builder for tests.

use fake::faker::company::en::CompanyName;
use fake::Fake;
use portal_db::entities::GameRow;
use portal_db::repositories::GameRepository;
use portal_db::DbPool;
use uuid::Uuid;

/// Builder for creating test games.
#[derive(Debug, Clone)]
pub struct GameBuilder {
    id: Option<Uuid>,
    slug: Option<String>,
    display_name: Option<String>,
    short_name: Option<String>,
    description: Option<String>,
    plugin_id: Option<String>,
    plugin_version: String,
    team_size_min: i32,
    team_size_max: i32,
    team_size_default: i32,
    status: String,
}

impl Default for GameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl GameBuilder {
    /// Create a new game builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            slug: None,
            display_name: None,
            short_name: None,
            description: None,
            plugin_id: None,
            plugin_version: "1.0.0".to_string(),
            team_size_min: 1,
            team_size_max: 5,
            team_size_default: 5,
            status: "active".to_string(),
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the game slug.
    #[must_use]
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// Set the display name.
    #[must_use]
    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the short name.
    #[must_use]
    pub fn short_name(mut self, name: impl Into<String>) -> Self {
        self.short_name = Some(name.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the plugin ID.
    #[must_use]
    pub fn plugin_id(mut self, plugin_id: impl Into<String>) -> Self {
        self.plugin_id = Some(plugin_id.into());
        self
    }

    /// Set the plugin version.
    #[must_use]
    pub fn plugin_version(mut self, version: impl Into<String>) -> Self {
        self.plugin_version = version.into();
        self
    }

    /// Set team size constraints.
    #[must_use]
    pub const fn team_size(mut self, min: i32, max: i32, default: i32) -> Self {
        self.team_size_min = min;
        self.team_size_max = max;
        self.team_size_default = default;
        self
    }

    /// Set the game status.
    #[must_use]
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    /// Set status to active (default).
    #[must_use]
    pub fn active(mut self) -> Self {
        self.status = "active".to_string();
        self
    }

    /// Set status to maintenance.
    #[must_use]
    pub fn maintenance(mut self) -> Self {
        self.status = "maintenance".to_string();
        self
    }

    /// Build and persist the game to the database using repository.
    pub async fn build_persisted(self, pool: &DbPool) -> GameRow {
        let repo = GameRepository::new(pool.clone());

        let display_name = self.display_name.unwrap_or_else(|| {
            let company: String = CompanyName().fake();
            format!("{company} Game")
        });
        let slug = self.slug.unwrap_or_else(|| slug::slugify(&display_name));
        let plugin_id = self.plugin_id.unwrap_or_else(|| format!("{slug}_plugin"));

        let new_game = portal_db::entities::NewGame {
            slug,
            display_name,
            short_name: self.short_name,
            description: self.description,
            plugin_id,
            plugin_version: self.plugin_version,
            team_size_min: self.team_size_min,
            team_size_max: self.team_size_max,
            team_size_default: self.team_size_default,
        };

        repo.create(new_game)
            .await
            .expect("Failed to create test game")
    }
}
