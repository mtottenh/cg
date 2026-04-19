//! League team service.

use crate::entities::league_team::{
    CreateLeagueTeamCommand, LeagueTeam, LeagueTeamMember, LeagueTeamMemberWithPlayer,
    LeagueTeamSeason, LeagueTeamSummary, PlayerLeagueTeamMembership, UpdateLeagueTeamCommand,
};
use crate::repositories::league_team::{
    AddLeagueTeamMember, CreateLeagueTeam, CreateLeagueTeamSeason, LeagueSeasonRepository,
    LeagueTeamMemberRepository, LeagueTeamRepository, LeagueTeamSeasonRepository, UpdateLeagueTeam,
};
use portal_core::types::{LeagueTeamRole, LeagueTeamSeasonStatus, LeagueTeamStatus};
use portal_core::{
    DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamSeasonId, PlayerId,
};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for league team-related business logic.
///
/// Teams belong to leagues (not seasons) with persistent identity.
/// Seasonal participation is tracked via `LeagueTeamSeason`.
pub struct LeagueTeamService<TR, TSR, TMR, SR>
where
    TR: LeagueTeamRepository,
    TSR: LeagueTeamSeasonRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    team_repo: Arc<TR>,
    team_season_repo: Arc<TSR>,
    member_repo: Arc<TMR>,
    season_repo: Arc<SR>,
}

