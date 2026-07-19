//! League team invitation builder for tests.

use chrono::{DateTime, Duration, Utc};
use portal_db::DbPool;
use portal_db::entities::LeagueTeamInvitationRow;
use uuid::Uuid;

use super::{LeagueTeamSeasonBuilder, PlayerBuilder};

/// Builder for creating test league team invitations.
#[derive(Debug, Clone)]
pub struct LeagueTeamInvitationBuilder {
    id: Option<Uuid>,
    team_season_id: Option<Uuid>,
    player_id: Option<Uuid>,
    invitation_type: String,
    role: String,
    message: Option<String>,
    invited_by: Option<Uuid>,
    status: String,
    expires_at: Option<DateTime<Utc>>,
}

impl Default for LeagueTeamInvitationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LeagueTeamInvitationBuilder {
    /// Create a new league team invitation builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            team_season_id: None,
            player_id: None,
            invitation_type: "invite".to_string(),
            role: "player".to_string(),
            message: None,
            invited_by: None,
            status: "pending".to_string(),
            expires_at: None,
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

    /// Set the player ID (who is being invited/requesting).
    #[must_use]
    pub const fn player_id(mut self, player_id: Uuid) -> Self {
        self.player_id = Some(player_id);
        self
    }

    /// Set type to invite (captain invites player).
    #[must_use]
    pub fn invite(mut self) -> Self {
        self.invitation_type = "invite".to_string();
        self
    }

    /// Set type to request (player requests to join).
    #[must_use]
    pub fn request(mut self) -> Self {
        self.invitation_type = "request".to_string();
        self
    }

    /// Set the proposed role to captain.
    #[must_use]
    pub fn for_captain(mut self) -> Self {
        self.role = "captain".to_string();
        self
    }

    /// Set the proposed role to player.
    #[must_use]
    pub fn for_player(mut self) -> Self {
        self.role = "player".to_string();
        self
    }

    /// Set the proposed role to substitute.
    #[must_use]
    pub fn for_substitute(mut self) -> Self {
        self.role = "substitute".to_string();
        self
    }

    /// Set a custom role string.
    #[must_use]
    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.role = role.into();
        self
    }

    /// Set the invitation message.
    #[must_use]
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set who sent this invitation.
    #[must_use]
    pub const fn invited_by(mut self, user_id: Uuid) -> Self {
        self.invited_by = Some(user_id);
        self
    }

    /// Set status to pending.
    #[must_use]
    pub fn pending(mut self) -> Self {
        self.status = "pending".to_string();
        self
    }

    /// Set status to accepted.
    #[must_use]
    pub fn accepted(mut self) -> Self {
        self.status = "accepted".to_string();
        self
    }

    /// Set status to declined.
    #[must_use]
    pub fn declined(mut self) -> Self {
        self.status = "declined".to_string();
        self
    }

    /// Set status to expired.
    #[must_use]
    pub fn expired_status(mut self) -> Self {
        self.status = "expired".to_string();
        self
    }

    /// Set status to cancelled.
    #[must_use]
    pub fn cancelled(mut self) -> Self {
        self.status = "cancelled".to_string();
        self
    }

    /// Set a specific expiration time.
    #[must_use]
    pub const fn expires_at(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set expiration to N days from now.
    #[must_use]
    pub fn expires_in_days(mut self, days: i64) -> Self {
        self.expires_at = Some(Utc::now() + Duration::days(days));
        self
    }

    /// Set expiration to already expired (1 day ago).
    #[must_use]
    pub fn already_expired(mut self) -> Self {
        self.expires_at = Some(Utc::now() - Duration::days(1));
        self
    }

    /// Build an in-memory league team invitation (not persisted).
    #[must_use]
    pub fn build(self, team_season_id: Uuid, player_id: Uuid) -> LeagueTeamInvitationRow {
        let now = Utc::now();

        LeagueTeamInvitationRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            team_season_id,
            player_id,
            invitation_type: self.invitation_type,
            role: self.role,
            message: self.message,
            invited_by: self.invited_by,
            status: self.status,
            responded_at: None,
            response_message: None,
            expires_at: self.expires_at.unwrap_or_else(|| now + Duration::days(7)),
            created_at: now,
        }
    }

    /// Build and persist the league team invitation to the database.
    ///
    /// If `team_season_id` is not set, creates a test team season automatically.
    /// If `player_id` is not set, creates a test player automatically.
    pub async fn build_persisted(self, pool: &DbPool) -> LeagueTeamInvitationRow {
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

        let invitation = self.build(team_season_id, player_id);

        sqlx::query_as::<_, LeagueTeamInvitationRow>(
            r"
            INSERT INTO league_team_invitations (
                id, team_season_id, player_id, type, role, message,
                invited_by, status, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            ",
        )
        .bind(invitation.id)
        .bind(invitation.team_season_id)
        .bind(invitation.player_id)
        .bind(&invitation.invitation_type)
        .bind(&invitation.role)
        .bind(&invitation.message)
        .bind(invitation.invited_by)
        .bind(&invitation.status)
        .bind(invitation.expires_at)
        .fetch_one(pool)
        .await
        .expect("Failed to create test league team invitation")
    }
}
