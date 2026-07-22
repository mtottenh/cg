//! League season participant builder for tests.

use chrono::Utc;
use portal_db::DbPool;
use portal_db::entities::LeagueSeasonParticipantRow;
use uuid::Uuid;

use super::{LeagueSeasonBuilder, PlayerBuilder};

/// Builder for creating test league season participants (individual format).
#[derive(Debug, Clone)]
pub struct LeagueSeasonParticipantBuilder {
    id: Option<Uuid>,
    season_id: Option<Uuid>,
    player_id: Option<Uuid>,
    status: String,
    seed: Option<i32>,
    rating: Option<i32>,
}

impl Default for LeagueSeasonParticipantBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LeagueSeasonParticipantBuilder {
    /// Create a new league season participant builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            season_id: None,
            player_id: None,
            status: "registered".to_string(),
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

    /// Set the season ID.
    #[must_use]
    pub const fn season_id(mut self, season_id: Uuid) -> Self {
        self.season_id = Some(season_id);
        self
    }

    /// Set the player ID.
    #[must_use]
    pub const fn player_id(mut self, player_id: Uuid) -> Self {
        self.player_id = Some(player_id);
        self
    }

    /// Set status to registered.
    #[must_use]
    pub fn registered(mut self) -> Self {
        self.status = "registered".to_string();
        self
    }

    /// Set status to active.
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

    /// Build an in-memory league season participant (not persisted).
    #[must_use]
    pub fn build(self, season_id: Uuid, player_id: Uuid) -> LeagueSeasonParticipantRow {
        let now = Utc::now();

        LeagueSeasonParticipantRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            season_id,
            player_id,
            status: self.status,
            seed: self.seed,
            rating: self.rating,
            matches_played: 0,
            matches_won: 0,
            matches_lost: 0,
            matches_drawn: 0,
            registered_at: now,
            withdrawn_at: None,
        }
    }

    /// Build and persist the league season participant to the database.
    ///
    /// If `season_id` is not set, creates a test season automatically.
    /// If `player_id` is not set, creates a test player automatically.
    pub async fn build_persisted(self, pool: &DbPool) -> LeagueSeasonParticipantRow {
        // Get or create season and player
        let season_id = if let Some(s) = self.season_id {
            s
        } else {
            let season = LeagueSeasonBuilder::new().build_persisted(pool).await;
            season.id
        };

        let player_id = if let Some(p) = self.player_id {
            p
        } else {
            let player = PlayerBuilder::new().build_persisted(pool).await;
            player.id
        };

        let participant = self.build(season_id, player_id);

        sqlx::query_as::<_, LeagueSeasonParticipantRow>(
            r"
            INSERT INTO league_season_participants (
                id, season_id, player_id, status, seed, rating
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(participant.id)
        .bind(participant.season_id)
        .bind(participant.player_id)
        .bind(&participant.status)
        .bind(participant.seed)
        .bind(participant.rating)
        .fetch_one(pool)
        .await
        .expect("Failed to create test league season participant")
    }
}
