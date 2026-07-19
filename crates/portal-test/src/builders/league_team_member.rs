//! League team member builder for tests.

use chrono::Utc;
use portal_db::DbPool;
use portal_db::entities::LeagueTeamMemberRow;
use uuid::Uuid;

use super::{LeagueTeamSeasonBuilder, PlayerBuilder};

/// Builder for creating test league team members.
#[derive(Debug, Clone)]
pub struct LeagueTeamMemberBuilder {
    id: Option<Uuid>,
    team_season_id: Option<Uuid>,
    player_id: Option<Uuid>,
    role: String,
    position: Option<String>,
    jersey_number: Option<i32>,
    status: String,
    added_by: Option<Uuid>,
}

impl Default for LeagueTeamMemberBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LeagueTeamMemberBuilder {
    /// Create a new league team member builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            team_season_id: None,
            player_id: None,
            role: "player".to_string(),
            position: None,
            jersey_number: None,
            status: "active".to_string(),
            added_by: None,
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the team season ID.
    #[must_use]
    pub const fn team_season_id(mut self, team_season_id: Uuid) -> Self {
        self.team_season_id = Some(team_season_id);
        self
    }

    /// Set the player ID.
    #[must_use]
    pub const fn player_id(mut self, player_id: Uuid) -> Self {
        self.player_id = Some(player_id);
        self
    }

    /// Set role to captain (can manage roster).
    #[must_use]
    pub fn captain(mut self) -> Self {
        self.role = "captain".to_string();
        self
    }

    /// Set role to player (primary roster member).
    #[must_use]
    pub fn player(mut self) -> Self {
        self.role = "player".to_string();
        self
    }

    /// Set role to substitute (backup player).
    #[must_use]
    pub fn substitute(mut self) -> Self {
        self.role = "substitute".to_string();
        self
    }

    /// Set a custom role string.
    #[must_use]
    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.role = role.into();
        self
    }

    /// Set the position (e.g., "mid", "support").
    #[must_use]
    pub fn position(mut self, position: impl Into<String>) -> Self {
        self.position = Some(position.into());
        self
    }

    /// Set the jersey number.
    #[must_use]
    pub const fn jersey_number(mut self, number: i32) -> Self {
        self.jersey_number = Some(number);
        self
    }

    /// Set status to active.
    #[must_use]
    pub fn active(mut self) -> Self {
        self.status = "active".to_string();
        self
    }

    /// Set status to inactive.
    #[must_use]
    pub fn inactive(mut self) -> Self {
        self.status = "inactive".to_string();
        self
    }

    /// Set status to left.
    #[must_use]
    pub fn left(mut self) -> Self {
        self.status = "left".to_string();
        self
    }

    /// Set status to removed.
    #[must_use]
    pub fn removed(mut self) -> Self {
        self.status = "removed".to_string();
        self
    }

    /// Set who added this member.
    #[must_use]
    pub const fn added_by(mut self, user_id: Uuid) -> Self {
        self.added_by = Some(user_id);
        self
    }

    /// Build an in-memory league team member (not persisted).
    /// Requires `team_season_id`, `player_id`, and `season_id` to be known.
    #[must_use]
    pub fn build(
        self,
        team_season_id: Uuid,
        player_id: Uuid,
        season_id: Uuid,
    ) -> LeagueTeamMemberRow {
        let now = Utc::now();

        LeagueTeamMemberRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            team_season_id,
            player_id,
            season_id,
            role: self.role,
            position: self.position,
            jersey_number: self.jersey_number,
            status: self.status,
            joined_at: now,
            left_at: None,
            added_by: self.added_by,
        }
    }

    /// Build and persist the league team member to the database.
    ///
    /// If `team_season_id` is not set, creates a test team season automatically.
    /// If `player_id` is not set, creates a test player automatically.
    pub async fn build_persisted(self, pool: &DbPool) -> LeagueTeamMemberRow {
        // Get or create team_season and player
        let team_season_id = if let Some(ts) = self.team_season_id {
            ts
        } else {
            let team_season = LeagueTeamSeasonBuilder::new().build_persisted(pool).await;
            team_season.id
        };

        let player_id = if let Some(p) = self.player_id {
            p
        } else {
            let player = PlayerBuilder::new().build_persisted(pool).await;
            player.id
        };

        // Get season_id from team_season (trigger will set it, but we need it for return)
        let season_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT season_id FROM league_team_seasons WHERE id = $1",
        )
        .bind(team_season_id)
        .fetch_one(pool)
        .await
        .expect("Team season not found");

        let member = self.build(team_season_id, player_id, season_id);

        sqlx::query_as::<_, LeagueTeamMemberRow>(
            r"
            INSERT INTO league_team_members (
                id, team_season_id, player_id, role, position, jersey_number,
                status, added_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
        )
        .bind(member.id)
        .bind(member.team_season_id)
        .bind(member.player_id)
        .bind(&member.role)
        .bind(&member.position)
        .bind(member.jersey_number)
        .bind(&member.status)
        .bind(member.added_by)
        .fetch_one(pool)
        .await
        .expect("Failed to create test league team member")
    }
}
