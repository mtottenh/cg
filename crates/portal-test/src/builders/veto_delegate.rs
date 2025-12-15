//! Veto delegate builder for tests.

use chrono::Utc;
use portal_db::entities::VetoDelegateRow;
use portal_db::DbPool;
use uuid::Uuid;

use super::{LeagueTeamSeasonBuilder, PlayerBuilder, UserBuilder};

/// Builder for creating test veto delegates.
#[derive(Debug, Clone)]
pub struct VetoDelegateBuilder {
    id: Option<Uuid>,
    team_season_id: Option<Uuid>,
    player_id: Option<Uuid>,
    delegated_by_user_id: Option<Uuid>,
    delegated_by_role: String,
    tournament_id: Option<Uuid>,
}

impl Default for VetoDelegateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl VetoDelegateBuilder {
    /// Create a new veto delegate builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            team_season_id: None,
            player_id: None,
            delegated_by_user_id: None,
            delegated_by_role: "captain".to_string(),
            tournament_id: None,
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

    /// Set the player ID (the delegate).
    #[must_use]
    pub const fn player_id(mut self, player_id: Uuid) -> Self {
        self.player_id = Some(player_id);
        self
    }

    /// Set the user ID who delegated.
    #[must_use]
    pub const fn delegated_by_user_id(mut self, user_id: Uuid) -> Self {
        self.delegated_by_user_id = Some(user_id);
        self
    }

    /// Set delegation by captain.
    #[must_use]
    pub fn by_captain(mut self) -> Self {
        self.delegated_by_role = "captain".to_string();
        self
    }

    /// Set delegation by owner.
    #[must_use]
    pub fn by_owner(mut self) -> Self {
        self.delegated_by_role = "owner".to_string();
        self
    }

    /// Set delegation by tournament admin.
    #[must_use]
    pub fn by_tournament_admin(mut self) -> Self {
        self.delegated_by_role = "tournament_admin".to_string();
        self
    }

    /// Set the delegated_by_role explicitly.
    #[must_use]
    pub fn delegated_by_role(mut self, role: impl Into<String>) -> Self {
        self.delegated_by_role = role.into();
        self
    }

    /// Scope the delegation to a specific tournament.
    #[must_use]
    pub const fn for_tournament(mut self, tournament_id: Uuid) -> Self {
        self.tournament_id = Some(tournament_id);
        self
    }

    /// Build an in-memory veto delegate (not persisted).
    #[must_use]
    pub fn build(
        self,
        team_season_id: Uuid,
        player_id: Uuid,
        delegated_by_user_id: Uuid,
    ) -> VetoDelegateRow {
        let now = Utc::now();

        VetoDelegateRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            team_season_id,
            player_id,
            delegated_by_user_id,
            delegated_by_role: self.delegated_by_role,
            tournament_id: self.tournament_id,
            revoked_at: None,
            revoked_by_user_id: None,
            created_at: now,
        }
    }

    /// Build and persist the veto delegate to the database.
    ///
    /// If `team_season_id` is not set, creates a test team season automatically.
    /// If `player_id` is not set, creates a test player automatically.
    /// If `delegated_by_user_id` is not set, creates a test user automatically.
    pub async fn build_persisted(self, pool: &DbPool) -> VetoDelegateRow {
        // Get or create team_season
        let team_season_id = if let Some(ts) = self.team_season_id {
            ts
        } else {
            let team_season = LeagueTeamSeasonBuilder::new().build_persisted(pool).await;
            team_season.id
        };

        // Get or create player (the delegate)
        let player_id = if let Some(p) = self.player_id {
            p
        } else {
            let player = PlayerBuilder::new().build_persisted(pool).await;
            player.id
        };

        // Get or create the user who delegated
        let delegated_by_user_id = if let Some(u) = self.delegated_by_user_id {
            u
        } else {
            let user = UserBuilder::new().build_persisted(pool).await;
            user.id
        };

        let delegate = self.build(team_season_id, player_id, delegated_by_user_id);

        sqlx::query_as::<_, VetoDelegateRow>(
            r"
            INSERT INTO veto_delegates (
                id, team_season_id, player_id, delegated_by_user_id,
                delegated_by_role, tournament_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(delegate.id)
        .bind(delegate.team_season_id)
        .bind(delegate.player_id)
        .bind(delegate.delegated_by_user_id)
        .bind(&delegate.delegated_by_role)
        .bind(delegate.tournament_id)
        .fetch_one(pool)
        .await
        .expect("Failed to create test veto delegate")
    }
}
