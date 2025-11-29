//! Team builder for tests.

use chrono::Utc;
use fake::faker::company::en::CompanyName;
use fake::Fake;
use portal_core::types::TeamRole;
use portal_core::{PlayerId, TeamId};
use portal_db::entities::{TeamMemberRow, TeamRow};
use portal_db::DbPool;
use uuid::Uuid;

use super::PlayerBuilder;

/// Builder for creating test teams.
#[derive(Debug, Clone)]
pub struct TeamBuilder {
    id: Option<Uuid>,
    name: Option<String>,
    tag: Option<String>,
    description: Option<String>,
    logo_url: Option<String>,
    game_id: Option<String>,
    founder_id: Option<Uuid>,
    status: String,
    /// Additional members to add (player_id, role)
    members: Vec<(Option<Uuid>, TeamRole)>,
}

impl Default for TeamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TeamBuilder {
    /// Create a new team builder with random defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            name: None,
            tag: None,
            description: None,
            logo_url: None,
            game_id: None,
            founder_id: None,
            status: "active".to_string(),
            members: Vec::new(),
        }
    }

    /// Create a team builder with a specific name.
    #[must_use]
    pub fn with_name(name: impl Into<String>) -> Self {
        Self::new().name(name)
    }

    /// Set a specific ID.
    #[must_use]
    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
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

    /// Set the game ID.
    #[must_use]
    pub fn for_game(mut self, game_id: impl Into<String>) -> Self {
        self.game_id = Some(game_id.into());
        self
    }

    /// Set the founder (player who creates the team).
    #[must_use]
    pub fn with_founder(mut self, player_id: Uuid) -> Self {
        self.founder_id = Some(player_id);
        self
    }

    /// Add a captain to the team.
    #[must_use]
    pub fn with_captain(mut self, player_id: Option<Uuid>) -> Self {
        self.members.push((player_id, TeamRole::Captain));
        self
    }

    /// Add an officer to the team.
    #[must_use]
    pub fn with_officer(mut self, player_id: Option<Uuid>) -> Self {
        self.members.push((player_id, TeamRole::Officer));
        self
    }

    /// Add a player to the team.
    #[must_use]
    pub fn with_player(mut self, player_id: Option<Uuid>) -> Self {
        self.members.push((player_id, TeamRole::Player));
        self
    }

    /// Add a substitute to the team.
    #[must_use]
    pub fn with_substitute(mut self, player_id: Option<Uuid>) -> Self {
        self.members.push((player_id, TeamRole::Substitute));
        self
    }

    /// Add N random players to the team.
    #[must_use]
    pub fn with_n_players(mut self, n: usize) -> Self {
        for _ in 0..n {
            self.members.push((None, TeamRole::Player));
        }
        self
    }

    /// Set the team as disbanded.
    #[must_use]
    pub fn disbanded(mut self) -> Self {
        self.status = "disbanded".to_string();
        self
    }

    /// Set the team as suspended.
    #[must_use]
    pub fn suspended(mut self) -> Self {
        self.status = "suspended".to_string();
        self
    }

    /// Generate a random tag from the team name.
    fn generate_tag(name: &str) -> String {
        // Take first letter of each word, up to 5 chars
        name.split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(5)
            .collect::<String>()
            .to_uppercase()
    }

    /// Build and persist the team to the database.
    ///
    /// If no founder is set, creates a new player automatically.
    /// Also creates any additional members specified.
    pub async fn build_persisted(self, pool: &DbPool) -> TeamWithMembers {
        let now = Utc::now();

        // Create founder if needed
        let founder_id = if let Some(id) = self.founder_id {
            id
        } else {
            let player = PlayerBuilder::new().build_persisted(pool).await;
            player.id
        };

        let name = self.name.unwrap_or_else(|| CompanyName().fake());
        let tag = self.tag.unwrap_or_else(|| Self::generate_tag(&name));

        // Create the team
        let team = sqlx::query_as::<_, TeamRow>(
            r#"
            INSERT INTO teams (id, name, tag, description, logo_url, game_id, created_by, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(self.id.unwrap_or_else(Uuid::now_v7))
        .bind(&name)
        .bind(&tag)
        .bind(&self.description)
        .bind(&self.logo_url)
        .bind(&self.game_id)
        .bind(founder_id)
        .bind(&self.status)
        .fetch_one(pool)
        .await
        .expect("Failed to create test team");

        // Add founder as captain
        let founder_member = sqlx::query_as::<_, TeamMemberRow>(
            r#"
            WITH inserted AS (
                INSERT INTO team_members (team_id, player_id, role, is_founder)
                VALUES ($1, $2, 'captain', true)
                RETURNING *
            )
            SELECT
                i.id, i.team_id, i.player_id,
                p.display_name, p.avatar_url,
                i.role, i.role_title, i.is_founder,
                i.primary_position, i.secondary_position,
                i.status, i.jersey_number, i.invited_by,
                i.joined_at, i.left_at
            FROM inserted i
            JOIN players p ON p.id = i.player_id
            "#,
        )
        .bind(team.id)
        .bind(founder_id)
        .fetch_one(pool)
        .await
        .expect("Failed to add founder to team");

        let mut members = vec![founder_member];

        // Add additional members
        for (player_id, role) in self.members {
            // Create player if needed
            let member_id = if let Some(id) = player_id {
                id
            } else {
                let player = PlayerBuilder::new().build_persisted(pool).await;
                player.id
            };

            let member = sqlx::query_as::<_, TeamMemberRow>(
                r#"
                WITH inserted AS (
                    INSERT INTO team_members (team_id, player_id, role, is_founder, invited_by)
                    VALUES ($1, $2, $3, false, $4)
                    RETURNING *
                )
                SELECT
                    i.id, i.team_id, i.player_id,
                    p.display_name, p.avatar_url,
                    i.role, i.role_title, i.is_founder,
                    i.primary_position, i.secondary_position,
                    i.status, i.jersey_number, i.invited_by,
                    i.joined_at, i.left_at
                FROM inserted i
                JOIN players p ON p.id = i.player_id
                "#,
            )
            .bind(team.id)
            .bind(member_id)
            .bind(role.to_string())
            .bind(founder_id)
            .fetch_one(pool)
            .await
            .expect("Failed to add member to team");

            members.push(member);
        }

        TeamWithMembers { team, members }
    }
}

/// A team with its members, returned by the builder.
#[derive(Debug)]
pub struct TeamWithMembers {
    pub team: TeamRow,
    pub members: Vec<TeamMemberRow>,
}

impl TeamWithMembers {
    /// Get the team.
    pub fn team(&self) -> &TeamRow {
        &self.team
    }

    /// Get all members.
    pub fn members(&self) -> &[TeamMemberRow] {
        &self.members
    }

    /// Get the founder.
    pub fn founder(&self) -> &TeamMemberRow {
        self.members
            .iter()
            .find(|m| m.is_founder)
            .expect("Team must have a founder")
    }

    /// Get captains.
    pub fn captains(&self) -> Vec<&TeamMemberRow> {
        self.members.iter().filter(|m| m.role == "captain").collect()
    }

    /// Get non-captain members.
    pub fn non_captains(&self) -> Vec<&TeamMemberRow> {
        self.members.iter().filter(|m| m.role != "captain").collect()
    }
}
