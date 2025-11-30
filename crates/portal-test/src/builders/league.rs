//! League builder for tests.

use chrono::Utc;
use fake::faker::company::en::CompanyName;
use fake::Fake;
use portal_db::entities::LeagueRow;
use portal_db::DbPool;
use uuid::Uuid;

/// Builder for creating test leagues.
#[derive(Debug, Clone)]
pub struct LeagueBuilder {
    id: Option<Uuid>,
    game_id: Option<Uuid>,
    name: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    logo_url: Option<String>,
    access_type: String,
    status: String,
    format_type: String,
    default_team_size_min: Option<i32>,
    default_team_size_max: Option<i32>,
    default_max_substitutes: Option<i32>,
    settings: serde_json::Value,
    created_by: Option<Uuid>,
}

impl Default for LeagueBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LeagueBuilder {
    /// Create a new league builder with random defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            game_id: None,
            name: None,
            slug: None,
            description: None,
            logo_url: None,
            access_type: "open".to_string(),
            status: "active".to_string(),
            format_type: "team".to_string(),
            default_team_size_min: Some(5),
            default_team_size_max: Some(7),
            default_max_substitutes: Some(2),
            settings: serde_json::json!({}),
            created_by: None,
        }
    }

    /// Set format to team (default).
    #[must_use]
    pub fn team_format(mut self) -> Self {
        self.format_type = "team".to_string();
        self
    }

    /// Set format to individual (1v1).
    #[must_use]
    pub fn individual_format(mut self) -> Self {
        self.format_type = "individual".to_string();
        self.default_team_size_min = None;
        self.default_team_size_max = None;
        self.default_max_substitutes = None;
        self
    }

    /// Set default team size constraints.
    #[must_use]
    pub const fn team_size(mut self, min: i32, max: i32) -> Self {
        self.default_team_size_min = Some(min);
        self.default_team_size_max = Some(max);
        self
    }

    /// Set default max substitutes.
    #[must_use]
    pub const fn max_substitutes(mut self, max: i32) -> Self {
        self.default_max_substitutes = Some(max);
        self
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the game ID.
    #[must_use]
    pub const fn game_id(mut self, game_id: Uuid) -> Self {
        self.game_id = Some(game_id);
        self
    }

    /// Set the league name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the league slug.
    #[must_use]
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the logo URL.
    #[must_use]
    pub fn logo_url(mut self, url: impl Into<String>) -> Self {
        self.logo_url = Some(url.into());
        self
    }

    /// Set access type to open.
    #[must_use]
    pub fn open(mut self) -> Self {
        self.access_type = "open".to_string();
        self
    }

    /// Set access type to invite only.
    #[must_use]
    pub fn invite_only(mut self) -> Self {
        self.access_type = "invite_only".to_string();
        self
    }

    /// Set access type to application.
    #[must_use]
    pub fn application(mut self) -> Self {
        self.access_type = "application".to_string();
        self
    }

    /// Set status to active.
    #[must_use]
    pub fn active(mut self) -> Self {
        self.status = "active".to_string();
        self
    }

    /// Set status to archived.
    #[must_use]
    pub fn archived(mut self) -> Self {
        self.status = "archived".to_string();
        self
    }

    /// Set status to suspended.
    #[must_use]
    pub fn suspended(mut self) -> Self {
        self.status = "suspended".to_string();
        self
    }

    /// Set custom settings.
    #[must_use]
    pub fn settings(mut self, settings: serde_json::Value) -> Self {
        self.settings = settings;
        self
    }

    /// Set the creator user ID.
    #[must_use]
    pub const fn created_by(mut self, user_id: Uuid) -> Self {
        self.created_by = Some(user_id);
        self
    }

    /// Build an in-memory league (not persisted).
    /// Requires `game_id` and `created_by` to be set.
    #[must_use]
    pub fn build(self, game_id: Uuid, created_by: Uuid) -> LeagueRow {
        let now = Utc::now();
        let name = self.name.unwrap_or_else(|| {
            let company: String = CompanyName().fake();
            format!("{company} League")
        });
        let slug = self.slug.unwrap_or_else(|| slug::slugify(&name));

        LeagueRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            game_id,
            name,
            slug,
            description: self.description,
            logo_url: self.logo_url,
            access_type: self.access_type,
            status: self.status,
            format_type: self.format_type,
            default_team_size_min: self.default_team_size_min,
            default_team_size_max: self.default_team_size_max,
            default_max_substitutes: self.default_max_substitutes,
            current_season_id: None, // Set by trigger after creation
            settings: self.settings,
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// Build and persist the league to the database.
    ///
    /// If `game_id` is not set, creates a test game automatically.
    /// If `created_by` is not set, creates a test user automatically.
    pub async fn build_persisted(self, pool: &DbPool) -> LeagueRow {
        use super::UserBuilder;

        // Get or create game
        let game_id = if let Some(id) = self.game_id {
            id
        } else {
            create_test_game(pool).await
        };

        // Get or create user
        let created_by = if let Some(id) = self.created_by {
            id
        } else {
            let user = UserBuilder::new().build_persisted(pool).await;
            user.id
        };

        let league = self.build(game_id, created_by);

        sqlx::query_as::<_, LeagueRow>(
            r"
            INSERT INTO leagues (
                id, game_id, name, slug, description, logo_url, access_type, status,
                format_type, default_team_size_min, default_team_size_max, default_max_substitutes,
                settings, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            ",
        )
        .bind(league.id)
        .bind(league.game_id)
        .bind(&league.name)
        .bind(&league.slug)
        .bind(&league.description)
        .bind(&league.logo_url)
        .bind(&league.access_type)
        .bind(&league.status)
        .bind(&league.format_type)
        .bind(league.default_team_size_min)
        .bind(league.default_team_size_max)
        .bind(league.default_max_substitutes)
        .bind(&league.settings)
        .bind(league.created_by)
        .fetch_one(pool)
        .await
        .expect("Failed to create test league")
    }
}

/// Creates a test game for league tests.
async fn create_test_game(pool: &DbPool) -> Uuid {
    use portal_db::entities::GameRow;

    // Use random UUID v4 for uniqueness in parallel tests
    let id = Uuid::new_v4();
    let slug = format!("game-{}", &id.to_string()[..12]);

    sqlx::query_as::<_, GameRow>(
        r"
        INSERT INTO games (id, slug, display_name, plugin_id, plugin_version, team_size_min, team_size_max, team_size_default, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&slug)
    .bind("Test Game")
    .bind("test-plugin")
    .bind("1.0.0")
    .bind(1)
    .bind(5)
    .bind(5)
    .bind("active")
    .fetch_one(pool)
    .await
    .expect("Failed to create test game")
    .id
}