impl<TR, TSR, TMR, SR> LeagueTeamService<TR, TSR, TMR, SR>
where
    TR: LeagueTeamRepository,
    TSR: LeagueTeamSeasonRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    /// Create a new league team service.
    pub const fn new(
        team_repo: Arc<TR>,
        team_season_repo: Arc<TSR>,
        member_repo: Arc<TMR>,
        season_repo: Arc<SR>,
    ) -> Self {
        Self {
            team_repo,
            team_season_repo,
            member_repo,
            season_repo,
        }
    }

    /// Get a team by ID.
    #[instrument(skip(self))]
    pub async fn get_team(&self, id: LeagueTeamId) -> Result<LeagueTeam, DomainError> {
        self.team_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::LeagueTeamNotFound(id))
    }

    /// Get a team season by ID.
    #[instrument(skip(self))]
    pub async fn get_team_season(
        &self,
        id: LeagueTeamSeasonId,
    ) -> Result<LeagueTeamSeason, DomainError> {
        self.team_season_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed { resource: "league team season", query: id.to_string() })
    }

    /// Get team season by team and season.
    #[instrument(skip(self))]
    pub async fn get_team_season_by_ids(
        &self,
        team_id: LeagueTeamId,
        season_id: LeagueSeasonId,
    ) -> Result<Option<LeagueTeamSeason>, DomainError> {
        self.team_season_repo
            .find_by_team_and_season(team_id, season_id)
            .await
    }

    /// Get team members with player details for a team season.
    #[instrument(skip(self))]
    pub async fn get_members(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamMemberWithPlayer>, DomainError> {
        let _ = self.get_team_season(team_season_id).await?;
        self.member_repo.list_members_with_players(team_season_id).await
    }

    /// Create a new team in a league and register it for a season.
    ///
    /// The creating player becomes the team owner AND the first captain
    /// (captain is a role in the seasonal roster).
    #[instrument(skip(self))]
    pub async fn create_team(
        &self,
        creator_player_id: PlayerId,
        cmd: CreateLeagueTeamCommand,
    ) -> Result<(LeagueTeam, LeagueTeamSeason), DomainError> {
        // Verify the season exists and is accepting registrations
        let season = self
            .season_repo
            .find_by_id(cmd.season_id)
            .await?
            .ok_or_else(|| DomainError::LeagueSeasonNotFound(cmd.season_id))?;

        if !season.can_register_team() {
            return Err(DomainError::RegistrationClosed);
        }

        // Check if max teams limit is reached
        if let Some(max_teams) = season.max_teams {
            let team_count = self.season_repo.count_teams(cmd.season_id).await?;
            if team_count >= i64::from(max_teams) {
                return Err(DomainError::Conflict(
                    "maximum number of teams reached for this season".to_string(),
                ));
            }
        }

        // Check if player is already a primary member of another team in this season
        if let Some(existing_team_season_id) = self
            .member_repo
            .find_primary_team_in_season(cmd.season_id, creator_player_id)
            .await?
        {
            return Err(DomainError::Conflict(format!(
                "player is already a primary member of team {existing_team_season_id} in this season"
            )));
        }

        // Check name uniqueness within the league
        if self.team_repo.name_exists(season.league_id, &cmd.name).await? {
            return Err(DomainError::Conflict(format!(
                "team name '{}' is already taken in this league",
                cmd.name
            )));
        }

        // Check tag uniqueness within the league
        if self.team_repo.tag_exists(season.league_id, &cmd.tag).await? {
            return Err(DomainError::Conflict(format!(
                "team tag '{}' is already taken in this league",
                cmd.tag
            )));
        }

        // Create the team (persistent entity at league level)
        let team = self
            .team_repo
            .create(CreateLeagueTeam {
                league_id: season.league_id,
                name: cmd.name,
                tag: cmd.tag,
                description: cmd.description,
                logo_url: cmd.logo_url,
                primary_color: cmd.primary_color,
                secondary_color: cmd.secondary_color,
                owner_player_id: creator_player_id,
            })
            .await?;

        // Register the team for this season
        let team_season = self
            .team_season_repo
            .create(CreateLeagueTeamSeason {
                team_id: team.id,
                season_id: cmd.season_id,
            })
            .await?;

        // Add the creator as captain on the seasonal roster
        self.member_repo
            .add_member(AddLeagueTeamMember {
                team_season_id: team_season.id,
                player_id: creator_player_id,
                role: LeagueTeamRole::Captain,
                position: None,
                jersey_number: None,
                added_by: None,
            })
            .await?;

        info!(
            team_id = %team.id,
            team_season_id = %team_season.id,
            season_id = %cmd.season_id,
            creator_player_id = %creator_player_id,
            "League team created and registered for season"
        );

        Ok((team, team_season))
    }

    /// Register an existing team for a new season.
    #[instrument(skip(self))]
    pub async fn register_for_season(
        &self,
        team_id: LeagueTeamId,
        season_id: LeagueSeasonId,
        registering_player_id: PlayerId,
    ) -> Result<LeagueTeamSeason, DomainError> {
        let team = self.get_team(team_id).await?;
        let season = self
            .season_repo
            .find_by_id(season_id)
            .await?
            .ok_or_else(|| DomainError::LeagueSeasonNotFound(season_id))?;

        // Verify team belongs to this league
        if team.league_id != season.league_id {
            return Err(DomainError::NotAuthorized(
                "team does not belong to this league".to_string(),
            ));
        }

        // Verify the user is the owner
        if team.owner_player_id != registering_player_id {
            return Err(DomainError::NotAuthorized(
                "only the team owner can register for new seasons".to_string(),
            ));
        }

        // Verify season accepts registrations
        if !season.can_register_team() {
            return Err(DomainError::RegistrationClosed);
        }

        // Check if team is already registered for this season
        if self
            .team_season_repo
            .find_by_team_and_season(team_id, season_id)
            .await?
            .is_some()
        {
            return Err(DomainError::Conflict(
                "team is already registered for this season".to_string(),
            ));
        }

        // Check if max teams limit is reached
        if let Some(max_teams) = season.max_teams {
            let team_count = self.season_repo.count_teams(season_id).await?;
            if team_count >= i64::from(max_teams) {
                return Err(DomainError::Conflict(
                    "maximum number of teams reached for this season".to_string(),
                ));
            }
        }

        // Register the team for this season
        let team_season = self
            .team_season_repo
            .create(CreateLeagueTeamSeason {
                team_id,
                season_id,
            })
            .await?;

        // Add the owner as captain on the seasonal roster
        self.member_repo
            .add_member(AddLeagueTeamMember {
                team_season_id: team_season.id,
                player_id: registering_player_id,
                role: LeagueTeamRole::Captain,
                position: None,
                jersey_number: None,
                added_by: None,
            })
            .await?;

        info!(
            team_id = %team_id,
            team_season_id = %team_season.id,
            season_id = %season_id,
            "Existing team registered for new season"
        );

        Ok(team_season)
    }

    /// Update a team's persistent info (name, logo, etc.).
    #[instrument(skip(self))]
    pub async fn update_team_authorized(
        &self,
        team_id: LeagueTeamId,
        cmd: UpdateLeagueTeamCommand,
    ) -> Result<LeagueTeam, DomainError> {
        let team = self.get_team(team_id).await?;

        // Check name uniqueness if changing
        if let Some(ref name) = cmd.name {
            if name.to_lowercase() != team.name.to_lowercase()
                && self.team_repo.name_exists(team.league_id, name).await?
            {
                return Err(DomainError::Conflict(format!(
                    "team name '{name}' is already taken in this league"
                )));
            }
        }

        // Check tag uniqueness if changing
        if let Some(ref tag) = cmd.tag {
            if tag.to_lowercase() != team.tag.to_lowercase()
                && self.team_repo.tag_exists(team.league_id, tag).await?
            {
                return Err(DomainError::Conflict(format!(
                    "team tag '{tag}' is already taken in this league"
                )));
            }
        }

        let updated = self
            .team_repo
            .update(
                team_id,
                UpdateLeagueTeam {
                    name: cmd.name,
                    tag: cmd.tag,
                    description: cmd.description,
                    logo_url: cmd.logo_url,
                    banner_url: cmd.banner_url,
                    primary_color: cmd.primary_color,
                    secondary_color: cmd.secondary_color,
                },
            )
            .await?;

        info!(team_id = %team_id, "League team updated");

        Ok(updated)
    }

    /// List teams in a league.
    #[instrument(skip(self))]
    pub async fn list_teams(
        &self,
        league_id: LeagueId,
        status_filter: Option<LeagueTeamStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeam>, i64), DomainError> {
        self.team_repo
            .list_by_league(league_id, status_filter, search, limit, offset)
            .await
    }

    /// List team season registrations for a season.
    #[instrument(skip(self))]
    pub async fn list_team_seasons(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<LeagueTeamSeasonStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeamSeason>, i64), DomainError> {
        self.team_season_repo
            .list_by_season(season_id, status_filter, search, limit, offset)
            .await
    }

    /// List team summaries with member counts.
    #[instrument(skip(self))]
    pub async fn list_team_summaries(
        &self,
        season_id: LeagueSeasonId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeamSummary>, i64), DomainError> {
        self.team_season_repo
            .list_summaries(season_id, limit, offset)
            .await
    }

    /// Add a member to a team's seasonal roster.
    #[instrument(skip(self))]
    pub async fn add_member_authorized(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        role: LeagueTeamRole,
        added_by: portal_core::UserId,
    ) -> Result<LeagueTeamMember, DomainError> {
        let team_season = self.get_team_season(team_season_id).await?;
        let season = self
            .season_repo
            .find_by_id(team_season.season_id)
            .await?
            .ok_or_else(|| {
                DomainError::LeagueSeasonNotFound(team_season.season_id)
            })?;

        // Check roster lock status
        if role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for primary member changes".to_string(),
            ));
        }

        if !role.is_primary() && !season.allows_substitute_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for substitute changes".to_string(),
            ));
        }

        // Check if player is already a member of this team season
        if self
            .member_repo
            .is_member(team_season_id, player_id)
            .await?
        {
            return Err(DomainError::AlreadyTeamMember);
        }

        // For primary roles, check one-team-per-season constraint
        if role.is_primary() {
            if let Some(existing_team_season_id) = self
                .member_repo
                .find_primary_team_in_season(team_season.season_id, player_id)
                .await?
            {
                return Err(DomainError::Conflict(format!(
                    "player is already a primary member of team {existing_team_season_id} in this season"
                )));
            }
        }

        // Check roster size limits
        if role.is_primary() {
            let primary_count = self
                .member_repo
                .count_primary_members(team_season_id)
                .await?;
            if let Some(max) = season.team_size_max {
                if primary_count >= i64::from(max) {
                    return Err(DomainError::TeamFull);
                }
            }
        } else {
            let sub_count = self.member_repo.count_substitutes(team_season_id).await?;
            if let Some(max_subs) = season.max_substitutes {
                if sub_count >= i64::from(max_subs) {
                    return Err(DomainError::Conflict(
                        "maximum number of substitutes reached".to_string(),
                    ));
                }
            }
        }

        let member = self
            .member_repo
            .add_member(AddLeagueTeamMember {
                team_season_id,
                player_id,
                role,
                position: None,
                jersey_number: None,
                added_by: Some(added_by),
            })
            .await?;

        info!(
            team_season_id = %team_season_id,
            player_id = %player_id,
            role = %role,
            "Member added to league team"
        );

        Ok(member)
    }

    /// Remove a member from a team's seasonal roster.
    #[instrument(skip(self))]
    pub async fn remove_member_authorized(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        let team_season = self.get_team_season(team_season_id).await?;
        let season = self
            .season_repo
            .find_by_id(team_season.season_id)
            .await?
            .ok_or_else(|| {
                DomainError::LeagueSeasonNotFound(team_season.season_id)
            })?;

        let member = self
            .member_repo
            .find_member(team_season_id, player_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        // Check roster lock status based on member role
        if member.role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for primary member changes".to_string(),
            ));
        }

        if !member.role.is_primary() && !season.allows_substitute_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for substitute changes".to_string(),
            ));
        }

        // If removing a captain, ensure at least one captain remains
        if member.role == LeagueTeamRole::Captain {
            let captain_count = self.member_repo.count_captains(team_season_id).await?;
            if captain_count <= 1 {
                return Err(DomainError::Conflict(
                    "cannot remove last captain; promote another member first".to_string(),
                ));
            }
        }

        self.member_repo
            .remove_member(team_season_id, player_id)
            .await?;

        info!(
            team_season_id = %team_season_id,
            player_id = %player_id,
            "Member removed from league team"
        );

        Ok(())
    }

    /// Promote a member to captain (multiple captains allowed).
    #[instrument(skip(self))]
    pub async fn promote_to_captain(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<LeagueTeamMember, DomainError> {
        let member = self
            .member_repo
            .find_member(team_season_id, player_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        if member.role == LeagueTeamRole::Captain {
            return Err(DomainError::Conflict("member is already a captain".to_string()));
        }

        if !member.is_active() {
            return Err(DomainError::InvalidState(
                "cannot promote inactive member to captain".to_string(),
            ));
        }

        let updated = self
            .member_repo
            .update_role(team_season_id, player_id, LeagueTeamRole::Captain)
            .await?;

        info!(
            team_season_id = %team_season_id,
            player_id = %player_id,
            "Member promoted to captain"
        );

        Ok(updated)
    }

    /// Demote a captain to player.
    #[instrument(skip(self))]
    pub async fn demote_from_captain(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<LeagueTeamMember, DomainError> {
        let member = self
            .member_repo
            .find_member(team_season_id, player_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        if member.role != LeagueTeamRole::Captain {
            return Err(DomainError::Conflict("member is not a captain".to_string()));
        }

        // Ensure at least one captain remains
        let captain_count = self.member_repo.count_captains(team_season_id).await?;
        if captain_count <= 1 {
            return Err(DomainError::Conflict(
                "cannot demote last captain; promote another member first".to_string(),
            ));
        }

        let updated = self
            .member_repo
            .update_role(team_season_id, player_id, LeagueTeamRole::Player)
            .await?;

        info!(
            team_season_id = %team_season_id,
            player_id = %player_id,
            "Captain demoted to player"
        );

        Ok(updated)
    }

    /// Transfer team ownership (permanent owner, not captain).
    #[instrument(skip(self))]
    pub async fn transfer_ownership(
        &self,
        team_id: LeagueTeamId,
        current_owner_player_id: PlayerId,
        new_owner_player_id: PlayerId,
    ) -> Result<LeagueTeam, DomainError> {
        let team = self.get_team(team_id).await?;

        // Verify current owner
        if team.owner_player_id != current_owner_player_id {
            return Err(DomainError::NotAuthorized(
                "only the team owner can transfer ownership".to_string(),
            ));
        }

        let updated = self
            .team_repo
            .transfer_ownership(team_id, new_owner_player_id)
            .await?;

        info!(
            team_id = %team_id,
            from_owner = %current_owner_player_id,
            to_owner = %new_owner_player_id,
            "Team ownership transferred"
        );

        Ok(updated)
    }

    /// Leave a team voluntarily (from a seasonal roster).
    #[instrument(skip(self))]
    pub async fn leave_team(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        let team_season = self.get_team_season(team_season_id).await?;

        let member = self
            .member_repo
            .find_member(team_season_id, player_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        // If captain, ensure at least one captain remains
        if member.role == LeagueTeamRole::Captain {
            let captain_count = self.member_repo.count_captains(team_season_id).await?;
            if captain_count <= 1 {
                return Err(DomainError::Conflict(
                    "captain cannot leave; promote another member to captain first".to_string(),
                ));
            }
        }

        let season = self
            .season_repo
            .find_by_id(team_season.season_id)
            .await?
            .ok_or_else(|| {
                DomainError::LeagueSeasonNotFound(team_season.season_id)
            })?;

        // Check roster lock status
        if member.role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked; cannot leave team".to_string(),
            ));
        }

        self.member_repo
            .remove_member(team_season_id, player_id)
            .await?;

        info!(team_season_id = %team_season_id, player_id = %player_id, "Player left league team");

        Ok(())
    }

    /// Withdraw a team from a season.
    #[instrument(skip(self))]
    pub async fn withdraw_from_season(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<LeagueTeamSeason, DomainError> {
        let team_season = self.get_team_season(team_season_id).await?;

        if team_season.status.is_terminal() {
            return Err(DomainError::InvalidState(
                "team season is already in a terminal state".to_string(),
            ));
        }

        let updated = self
            .team_season_repo
            .update_status(team_season_id, LeagueTeamSeasonStatus::Withdrawn)
            .await?;

        info!(team_season_id = %team_season_id, "Team withdrawn from season");

        Ok(updated)
    }

    /// Disband a team permanently.
    #[instrument(skip(self))]
    pub async fn disband_team(&self, team_id: LeagueTeamId) -> Result<(), DomainError> {
        let team = self.get_team(team_id).await?;

        if team.status.is_terminal() {
            return Err(DomainError::InvalidState(
                "team is already in a terminal state".to_string(),
            ));
        }

        self.team_repo
            .update_status(team_id, LeagueTeamStatus::Disbanded)
            .await?;

        info!(team_id = %team_id, "League team disbanded");

        Ok(())
    }

    /// Get player's team memberships across all seasons.
    #[instrument(skip(self))]
    pub async fn get_player_memberships(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerLeagueTeamMembership>, DomainError> {
        self.member_repo.list_memberships_for_player(player_id).await
    }

    /// Check if a player is a member of a team season.
    pub async fn is_member(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        self.member_repo.is_member(team_season_id, player_id).await
    }

    /// Check if a player is a captain of a team season.
    pub async fn is_captain(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        self.member_repo.is_captain(team_season_id, player_id).await
    }

    /// Check if a player is the team owner.
    pub async fn is_owner(
        &self,
        team_id: LeagueTeamId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        let team = self.get_team(team_id).await?;
        Ok(team.owner_player_id == player_id)
    }
}

impl<TR, TSR, TMR, SR> Clone for LeagueTeamService<TR, TSR, TMR, SR>
where
    TR: LeagueTeamRepository,
    TSR: LeagueTeamSeasonRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    fn clone(&self) -> Self {
        Self {
            team_repo: Arc::clone(&self.team_repo),
            team_season_repo: Arc::clone(&self.team_season_repo),
            member_repo: Arc::clone(&self.member_repo),
            season_repo: Arc::clone(&self.season_repo),
        }
    }
}
