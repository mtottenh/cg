//! League team services with business logic.
//!
//! This module contains services for managing league-scoped teams, seasons,
//! and team memberships.

use crate::entities::league_team::{
    CreateLeagueSeasonCommand, CreateLeagueTeamCommand, LeagueSeason, LeagueTeam,
    LeagueTeamInvitation, LeagueTeamInvitationWithTeam, LeagueTeamMember, LeagueTeamMemberWithUser,
    LeagueTeamSummary, UpdateLeagueSeasonCommand, UpdateLeagueTeamCommand,
    UserLeagueTeamMembership,
};
use crate::repositories::league_team::{
    AddLeagueTeamMember, CreateLeagueSeason, CreateLeagueTeam, CreateLeagueTeamInvitation,
    LeagueSeasonRepository, LeagueTeamInvitationRepository, LeagueTeamMemberRepository,
    LeagueTeamRepository, UpdateLeagueSeason, UpdateLeagueTeam,
};
use crate::repositories::LeagueRepository;
use portal_core::types::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamRole, LeagueTeamStatus,
    RosterLockStatus, SeasonStatus,
};
use portal_core::{DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamInvitationId, UserId};
use std::sync::Arc;
use tracing::{info, instrument};

// =============================================================================
// LEAGUE SEASON SERVICE
// =============================================================================

/// Service for league season-related business logic.
pub struct LeagueSeasonService<SR, LR>
where
    SR: LeagueSeasonRepository,
    LR: LeagueRepository,
{
    season_repo: Arc<SR>,
    league_repo: Arc<LR>,
}

