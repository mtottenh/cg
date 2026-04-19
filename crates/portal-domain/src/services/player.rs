//! Player service with business logic.

use crate::entities::league_team::PlayerLeagueTeamMembership;
use crate::entities::Player;
use crate::repositories::league_team::LeagueTeamMemberRepository;
use crate::repositories::{PlayerRepository, PlayerSearchFilters, UpdatePlayer};
use portal_core::{DomainError, FieldError, LeagueSeasonId, PlayerId, UserId, ValidationError};
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
pub struct PlayerService<PR, LTMR>
where
    PR: PlayerRepository,
    LTMR: LeagueTeamMemberRepository,
{
    player_repo: Arc<PR>,
    league_team_member_repo: Arc<LTMR>,
}

impl<PR, LTMR> PlayerService<PR, LTMR>
where
    PR: PlayerRepository,
    LTMR: LeagueTeamMemberRepository,
{
    /// Create a new player service.
    pub const fn new(player_repo: Arc<PR>, league_team_member_repo: Arc<LTMR>) -> Self {
        Self {
            player_repo,
            league_team_member_repo,
        }
    }

    /// Get a player by ID.
    #[instrument(skip(self))]
    pub async fn get_player(&self, id: PlayerId) -> Result<Player, DomainError> {
        self.player_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::PlayerNotFound(id))
    }

    /// Get a player by user ID.
    #[instrument(skip(self))]
    pub async fn get_player_by_user_id(&self, user_id: UserId) -> Result<Player, DomainError> {
        self.player_repo
            .find_by_user_id(user_id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "player",
                query: format!("user:{user_id}"),
            })
    }

    /// Find a player by their SteamID64. Returns None if not found.
    #[instrument(skip(self))]
    pub async fn find_by_steam_id_64(
        &self,
        steam_id_64: i64,
    ) -> Result<Option<Player>, DomainError> {
        self.player_repo.find_by_steam_id_64(steam_id_64).await
    }

    /// Search players with filters.
    #[instrument(skip(self))]
    pub async fn search_players(
        &self,
        filters: &PlayerSearchFilters,
        limit: i64,
        offset: i64,
    ) -> Result<PlayerSearchResult, DomainError> {
        let players = self.player_repo.search(filters, limit, offset).await?;
        let total = self.player_repo.count_search(filters).await?;

        Ok(PlayerSearchResult { players, total })
    }

    /// Get all league team memberships for a player (with team/season/league details).
    ///
    /// This is the primary method for fetching a player's team affiliations
    /// across all leagues and seasons.
    #[instrument(skip(self))]
    pub async fn get_player_league_team_memberships(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerLeagueTeamMembership>, DomainError> {
        // Verify player exists
        let _player = self.get_player(player_id).await?;
        self.league_team_member_repo
            .list_memberships_for_player(player_id)
            .await
    }

    /// Get league team memberships for a player in a specific season.
    #[instrument(skip(self))]
    pub async fn get_player_league_team_memberships_in_season(
        &self,
        player_id: PlayerId,
        season_id: LeagueSeasonId,
    ) -> Result<Vec<PlayerLeagueTeamMembership>, DomainError> {
        self.league_team_member_repo
            .list_memberships_in_season(player_id, season_id)
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
        let player = self.get_player(player_id).await?;

        // Validate steam_id if provided
        if let Some(ref steam_id_str) = cmd.steam_id {
            // Immutability: reject if already set
            if player.steam_id.is_some() {
                return Err(DomainError::Conflict(
                    "Steam ID is already set and cannot be changed".into(),
                ));
            }
            // Validate: must parse as i64 in SteamID64 range
            let parsed: i64 = steam_id_str.parse().map_err(|_| {
                DomainError::Validation(ValidationError::field(FieldError::format(
                    "steam_id",
                    "a valid SteamID64 (e.g. 76561198012345678)",
                )))
            })?;
            if parsed < 76_561_197_960_265_728 {
                return Err(DomainError::Validation(ValidationError::field(
                    FieldError::format(
                        "steam_id",
                        "a valid SteamID64 (must be >= 76561197960265728)",
                    ),
                )));
            }
        }

        // Validate display name uniqueness if changing
        if let Some(ref new_name) = cmd.display_name {
            if let Some(existing) = self.player_repo.find_by_display_name(new_name).await? {
                if existing.id != player_id {
                    return Err(DomainError::Conflict(format!(
                        "Display name '{new_name}' is already taken"
                    )));
                }
            }
        }

        self.player_repo.update(player_id, cmd).await
    }
}

impl<PR, LTMR> Clone for PlayerService<PR, LTMR>
where
    PR: PlayerRepository,
    LTMR: LeagueTeamMemberRepository,
{
    fn clone(&self) -> Self {
        Self {
            player_repo: Arc::clone(&self.player_repo),
            league_team_member_repo: Arc::clone(&self.league_team_member_repo),
        }
    }
}
