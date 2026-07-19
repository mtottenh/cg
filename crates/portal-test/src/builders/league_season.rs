//! League season builder for tests.

use chrono::{DateTime, Duration, Utc};
use portal_db::DbPool;
use portal_db::entities::LeagueSeasonRow;
use uuid::Uuid;

use super::LeagueBuilder;

/// Builder for creating test league seasons.
#[derive(Debug, Clone)]
pub struct LeagueSeasonBuilder {
    id: Option<Uuid>,
    league_id: Option<Uuid>,
    name: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    registration_start: Option<DateTime<Utc>>,
    registration_end: Option<DateTime<Utc>>,
    season_start: Option<DateTime<Utc>>,
    season_end: Option<DateTime<Utc>>,
    team_size_min: Option<i32>,
    team_size_max: Option<i32>,
    max_substitutes: Option<i32>,
    max_teams: Option<i32>,
    roster_lock_status: String,
    status: String,
    settings: serde_json::Value,
    created_by: Option<Uuid>,
}

impl Default for LeagueSeasonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LeagueSeasonBuilder {
    /// Create a new league season builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            league_id: None,
            name: None,
            slug: None,
            description: None,
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: Some(3),
            team_size_max: Some(5),
            max_substitutes: Some(2),
            max_teams: None,
            roster_lock_status: "open".to_string(),
            status: "registration".to_string(),
            settings: serde_json::json!({}),
            created_by: None,
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

    /// Set the season name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the season slug.
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

    /// Set registration start time.
    #[must_use]
    pub const fn registration_start(mut self, time: DateTime<Utc>) -> Self {
        self.registration_start = Some(time);
        self
    }

    /// Set registration end time.
    #[must_use]
    pub const fn registration_end(mut self, time: DateTime<Utc>) -> Self {
        self.registration_end = Some(time);
        self
    }

    /// Set season start time.
    #[must_use]
    pub const fn season_start(mut self, time: DateTime<Utc>) -> Self {
        self.season_start = Some(time);
        self
    }

    /// Set season end time.
    #[must_use]
    pub const fn season_end(mut self, time: DateTime<Utc>) -> Self {
        self.season_end = Some(time);
        self
    }

    /// Set team size constraints.
    #[must_use]
    pub const fn team_size(mut self, min: i32, max: i32) -> Self {
        self.team_size_min = Some(min);
        self.team_size_max = Some(max);
        self
    }

    /// Set maximum substitutes.
    #[must_use]
    pub const fn max_substitutes(mut self, max: i32) -> Self {
        self.max_substitutes = Some(max);
        self
    }

    /// Set maximum teams.
    #[must_use]
    pub const fn max_teams(mut self, max: i32) -> Self {
        self.max_teams = Some(max);
        self
    }

    /// Set status to draft.
    #[must_use]
    pub fn draft(mut self) -> Self {
        self.status = "draft".to_string();
        self
    }

    /// Set status to registration (open for signups).
    #[must_use]
    pub fn registration(mut self) -> Self {
        self.status = "registration".to_string();
        self
    }

    /// Set status to active (competition ongoing).
    #[must_use]
    pub fn active(mut self) -> Self {
        self.status = "active".to_string();
        self
    }

    /// Set status to playoffs.
    #[must_use]
    pub fn playoffs(mut self) -> Self {
        self.status = "playoffs".to_string();
        self
    }

    /// Set status to completed.
    #[must_use]
    pub fn completed(mut self) -> Self {
        self.status = "completed".to_string();
        self
    }

    /// Set status to cancelled.
    #[must_use]
    pub fn cancelled(mut self) -> Self {
        self.status = "cancelled".to_string();
        self
    }

    /// Set roster lock to open (all changes allowed).
    #[must_use]
    pub fn roster_open(mut self) -> Self {
        self.roster_lock_status = "open".to_string();
        self
    }

    /// Set roster lock to soft lock (only substitute changes allowed).
    #[must_use]
    pub fn roster_soft_lock(mut self) -> Self {
        self.roster_lock_status = "soft_lock".to_string();
        self
    }

    /// Set roster lock to hard lock (no changes allowed).
    #[must_use]
    pub fn roster_hard_lock(mut self) -> Self {
        self.roster_lock_status = "hard_lock".to_string();
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

    /// Configure with realistic registration window (now to 7 days from now).
    #[must_use]
    pub fn with_registration_window(mut self) -> Self {
        let now = Utc::now();
        self.registration_start = Some(now);
        self.registration_end = Some(now + Duration::days(7));
        self
    }

    /// Build an in-memory league season (not persisted).
    /// Requires `league_id` and `created_by` to be set.
    #[must_use]
    pub fn build(self, league_id: Uuid, created_by: Uuid) -> LeagueSeasonRow {
        let now = Utc::now();
        let name = self.name.unwrap_or_else(|| "Season 1".to_string());
        let slug = self.slug.unwrap_or_else(|| slug::slugify(&name));

        LeagueSeasonRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            league_id,
            name,
            slug,
            description: self.description,
            registration_start: self.registration_start,
            registration_end: self.registration_end,
            season_start: self.season_start,
            season_end: self.season_end,
            team_size_min: self.team_size_min,
            team_size_max: self.team_size_max,
            max_substitutes: self.max_substitutes,
            max_teams: self.max_teams,
            roster_lock_status: self.roster_lock_status,
            roster_locked_at: None,
            roster_locked_by: None,
            status: self.status,
            settings: self.settings,
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// Build and persist the league season to the database.
    ///
    /// If `league_id` is not set, creates a test league automatically.
    /// If `created_by` is not set, uses the league's creator.
    pub async fn build_persisted(self, pool: &DbPool) -> LeagueSeasonRow {
        // Get or create league
        let (league_id, league_creator) = if let Some(id) = self.league_id {
            // Fetch the league's created_by
            let creator =
                sqlx::query_scalar::<_, Uuid>("SELECT created_by FROM leagues WHERE id = $1")
                    .bind(id)
                    .fetch_one(pool)
                    .await
                    .expect("League not found");
            (id, creator)
        } else {
            let league = LeagueBuilder::new().build_persisted(pool).await;
            (league.id, league.created_by)
        };

        // Use provided created_by or fall back to league creator
        let created_by = self.created_by.unwrap_or(league_creator);

        let season = self.build(league_id, created_by);

        sqlx::query_as::<_, LeagueSeasonRow>(
            r"
            INSERT INTO league_seasons (
                id, league_id, name, slug, description,
                registration_start, registration_end, season_start, season_end,
                team_size_min, team_size_max, max_substitutes, max_teams,
                roster_lock_status, status, settings, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING *
            ",
        )
        .bind(season.id)
        .bind(season.league_id)
        .bind(&season.name)
        .bind(&season.slug)
        .bind(&season.description)
        .bind(season.registration_start)
        .bind(season.registration_end)
        .bind(season.season_start)
        .bind(season.season_end)
        .bind(season.team_size_min)
        .bind(season.team_size_max)
        .bind(season.max_substitutes)
        .bind(season.max_teams)
        .bind(&season.roster_lock_status)
        .bind(&season.status)
        .bind(&season.settings)
        .bind(season.created_by)
        .fetch_one(pool)
        .await
        .expect("Failed to create test league season")
    }
}
