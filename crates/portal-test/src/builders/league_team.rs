//! League team builder for tests.

use chrono::Utc;
use fake::faker::company::en::CompanyName;
use fake::Fake;
use portal_db::entities::LeagueTeamRow;
use portal_db::DbPool;
use uuid::Uuid;

use super::{LeagueBuilder, PlayerBuilder};

/// Builder for creating test league teams.
#[derive(Debug, Clone)]
pub struct LeagueTeamBuilder {
    id: Option<Uuid>,
    league_id: Option<Uuid>,
    name: Option<String>,
    tag: Option<String>,
    description: Option<String>,
    logo_url: Option<String>,
    banner_url: Option<String>,
    primary_color: Option<String>,
    secondary_color: Option<String>,
    owner_player_id: Option<Uuid>,
    status: String,
}

impl Default for LeagueTeamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LeagueTeamBuilder {
    /// Create a new league team builder with random defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            league_id: None,
            name: None,
            tag: None,
            description: None,
            logo_url: None,
            banner_url: None,
            primary_color: None,
            secondary_color: None,
            owner_player_id: None,
            status: "active".to_string(),
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the league ID.
    #[must_use]
    pub const fn league_id(mut self, league_id: Uuid) -> Self {
        self.league_id = Some(league_id);
        self
    }

    /// Set the team name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the team tag.
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
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

    /// Set the banner URL.
    #[must_use]
    pub fn banner_url(mut self, url: impl Into<String>) -> Self {
        self.banner_url = Some(url.into());
        self
    }

    /// Set the primary color.
    #[must_use]
    pub fn primary_color(mut self, color: impl Into<String>) -> Self {
        self.primary_color = Some(color.into());
        self
    }

    /// Set the secondary color.
    #[must_use]
    pub fn secondary_color(mut self, color: impl Into<String>) -> Self {
        self.secondary_color = Some(color.into());
        self
    }

    /// Set the owner player ID.
    #[must_use]
    pub const fn owner(mut self, player_id: Uuid) -> Self {
        self.owner_player_id = Some(player_id);
        self
    }

    /// Set status to active.
    #[must_use]
    pub fn active(mut self) -> Self {
        self.status = "active".to_string();
        self
    }

    /// Set status to disbanded.
    #[must_use]
    pub fn disbanded(mut self) -> Self {
        self.status = "disbanded".to_string();
        self
    }

    /// Set status to suspended.
    #[must_use]
    pub fn suspended(mut self) -> Self {
        self.status = "suspended".to_string();
        self
    }

    /// Build an in-memory league team (not persisted).
    /// Requires `league_id` and `owner_player_id` to be set.
    #[must_use]
    pub fn build(self, league_id: Uuid, owner_player_id: Uuid) -> LeagueTeamRow {
        let now = Utc::now();
        let name = self.name.unwrap_or_else(|| {
            let company: String = CompanyName().fake();
            format!("{company} Gaming")
        });
        let tag = self.tag.unwrap_or_else(|| {
            // Generate a 3-4 letter tag from the name
            name.chars()
                .filter(|c| c.is_alphabetic())
                .take(4)
                .collect::<String>()
                .to_uppercase()
        });

        LeagueTeamRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            league_id,
            name: name.clone(),
            name_normalized: name.to_lowercase(),
            tag: tag.clone(),
            tag_normalized: tag.to_lowercase(),
            description: self.description,
            logo_url: self.logo_url,
            banner_url: self.banner_url,
            primary_color: self.primary_color,
            secondary_color: self.secondary_color,
            owner_player_id,
            status: self.status,
            created_at: now,
            updated_at: now,
            disbanded_at: None,
        }
    }

    /// Build and persist the league team to the database.
    ///
    /// If `league_id` is not set, creates a test league automatically.
    /// If `owner_player_id` is not set, creates a test player automatically.
    pub async fn build_persisted(self, pool: &DbPool) -> LeagueTeamRow {
        // Get or create league
        let league_id = if let Some(id) = self.league_id {
            id
        } else {
            let league = LeagueBuilder::new().build_persisted(pool).await;
            league.id
        };

        // Get or create owner
        let owner_player_id = if let Some(id) = self.owner_player_id {
            id
        } else {
            let player = PlayerBuilder::new().build_persisted(pool).await;
            player.id
        };

        let team = self.build(league_id, owner_player_id);

        // Note: name_normalized and tag_normalized are generated columns
        sqlx::query_as::<_, LeagueTeamRow>(
            r"
            INSERT INTO league_teams (
                id, league_id, name, tag,
                description, logo_url, banner_url, primary_color, secondary_color,
                owner_player_id, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(team.id)
        .bind(team.league_id)
        .bind(&team.name)
        .bind(&team.tag)
        .bind(&team.description)
        .bind(&team.logo_url)
        .bind(&team.banner_url)
        .bind(&team.primary_color)
        .bind(&team.secondary_color)
        .bind(team.owner_player_id)
        .bind(&team.status)
        .fetch_one(pool)
        .await
        .expect("Failed to create test league team")
    }
}