impl<SR, LR> LeagueSeasonService<SR, LR>
where
    SR: LeagueSeasonRepository,
    LR: LeagueRepository,
{
    /// Create a new league season service.
    pub fn new(season_repo: Arc<SR>, league_repo: Arc<LR>) -> Self {
        Self {
            season_repo,
            league_repo,
        }
    }

    /// Get a season by ID.
    #[instrument(skip(self))]
    pub async fn get_season(&self, id: LeagueSeasonId) -> Result<LeagueSeason, DomainError> {
        self.season_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", id.to_string()))
    }

    /// Get a season by league and slug.
    #[instrument(skip(self))]
    pub async fn get_season_by_slug(
        &self,
        league_id: LeagueId,
        slug: &str,
    ) -> Result<LeagueSeason, DomainError> {
        self.season_repo
            .find_by_slug(league_id, slug)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", slug.to_string()))
    }

    /// Create a new season for a league.
    #[instrument(skip(self))]
    pub async fn create_season(
        &self,
        creator_id: UserId,
        cmd: CreateLeagueSeasonCommand,
    ) -> Result<LeagueSeason, DomainError> {
        // Verify the league exists
        let _league = self
            .league_repo
            .find_by_id(cmd.league_id)
            .await?
            .ok_or_else(|| DomainError::LeagueNotFound(cmd.league_id.to_string()))?;

        // Check slug uniqueness within the league
        if self
            .season_repo
            .slug_exists(cmd.league_id, &cmd.slug)
            .await?
        {
            return Err(DomainError::Conflict(format!(
                "season slug '{}' is already taken in this league",
                cmd.slug
            )));
        }

        let season = self
            .season_repo
            .create(CreateLeagueSeason {
                league_id: cmd.league_id,
                name: cmd.name,
                slug: cmd.slug,
                description: cmd.description,
                registration_start: cmd.registration_start,
                registration_end: cmd.registration_end,
                season_start: cmd.season_start,
                season_end: cmd.season_end,
                team_size_min: cmd.team_size_min.unwrap_or(1),
                team_size_max: cmd.team_size_max.unwrap_or(5),
                max_substitutes: cmd.max_substitutes.unwrap_or(2),
                max_teams: cmd.max_teams,
                created_by: creator_id,
            })
            .await?;

        info!(
            season_id = %season.id,
            league_id = %cmd.league_id,
            creator_id = %creator_id,
            "League season created"
        );

        Ok(season)
    }

    /// Update a season.
    #[instrument(skip(self))]
    pub async fn update_season(
        &self,
        id: LeagueSeasonId,
        cmd: UpdateLeagueSeasonCommand,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        // Check slug uniqueness if changing
        if let Some(ref slug) = cmd.slug {
            if slug != &season.slug && self.season_repo.slug_exists(season.league_id, slug).await? {
                return Err(DomainError::Conflict(format!(
                    "season slug '{}' is already taken in this league",
                    slug
                )));
            }
        }

        let updated = self
            .season_repo
            .update(
                id,
                UpdateLeagueSeason {
                    name: cmd.name,
                    slug: cmd.slug,
                    description: cmd.description,
                    registration_start: cmd.registration_start,
                    registration_end: cmd.registration_end,
                    season_start: cmd.season_start,
                    season_end: cmd.season_end,
                    team_size_min: cmd.team_size_min,
                    team_size_max: cmd.team_size_max,
                    max_substitutes: cmd.max_substitutes,
                    max_teams: cmd.max_teams,
                    status: cmd.status,
                    settings: cmd.settings,
                },
            )
            .await?;

        info!(season_id = %id, "League season updated");

        Ok(updated)
    }

    /// List seasons for a league.
    #[instrument(skip(self))]
    pub async fn list_seasons(&self, league_id: LeagueId) -> Result<Vec<LeagueSeason>, DomainError> {
        self.season_repo.list_by_league(league_id).await
    }

    /// Update roster lock status.
    #[instrument(skip(self))]
    pub async fn update_roster_lock(
        &self,
        id: LeagueSeasonId,
        status: RosterLockStatus,
        locked_by: UserId,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        // Can only lock rosters during registration or active season
        if !season.status.allows_roster_changes() && status != RosterLockStatus::Open {
            return Err(DomainError::InvalidState(
                "cannot modify roster lock in current season state".to_string(),
            ));
        }

        let updated = self
            .season_repo
            .update_roster_lock(id, status, Some(locked_by))
            .await?;

        info!(
            season_id = %id,
            roster_lock = %status,
            locked_by = %locked_by,
            "Roster lock status updated"
        );

        Ok(updated)
    }

    /// Update season status.
    #[instrument(skip(self))]
    pub async fn update_status(
        &self,
        id: LeagueSeasonId,
        status: SeasonStatus,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        // Validate status transitions
        let valid_transition = match (&season.status, &status) {
            (SeasonStatus::Draft, SeasonStatus::Registration) => true,
            (SeasonStatus::Registration, SeasonStatus::Active) => true,
            (SeasonStatus::Active, SeasonStatus::Playoffs) => true,
            (SeasonStatus::Playoffs, SeasonStatus::Completed) => true,
            (SeasonStatus::Active, SeasonStatus::Completed) => true, // Direct completion without playoffs
            (_, SeasonStatus::Cancelled) => !season.status.is_terminal(),
            _ => false,
        };

        if !valid_transition {
            return Err(DomainError::InvalidState(format!(
                "cannot transition season from {} to {}",
                season.status, status
            )));
        }

        let updated = self.season_repo.update_status(id, status).await?;

        info!(
            season_id = %id,
            from_status = %season.status,
            to_status = %status,
            "Season status updated"
        );

        Ok(updated)
    }
}

impl<SR, LR> Clone for LeagueSeasonService<SR, LR>
where
    SR: LeagueSeasonRepository,
    LR: LeagueRepository,
{
    fn clone(&self) -> Self {
        Self {
            season_repo: Arc::clone(&self.season_repo),
            league_repo: Arc::clone(&self.league_repo),
        }
    }
}

// =============================================================================
// LEAGUE TEAM SERVICE
// =============================================================================

/// Service for league team-related business logic.
pub struct LeagueTeamService<TR, TMR, SR>
where
    TR: LeagueTeamRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    team_repo: Arc<TR>,
    member_repo: Arc<TMR>,
    season_repo: Arc<SR>,
}

