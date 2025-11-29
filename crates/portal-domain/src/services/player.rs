//! Player service with business logic.

use crate::entities::team::{PlayerTeamMembership, Team};
use crate::entities::Player;
use crate::repositories::team::{TeamMemberRepository, TeamRepository};
use crate::repositories::{PlayerRepository, UpdatePlayer};
use portal_core::{DomainError, PlayerId, UserId};
use std::sync::Arc;
use tracing::instrument;

/// Result of a paginated player search.
#[derive(Debug, Clone)]
pub struct PlayerSearchResult {
    /// The players matching the search.
    pub players: Vec<Player>,
    /// Total count of matching players.
    pub total: i64,
}

/// Service for player-related business logic.
pub struct PlayerService<PR, TR, TMR>
where
    PR: PlayerRepository,
    TR: TeamRepository,
    TMR: TeamMemberRepository,
{
    player_repo: Arc<PR>,
    team_repo: Arc<TR>,
    team_member_repo: Arc<TMR>,
}

impl<PR, TR, TMR> PlayerService<PR, TR, TMR>
where
    PR: PlayerRepository,
    TR: TeamRepository,
    TMR: TeamMemberRepository,
{
    /// Create a new player service.
    pub fn new(player_repo: Arc<PR>, team_repo: Arc<TR>, team_member_repo: Arc<TMR>) -> Self {
        Self {
            player_repo,
            team_repo,
            team_member_repo,
        }
    }

    /// Get a player by ID.
    #[instrument(skip(self))]
    pub async fn get_player(&self, id: PlayerId) -> Result<Player, DomainError> {
        self.player_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::PlayerNotFound(id.to_string()))
    }

    /// Get a player by user ID.
    #[instrument(skip(self))]
    pub async fn get_player_by_user_id(&self, user_id: UserId) -> Result<Player, DomainError> {
        self.player_repo
            .find_by_user_id(user_id)
            .await?
            .ok_or_else(|| DomainError::PlayerNotFound(format!("user:{}", user_id)))
    }

    /// Search players by display name.
    #[instrument(skip(self))]
    pub async fn search_players(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<PlayerSearchResult, DomainError> {
        let players = self.player_repo.search(query, limit, offset).await?;
        let total = self.player_repo.count_search(query).await?;

        Ok(PlayerSearchResult { players, total })
    }

    /// Get teams for a player.
    #[instrument(skip(self))]
    pub async fn get_player_teams(&self, player_id: PlayerId) -> Result<Vec<Team>, DomainError> {
        // Verify player exists
        let _ = self.get_player(player_id).await?;
        self.team_repo.list_by_player(player_id).await
    }

    /// Get team memberships for a player (with team details and role).
    #[instrument(skip(self))]
    pub async fn get_player_team_memberships(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerTeamMembership>, DomainError> {
        // Verify player exists
        let _ = self.get_player(player_id).await?;
        self.team_member_repo
            .list_memberships_for_player(player_id)
            .await
    }

    /// Find a player by display name.
    #[instrument(skip(self))]
    pub async fn find_by_display_name(&self, name: &str) -> Result<Option<Player>, DomainError> {
        self.player_repo.find_by_display_name(name).await
    }

    /// Update a player's profile.
    ///
    /// Only the player themselves can update their profile.
    #[instrument(skip(self))]
    pub async fn update_profile(
        &self,
        player_id: PlayerId,
        cmd: UpdatePlayer,
    ) -> Result<Player, DomainError> {
        // Verify player exists
        let _ = self.get_player(player_id).await?;

        // Validate display name uniqueness if changing
        if let Some(ref new_name) = cmd.display_name {
            if let Some(existing) = self.player_repo.find_by_display_name(new_name).await? {
                if existing.id != player_id {
                    return Err(DomainError::Conflict(format!(
                        "Display name '{}' is already taken",
                        new_name
                    )));
                }
            }
        }

        self.player_repo.update(player_id, cmd).await
    }
}

impl<PR, TR, TMR> Clone for PlayerService<PR, TR, TMR>
where
    PR: PlayerRepository,
    TR: TeamRepository,
    TMR: TeamMemberRepository,
{
    fn clone(&self) -> Self {
        Self {
            player_repo: Arc::clone(&self.player_repo),
            team_repo: Arc::clone(&self.team_repo),
            team_member_repo: Arc::clone(&self.team_member_repo),
        }
    }
}
