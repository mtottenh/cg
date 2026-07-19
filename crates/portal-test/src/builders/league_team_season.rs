//! League team season builder for tests.

use chrono::Utc;
use portal_db::DbPool;
use portal_db::entities::LeagueTeamSeasonRow;
use uuid::Uuid;

use super::{LeagueSeasonBuilder, LeagueTeamBuilder};

/// Builder for creating test league team season registrations.
#[derive(Debug, Clone)]
pub struct LeagueTeamSeasonBuilder {
    id: Option<Uuid>,
    team_id: Option<Uuid>,
    season_id: Option<Uuid>,
    status: String,
    registration_notes: Option<String>,
    seed: Option<i32>,
    rating: Option<i32>,
}

impl Default for LeagueTeamSeasonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LeagueTeamSeasonBuilder {
    /// Create a new league team season builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            team_id: None,
            season_id: None,
            status: "registered".to_string(),
            registration_notes: None,
            seed: None,
            rating: None,
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the team ID.
    #[must_use]
    pub const fn team_id(mut self, team_id: Uuid) -> Self {
        self.team_id = Some(team_id);
        self
    }

    /// Set the season ID.
    #[must_use]
    pub const fn season_id(mut self, season_id: Uuid) -> Self {
        self.season_id = Some(season_id);
        self
    }

    /// Set status to registered (pending approval or just signed up).
    #[must_use]
    pub fn registered(mut self) -> Self {
        self.status = "registered".to_string();
        self
    }

    /// Set status to confirmed (approved for participation).
    #[must_use]
    pub fn confirmed(mut self) -> Self {
        self.status = "confirmed".to_string();
        self
    }

    /// Set status to active (participating in matches).
    #[must_use]
    pub fn active(mut self) -> Self {
        self.status = "active".to_string();
        self
    }

    /// Set status to eliminated.
    #[must_use]
    pub fn eliminated(mut self) -> Self {
        self.status = "eliminated".to_string();
        self
    }

    /// Set status to disqualified.
    #[must_use]
    pub fn disqualified(mut self) -> Self {
        self.status = "disqualified".to_string();
        self
    }

    /// Set status to withdrawn.
    #[must_use]
    pub fn withdrawn(mut self) -> Self {
        self.status = "withdrawn".to_string();
        self
    }

    /// Set registration notes.
    #[must_use]
    pub fn notes(mut self, notes: impl Into<String>) -> Self {
        self.registration_notes = Some(notes.into());
        self
    }

    /// Set seed.
    #[must_use]
    pub const fn seed(mut self, seed: i32) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Set rating.
    #[must_use]
    pub const fn rating(mut self, rating: i32) -> Self {
        self.rating = Some(rating);
        self
    }

    /// Build an in-memory league team season (not persisted).
    /// Requires `team_id` and `season_id` to be set.
    #[must_use]
    pub fn build(self, team_id: Uuid, season_id: Uuid) -> LeagueTeamSeasonRow {
        let now = Utc::now();

        LeagueTeamSeasonRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            team_id,
            season_id,
            status: self.status,
            registered_at: Some(now),
            registration_notes: self.registration_notes,
            matches_played: 0,
            matches_won: 0,
            matches_lost: 0,
            matches_drawn: 0,
            seed: self.seed,
            rating: self.rating,
            created_at: now,
            updated_at: now,
        }
    }

    /// Build and persist the league team season to the database.
    ///
    /// If `team_id` is not set, creates a test team automatically.
    /// If `season_id` is not set, creates a test season automatically.
    ///
    /// Note: The team and season should belong to the same league.
    /// If creating both automatically, this will create them in the same league.
    pub async fn build_persisted(self, pool: &DbPool) -> LeagueTeamSeasonRow {
        // Get or create team and season
        let (team_id, season_id) = match (self.team_id, self.season_id) {
            (Some(t), Some(s)) => (t, s),
            (Some(t), None) => {
                // Team exists, get its league and create season in same league
                let league_id = sqlx::query_scalar::<_, Uuid>(
                    "SELECT league_id FROM league_teams WHERE id = $1",
                )
                .bind(t)
                .fetch_one(pool)
                .await
                .expect("Team not found");

                let season = LeagueSeasonBuilder::new()
                    .league_id(league_id)
                    .build_persisted(pool)
                    .await;
                (t, season.id)
            }
            (None, Some(s)) => {
                // Season exists, get its league and create team in same league
                let league_id = sqlx::query_scalar::<_, Uuid>(
                    "SELECT league_id FROM league_seasons WHERE id = $1",
                )
                .bind(s)
                .fetch_one(pool)
                .await
                .expect("Season not found");

                let team = LeagueTeamBuilder::new()
                    .league_id(league_id)
                    .build_persisted(pool)
                    .await;
                (team.id, s)
            }
            (None, None) => {
                // Create both in the same league
                let season = LeagueSeasonBuilder::new().build_persisted(pool).await;
                let team = LeagueTeamBuilder::new()
                    .league_id(season.league_id)
                    .build_persisted(pool)
                    .await;
                (team.id, season.id)
            }
        };

        let team_season = self.build(team_id, season_id);

        sqlx::query_as::<_, LeagueTeamSeasonRow>(
            r"
            INSERT INTO league_team_seasons (
                id, team_id, season_id, status, registered_at, registration_notes,
                seed, rating
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
        )
        .bind(team_season.id)
        .bind(team_season.team_id)
        .bind(team_season.season_id)
        .bind(&team_season.status)
        .bind(team_season.registered_at)
        .bind(&team_season.registration_notes)
        .bind(team_season.seed)
        .bind(team_season.rating)
        .fetch_one(pool)
        .await
        .expect("Failed to create test league team season")
    }
}