impl<TR, TMR, SR> LeagueTeamService<TR, TMR, SR>
where
    TR: LeagueTeamRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    /// Create a new league team service.
    pub fn new(team_repo: Arc<TR>, member_repo: Arc<TMR>, season_repo: Arc<SR>) -> Self {
        Self {
            team_repo,
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
            .ok_or_else(|| DomainError::not_found("league team", id.to_string()))
    }

    /// Get team members with user details.
    #[instrument(skip(self))]
    pub async fn get_members(
        &self,
        team_id: LeagueTeamId,
    ) -> Result<Vec<LeagueTeamMemberWithUser>, DomainError> {
        let _ = self.get_team(team_id).await?;
        self.member_repo.list_members_with_users(team_id).await
    }

    /// Create a new team in a season.
    ///
    /// The creating user automatically becomes the captain.
    #[instrument(skip(self))]
    pub async fn create_team(
        &self,
        creator_id: UserId,
        cmd: CreateLeagueTeamCommand,
    ) -> Result<LeagueTeam, DomainError> {
        // Verify the season exists and is accepting registrations
        let season = self
            .season_repo
            .find_by_id(cmd.season_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", cmd.season_id.to_string()))?;

        if !season.can_create_team() {
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

        // Check if user is already a primary member of another team in this season
        if let Some(existing_team_id) = self
            .member_repo
            .find_primary_team_in_season(cmd.season_id, creator_id)
            .await?
        {
            return Err(DomainError::Conflict(format!(
                "user is already a primary member of team {} in this season",
                existing_team_id
            )));
        }

        // Check name uniqueness within the season
        if self.team_repo.name_exists(cmd.season_id, &cmd.name).await? {
            return Err(DomainError::Conflict(format!(
                "team name '{}' is already taken in this season",
                cmd.name
            )));
        }

        // Check tag uniqueness within the season
        if self.team_repo.tag_exists(cmd.season_id, &cmd.tag).await? {
            return Err(DomainError::Conflict(format!(
                "team tag '{}' is already taken in this season",
                cmd.tag
            )));
        }

        // Create the team
        let team = self
            .team_repo
            .create(CreateLeagueTeam {
                season_id: cmd.season_id,
                name: cmd.name,
                tag: cmd.tag,
                description: cmd.description,
                logo_url: cmd.logo_url,
                primary_color: cmd.primary_color,
                secondary_color: cmd.secondary_color,
                captain_user_id: creator_id,
            })
            .await?;

        // Add the creator as captain
        self.member_repo
            .add_member(AddLeagueTeamMember {
                team_id: team.id,
                user_id: creator_id,
                role: LeagueTeamRole::Captain,
                position: None,
                jersey_number: None,
                added_by: None,
            })
            .await?;

        info!(
            team_id = %team.id,
            season_id = %cmd.season_id,
            creator_id = %creator_id,
            "League team created"
        );

        Ok(team)
    }

    /// Update a team (authorized version - caller must verify permissions).
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
                && self.team_repo.name_exists(team.season_id, name).await?
            {
                return Err(DomainError::Conflict(format!(
                    "team name '{}' is already taken in this season",
                    name
                )));
            }
        }

        // Check tag uniqueness if changing
        if let Some(ref tag) = cmd.tag {
            if tag.to_lowercase() != team.tag.to_lowercase()
                && self.team_repo.tag_exists(team.season_id, tag).await?
            {
                return Err(DomainError::Conflict(format!(
                    "team tag '{}' is already taken in this season",
                    tag
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

    /// List teams in a season.
    #[instrument(skip(self))]
    pub async fn list_teams(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<LeagueTeamStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeam>, i64), DomainError> {
        self.team_repo
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
        self.team_repo.list_summaries(season_id, limit, offset).await
    }

    /// Add a member to a team (authorized version).
    #[instrument(skip(self))]
    pub async fn add_member_authorized(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
        role: LeagueTeamRole,
        added_by: UserId,
    ) -> Result<LeagueTeamMember, DomainError> {
        let team = self.get_team(team_id).await?;
        let season = self
            .season_repo
            .find_by_id(team.season_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", team.season_id.to_string()))?;

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

        // Check if user is already a member of this team
        if self.member_repo.is_member(team_id, user_id).await? {
            return Err(DomainError::AlreadyTeamMember);
        }

        // For primary roles, check one-team-per-season constraint
        if role.is_primary() {
            if let Some(existing_team_id) = self
                .member_repo
                .find_primary_team_in_season(team.season_id, user_id)
                .await?
            {
                return Err(DomainError::Conflict(format!(
                    "user is already a primary member of team {} in this season",
                    existing_team_id
                )));
            }
        }

        // Check roster size limits
        if role.is_primary() {
            let primary_count = self.member_repo.count_primary_members(team_id).await?;
            if primary_count >= i64::from(season.team_size_max) {
                return Err(DomainError::TeamFull);
            }
        } else {
            let sub_count = self.member_repo.count_substitutes(team_id).await?;
            if sub_count >= i64::from(season.max_substitutes) {
                return Err(DomainError::Conflict(
                    "maximum number of substitutes reached".to_string(),
                ));
            }
        }

        let member = self
            .member_repo
            .add_member(AddLeagueTeamMember {
                team_id,
                user_id,
                role,
                position: None,
                jersey_number: None,
                added_by: Some(added_by),
            })
            .await?;

        info!(
            team_id = %team_id,
            user_id = %user_id,
            role = %role,
            "Member added to league team"
        );

        Ok(member)
    }

    /// Remove a member from a team (authorized version).
    #[instrument(skip(self))]
    pub async fn remove_member_authorized(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
    ) -> Result<(), DomainError> {
        let team = self.get_team(team_id).await?;
        let season = self
            .season_repo
            .find_by_id(team.season_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", team.season_id.to_string()))?;

        let member = self
            .member_repo
            .find_member(team_id, user_id)
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

        // Cannot remove the captain without transferring captaincy first
        if member.role == LeagueTeamRole::Captain {
            return Err(DomainError::Conflict(
                "cannot remove captain; transfer captaincy first".to_string(),
            ));
        }

        self.member_repo.remove_member(team_id, user_id).await?;

        info!(
            team_id = %team_id,
            user_id = %user_id,
            "Member removed from league team"
        );

        Ok(())
    }

    /// Transfer captaincy to another member.
    #[instrument(skip(self))]
    pub async fn transfer_captain(
        &self,
        team_id: LeagueTeamId,
        current_captain_id: UserId,
        new_captain_id: UserId,
    ) -> Result<(), DomainError> {
        let team = self.get_team(team_id).await?;

        // Verify current captain
        if team.captain_user_id != current_captain_id {
            return Err(DomainError::NotAuthorized(
                "only the captain can transfer captaincy".to_string(),
            ));
        }

        // Verify new captain is a member
        let new_captain_member = self
            .member_repo
            .find_member(team_id, new_captain_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        if !new_captain_member.is_active() {
            return Err(DomainError::InvalidState(
                "cannot transfer captaincy to inactive member".to_string(),
            ));
        }

        // Update the team's captain
        self.team_repo.update_captain(team_id, new_captain_id).await?;

        // Update member roles
        self.member_repo
            .update_role(team_id, current_captain_id, LeagueTeamRole::Player)
            .await?;
        self.member_repo
            .update_role(team_id, new_captain_id, LeagueTeamRole::Captain)
            .await?;

        info!(
            team_id = %team_id,
            from_captain = %current_captain_id,
            to_captain = %new_captain_id,
            "Captaincy transferred"
        );

        Ok(())
    }

    /// Leave a team voluntarily.
    #[instrument(skip(self))]
    pub async fn leave_team(&self, team_id: LeagueTeamId, user_id: UserId) -> Result<(), DomainError> {
        let team = self.get_team(team_id).await?;

        let member = self
            .member_repo
            .find_member(team_id, user_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        // Captain cannot leave - must transfer captaincy first
        if member.role == LeagueTeamRole::Captain {
            return Err(DomainError::Conflict(
                "captain cannot leave; transfer captaincy first".to_string(),
            ));
        }

        let season = self
            .season_repo
            .find_by_id(team.season_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", team.season_id.to_string()))?;

        // Check roster lock status
        if member.role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked; cannot leave team".to_string(),
            ));
        }

        self.member_repo.remove_member(team_id, user_id).await?;

        info!(team_id = %team_id, user_id = %user_id, "User left league team");

        Ok(())
    }

    /// Disband a team.
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

    /// Get user's team memberships across all seasons.
    #[instrument(skip(self))]
    pub async fn get_user_memberships(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UserLeagueTeamMembership>, DomainError> {
        self.member_repo.list_memberships_for_user(user_id).await
    }

    /// Check if a user is a member of a team.
    pub async fn is_member(&self, team_id: LeagueTeamId, user_id: UserId) -> Result<bool, DomainError> {
        self.member_repo.is_member(team_id, user_id).await
    }

    /// Check if a user is the captain of a team.
    pub async fn is_captain(&self, team_id: LeagueTeamId, user_id: UserId) -> Result<bool, DomainError> {
        self.member_repo.is_captain(team_id, user_id).await
    }
}

impl<TR, TMR, SR> Clone for LeagueTeamService<TR, TMR, SR>
where
    TR: LeagueTeamRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    fn clone(&self) -> Self {
        Self {
            team_repo: Arc::clone(&self.team_repo),
            member_repo: Arc::clone(&self.member_repo),
            season_repo: Arc::clone(&self.season_repo),
        }
    }
}

// =============================================================================
// LEAGUE TEAM INVITATION SERVICE
// =============================================================================

/// Service for league team invitation business logic.
pub struct LeagueTeamInvitationService<IR, TR, TMR, SR>
where
    IR: LeagueTeamInvitationRepository,
    TR: LeagueTeamRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    invitation_repo: Arc<IR>,
    team_repo: Arc<TR>,
    member_repo: Arc<TMR>,
    season_repo: Arc<SR>,
}

impl<IR, TR, TMR, SR> LeagueTeamInvitationService<IR, TR, TMR, SR>
where
    IR: LeagueTeamInvitationRepository,
    TR: LeagueTeamRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    /// Create a new invitation service.
    pub fn new(
        invitation_repo: Arc<IR>,
        team_repo: Arc<TR>,
        member_repo: Arc<TMR>,
        season_repo: Arc<SR>,
    ) -> Self {
        Self {
            invitation_repo,
            team_repo,
            member_repo,
            season_repo,
        }
    }

    /// Create an invitation (captain invites player).
    #[instrument(skip(self))]
    pub async fn create_invitation(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
        role: LeagueTeamRole,
        message: Option<String>,
        invited_by: UserId,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let team = self
            .team_repo
            .find_by_id(team_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league team", team_id.to_string()))?;

        let season = self
            .season_repo
            .find_by_id(team.season_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", team.season_id.to_string()))?;

        // Check roster lock status
        if role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for primary member invitations".to_string(),
            ));
        }

        // Check if user is already a member
        if self.member_repo.is_member(team_id, user_id).await? {
            return Err(DomainError::AlreadyTeamMember);
        }

        // For primary roles, check one-team-per-season constraint
        if role.is_primary() {
            if let Some(existing_team_id) = self
                .member_repo
                .find_primary_team_in_season(team.season_id, user_id)
                .await?
            {
                return Err(DomainError::Conflict(format!(
                    "user is already a primary member of team {} in this season",
                    existing_team_id
                )));
            }
        }

        // Check for existing pending invitation
        if self
            .invitation_repo
            .find_existing_pending(team_id, user_id)
            .await?
            .is_some()
        {
            return Err(DomainError::InvitationAlreadyExists);
        }

        let invitation = self
            .invitation_repo
            .create(CreateLeagueTeamInvitation {
                team_id,
                user_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role,
                message,
                invited_by: Some(invited_by),
            })
            .await?;

        info!(
            invitation_id = %invitation.id,
            team_id = %team_id,
            user_id = %user_id,
            "League team invitation created"
        );

        Ok(invitation)
    }

    /// Create a join request (player requests to join).
    #[instrument(skip(self))]
    pub async fn create_join_request(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
        role: LeagueTeamRole,
        message: Option<String>,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let team = self
            .team_repo
            .find_by_id(team_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league team", team_id.to_string()))?;

        let season = self
            .season_repo
            .find_by_id(team.season_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", team.season_id.to_string()))?;

        if !season.is_registration_open() {
            return Err(DomainError::RegistrationClosed);
        }

        // Check if user is already a member
        if self.member_repo.is_member(team_id, user_id).await? {
            return Err(DomainError::AlreadyTeamMember);
        }

        // For primary roles, check one-team-per-season constraint
        if role.is_primary() {
            if let Some(existing_team_id) = self
                .member_repo
                .find_primary_team_in_season(team.season_id, user_id)
                .await?
            {
                return Err(DomainError::Conflict(format!(
                    "user is already a primary member of team {} in this season",
                    existing_team_id
                )));
            }
        }

        // Check for existing pending request
        if self
            .invitation_repo
            .find_existing_pending(team_id, user_id)
            .await?
            .is_some()
        {
            return Err(DomainError::InvitationAlreadyExists);
        }

        let invitation = self
            .invitation_repo
            .create(CreateLeagueTeamInvitation {
                team_id,
                user_id,
                invitation_type: LeagueTeamInvitationType::Request,
                role,
                message,
                invited_by: None,
            })
            .await?;

        info!(
            invitation_id = %invitation.id,
            team_id = %team_id,
            user_id = %user_id,
            "League team join request created"
        );

        Ok(invitation)
    }

    /// Accept an invitation/request.
    #[instrument(skip(self))]
    pub async fn accept_invitation(
        &self,
        invitation_id: LeagueTeamInvitationId,
        accepted_by: UserId,
    ) -> Result<LeagueTeamMember, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league team invitation", invitation_id.to_string()))?;

        if !invitation.is_actionable() {
            if invitation.is_expired() {
                return Err(DomainError::InvitationExpired);
            }
            return Err(DomainError::InvitationInvalid);
        }

        // Verify the acceptor is the appropriate party
        match invitation.invitation_type {
            LeagueTeamInvitationType::Invite => {
                // Invitee accepts
                if invitation.user_id != accepted_by {
                    return Err(DomainError::NotAuthorized(
                        "only the invited user can accept this invitation".to_string(),
                    ));
                }
            }
            LeagueTeamInvitationType::Request => {
                // Team captain accepts
                let team = self
                    .team_repo
                    .find_by_id(invitation.team_id)
                    .await?
                    .ok_or_else(|| {
                        DomainError::not_found("league team", invitation.team_id.to_string())
                    })?;

                if team.captain_user_id != accepted_by {
                    return Err(DomainError::NotAuthorized(
                        "only the team captain can accept join requests".to_string(),
                    ));
                }
            }
        }

        let team = self
            .team_repo
            .find_by_id(invitation.team_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league team", invitation.team_id.to_string()))?;

        let season = self
            .season_repo
            .find_by_id(team.season_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league season", team.season_id.to_string()))?;

        // Re-verify roster lock status
        if invitation.role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for primary member changes".to_string(),
            ));
        }

        // Re-verify one-team-per-season constraint for primary roles
        if invitation.role.is_primary() {
            if let Some(existing_team_id) = self
                .member_repo
                .find_primary_team_in_season(team.season_id, invitation.user_id)
                .await?
            {
                return Err(DomainError::Conflict(format!(
                    "user is already a primary member of team {} in this season",
                    existing_team_id
                )));
            }
        }

        // Check roster size limits
        if invitation.role.is_primary() {
            let primary_count = self.member_repo.count_primary_members(invitation.team_id).await?;
            if primary_count >= i64::from(season.team_size_max) {
                return Err(DomainError::TeamFull);
            }
        } else {
            let sub_count = self.member_repo.count_substitutes(invitation.team_id).await?;
            if sub_count >= i64::from(season.max_substitutes) {
                return Err(DomainError::Conflict(
                    "maximum number of substitutes reached".to_string(),
                ));
            }
        }

        // Update invitation status
        self.invitation_repo
            .update_status(invitation_id, LeagueTeamInvitationStatus::Accepted, None)
            .await?;

        // Add member to team
        let member = self
            .member_repo
            .add_member(AddLeagueTeamMember {
                team_id: invitation.team_id,
                user_id: invitation.user_id,
                role: invitation.role,
                position: None,
                jersey_number: None,
                added_by: invitation.invited_by,
            })
            .await?;

        info!(
            invitation_id = %invitation_id,
            team_id = %invitation.team_id,
            user_id = %invitation.user_id,
            "League team invitation accepted"
        );

        Ok(member)
    }

    /// Decline an invitation/request.
    #[instrument(skip(self))]
    pub async fn decline_invitation(
        &self,
        invitation_id: LeagueTeamInvitationId,
        declined_by: UserId,
        response_message: Option<String>,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league team invitation", invitation_id.to_string()))?;

        if !invitation.is_actionable() {
            return Err(DomainError::InvitationInvalid);
        }

        // Verify the decliner is the appropriate party
        match invitation.invitation_type {
            LeagueTeamInvitationType::Invite => {
                // Invitee declines
                if invitation.user_id != declined_by {
                    return Err(DomainError::NotAuthorized(
                        "only the invited user can decline this invitation".to_string(),
                    ));
                }
            }
            LeagueTeamInvitationType::Request => {
                // Team captain declines
                let team = self
                    .team_repo
                    .find_by_id(invitation.team_id)
                    .await?
                    .ok_or_else(|| {
                        DomainError::not_found("league team", invitation.team_id.to_string())
                    })?;

                if team.captain_user_id != declined_by {
                    return Err(DomainError::NotAuthorized(
                        "only the team captain can decline join requests".to_string(),
                    ));
                }
            }
        }

        let updated = self
            .invitation_repo
            .update_status(invitation_id, LeagueTeamInvitationStatus::Declined, response_message)
            .await?;

        info!(
            invitation_id = %invitation_id,
            "League team invitation declined"
        );

        Ok(updated)
    }

    /// Cancel an invitation (by the inviter).
    #[instrument(skip(self))]
    pub async fn cancel_invitation(
        &self,
        invitation_id: LeagueTeamInvitationId,
        cancelled_by: UserId,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league team invitation", invitation_id.to_string()))?;

        if !invitation.is_pending() {
            return Err(DomainError::InvitationInvalid);
        }

        // Verify the canceller is the team captain
        let team = self
            .team_repo
            .find_by_id(invitation.team_id)
            .await?
            .ok_or_else(|| DomainError::not_found("league team", invitation.team_id.to_string()))?;

        if team.captain_user_id != cancelled_by {
            return Err(DomainError::NotAuthorized(
                "only the team captain can cancel invitations".to_string(),
            ));
        }

        let updated = self
            .invitation_repo
            .update_status(invitation_id, LeagueTeamInvitationStatus::Cancelled, None)
            .await?;

        info!(
            invitation_id = %invitation_id,
            "League team invitation cancelled"
        );

        Ok(updated)
    }

    /// Get pending invitations for a team.
    #[instrument(skip(self))]
    pub async fn get_team_invitations(
        &self,
        team_id: LeagueTeamId,
    ) -> Result<Vec<LeagueTeamInvitation>, DomainError> {
        self.invitation_repo.find_pending_by_team(team_id).await
    }

    /// Get pending invitations for a user.
    #[instrument(skip(self))]
    pub async fn get_user_invitations(
        &self,
        user_id: UserId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError> {
        self.invitation_repo.find_pending_for_user(user_id).await
    }

    /// Count pending invitations for a user.
    pub async fn count_user_invitations(&self, user_id: UserId) -> Result<i64, DomainError> {
        self.invitation_repo.count_pending_for_user(user_id).await
    }
}

impl<IR, TR, TMR, SR> Clone for LeagueTeamInvitationService<IR, TR, TMR, SR>
where
    IR: LeagueTeamInvitationRepository,
    TR: LeagueTeamRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    fn clone(&self) -> Self {
        Self {
            invitation_repo: Arc::clone(&self.invitation_repo),
            team_repo: Arc::clone(&self.team_repo),
            member_repo: Arc::clone(&self.member_repo),
            season_repo: Arc::clone(&self.season_repo),
        }
    }
}
