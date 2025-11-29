//! League team repository traits.
//!
//! These repositories handle league-scoped teams, which are distinct from global teams.

use crate::entities::league_team::{
    LeagueSeason, LeagueTeam, LeagueTeamInvitation, LeagueTeamInvitationWithTeam, LeagueTeamMember,
    LeagueTeamMemberWithUser, LeagueTeamSummary, UserLeagueTeamMembership,
};
use async_trait::async_trait;
use portal_core::types::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
use portal_core::{
    DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamInvitationId,
    LeagueTeamMemberId, UserId,
};

// =============================================================================
// LEAGUE SEASON REPOSITORY
// =============================================================================

/// Repository trait for league season operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueSeasonRepository: Send + Sync {
    /// Find a season by ID.
    async fn find_by_id(&self, id: LeagueSeasonId) -> Result<Option<LeagueSeason>, DomainError>;

    /// Find a season by league ID and slug.
    async fn find_by_slug(
        &self,
        league_id: LeagueId,
        slug: &str,
    ) -> Result<Option<LeagueSeason>, DomainError>;

    /// Create a new season.
    async fn create(&self, season: CreateLeagueSeason) -> Result<LeagueSeason, DomainError>;

    /// Update a season.
    async fn update(
        &self,
        id: LeagueSeasonId,
        update: UpdateLeagueSeason,
    ) -> Result<LeagueSeason, DomainError>;

    /// List all seasons for a league.
    async fn list_by_league(&self, league_id: LeagueId) -> Result<Vec<LeagueSeason>, DomainError>;

    /// List active seasons for a league (registration, active, playoffs).
    async fn list_active_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueSeason>, DomainError>;

    /// Get the current/active season for a league.
    async fn find_current_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Option<LeagueSeason>, DomainError>;

    /// Check if a slug is taken within a league.
    async fn slug_exists(&self, league_id: LeagueId, slug: &str) -> Result<bool, DomainError>;

    /// Update the roster lock status.
    async fn update_roster_lock(
        &self,
        id: LeagueSeasonId,
        status: RosterLockStatus,
        locked_by: Option<UserId>,
    ) -> Result<LeagueSeason, DomainError>;

    /// Update the season status.
    async fn update_status(
        &self,
        id: LeagueSeasonId,
        status: SeasonStatus,
    ) -> Result<LeagueSeason, DomainError>;

    /// Count teams in a season.
    async fn count_teams(&self, season_id: LeagueSeasonId) -> Result<i64, DomainError>;
}

/// Data for creating a new league season.
#[derive(Debug, Clone)]
pub struct CreateLeagueSeason {
    pub league_id: LeagueId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub registration_start: Option<chrono::DateTime<chrono::Utc>>,
    pub registration_end: Option<chrono::DateTime<chrono::Utc>>,
    pub season_start: Option<chrono::DateTime<chrono::Utc>>,
    pub season_end: Option<chrono::DateTime<chrono::Utc>>,
    pub team_size_min: i32,
    pub team_size_max: i32,
    pub max_substitutes: i32,
    pub max_teams: Option<i32>,
    pub created_by: UserId,
}

/// Data for updating a league season.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueSeason {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub registration_start: Option<chrono::DateTime<chrono::Utc>>,
    pub registration_end: Option<chrono::DateTime<chrono::Utc>>,
    pub season_start: Option<chrono::DateTime<chrono::Utc>>,
    pub season_end: Option<chrono::DateTime<chrono::Utc>>,
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub max_substitutes: Option<i32>,
    pub max_teams: Option<i32>,
    pub status: Option<SeasonStatus>,
    pub settings: Option<serde_json::Value>,
}

// =============================================================================
// LEAGUE TEAM REPOSITORY
// =============================================================================

/// Repository trait for league team operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueTeamRepository: Send + Sync {
    /// Find a team by ID.
    async fn find_by_id(&self, id: LeagueTeamId) -> Result<Option<LeagueTeam>, DomainError>;

    /// Find a team by season and name.
    async fn find_by_name(
        &self,
        season_id: LeagueSeasonId,
        name: &str,
    ) -> Result<Option<LeagueTeam>, DomainError>;

    /// Find a team by season and tag.
    async fn find_by_tag(
        &self,
        season_id: LeagueSeasonId,
        tag: &str,
    ) -> Result<Option<LeagueTeam>, DomainError>;

    /// Create a new team.
    async fn create(&self, team: CreateLeagueTeam) -> Result<LeagueTeam, DomainError>;

    /// Update a team.
    async fn update(
        &self,
        id: LeagueTeamId,
        update: UpdateLeagueTeam,
    ) -> Result<LeagueTeam, DomainError>;

    /// List teams for a season with optional filters and pagination.
    async fn list_by_season(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<LeagueTeamStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeam>, i64), DomainError>;

    /// List all teams a user is captain of in a season.
    async fn list_by_captain(
        &self,
        season_id: LeagueSeasonId,
        captain_user_id: UserId,
    ) -> Result<Vec<LeagueTeam>, DomainError>;

    /// Update team status.
    async fn update_status(
        &self,
        id: LeagueTeamId,
        status: LeagueTeamStatus,
    ) -> Result<LeagueTeam, DomainError>;

    /// Update team captain.
    async fn update_captain(
        &self,
        id: LeagueTeamId,
        captain_user_id: UserId,
    ) -> Result<LeagueTeam, DomainError>;

    /// Check if a name is taken in a season.
    async fn name_exists(&self, season_id: LeagueSeasonId, name: &str) -> Result<bool, DomainError>;

    /// Check if a tag is taken in a season.
    async fn tag_exists(&self, season_id: LeagueSeasonId, tag: &str) -> Result<bool, DomainError>;

    /// Get team summaries with member counts for a season.
    async fn list_summaries(
        &self,
        season_id: LeagueSeasonId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeamSummary>, i64), DomainError>;
}

/// Data for creating a new league team.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeam {
    pub season_id: LeagueSeasonId,
    pub name: String,
    pub tag: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub captain_user_id: UserId,
}

/// Data for updating a league team.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueTeam {
    pub name: Option<String>,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
}

// =============================================================================
// LEAGUE TEAM MEMBER REPOSITORY
// =============================================================================

/// Repository trait for league team member operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueTeamMemberRepository: Send + Sync {
    /// Find a member by ID.
    async fn find_by_id(
        &self,
        id: LeagueTeamMemberId,
    ) -> Result<Option<LeagueTeamMember>, DomainError>;

    /// Find a member by team and user.
    async fn find_member(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
    ) -> Result<Option<LeagueTeamMember>, DomainError>;

    /// Add a member to a team.
    async fn add_member(&self, member: AddLeagueTeamMember) -> Result<LeagueTeamMember, DomainError>;

    /// Update a member's role.
    async fn update_role(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
        new_role: LeagueTeamRole,
    ) -> Result<LeagueTeamMember, DomainError>;

    /// Update a member's status.
    async fn update_status(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
        status: LeagueTeamMemberStatus,
    ) -> Result<LeagueTeamMember, DomainError>;

    /// Remove a member from a team (set left_at timestamp).
    async fn remove_member(&self, team_id: LeagueTeamId, user_id: UserId) -> Result<(), DomainError>;

    /// List all active members of a team.
    async fn list_members(
        &self,
        team_id: LeagueTeamId,
    ) -> Result<Vec<LeagueTeamMember>, DomainError>;

    /// List all active members of a team with user details.
    async fn list_members_with_users(
        &self,
        team_id: LeagueTeamId,
    ) -> Result<Vec<LeagueTeamMemberWithUser>, DomainError>;

    /// Count members by role in a team.
    async fn count_by_role(
        &self,
        team_id: LeagueTeamId,
        role: LeagueTeamRole,
    ) -> Result<i64, DomainError>;

    /// Count all active members in a team.
    async fn count_active_members(&self, team_id: LeagueTeamId) -> Result<i64, DomainError>;

    /// Count primary members (captain + players) in a team.
    async fn count_primary_members(&self, team_id: LeagueTeamId) -> Result<i64, DomainError>;

    /// Count substitutes in a team.
    async fn count_substitutes(&self, team_id: LeagueTeamId) -> Result<i64, DomainError>;

    /// Check if a user is a member of a team.
    async fn is_member(&self, team_id: LeagueTeamId, user_id: UserId) -> Result<bool, DomainError>;

    /// Check if a user is a captain of a team.
    async fn is_captain(&self, team_id: LeagueTeamId, user_id: UserId) -> Result<bool, DomainError>;

    /// Check if a user is already a primary member of another team in the same season.
    /// Returns the team ID if they are, None otherwise.
    async fn find_primary_team_in_season(
        &self,
        season_id: LeagueSeasonId,
        user_id: UserId,
    ) -> Result<Option<LeagueTeamId>, DomainError>;

    /// List all team memberships for a user across all seasons.
    async fn list_memberships_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UserLeagueTeamMembership>, DomainError>;

    /// List all team memberships for a user in a specific season.
    async fn list_memberships_in_season(
        &self,
        user_id: UserId,
        season_id: LeagueSeasonId,
    ) -> Result<Vec<UserLeagueTeamMembership>, DomainError>;
}

/// Data for adding a league team member.
#[derive(Debug, Clone)]
pub struct AddLeagueTeamMember {
    pub team_id: LeagueTeamId,
    pub user_id: UserId,
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
    pub added_by: Option<UserId>,
}

// =============================================================================
// LEAGUE TEAM INVITATION REPOSITORY
// =============================================================================

/// Repository trait for league team invitation operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueTeamInvitationRepository: Send + Sync {
    /// Create a new invitation.
    async fn create(
        &self,
        invitation: CreateLeagueTeamInvitation,
    ) -> Result<LeagueTeamInvitation, DomainError>;

    /// Find an invitation by ID.
    async fn find_by_id(
        &self,
        id: LeagueTeamInvitationId,
    ) -> Result<Option<LeagueTeamInvitation>, DomainError>;

    /// Find an invitation by ID with team and season details.
    async fn find_by_id_with_team(
        &self,
        id: LeagueTeamInvitationId,
    ) -> Result<Option<LeagueTeamInvitationWithTeam>, DomainError>;

    /// Find all pending invitations for a team.
    async fn find_pending_by_team(
        &self,
        team_id: LeagueTeamId,
    ) -> Result<Vec<LeagueTeamInvitation>, DomainError>;

    /// Find all pending invitations/requests for a user.
    async fn find_pending_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError>;

    /// Find all pending invitations for a user in a specific season.
    async fn find_pending_for_user_in_season(
        &self,
        user_id: UserId,
        season_id: LeagueSeasonId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError>;

    /// Check if there's an existing pending invitation for this user/team.
    async fn find_existing_pending(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
    ) -> Result<Option<LeagueTeamInvitation>, DomainError>;

    /// Update invitation status.
    async fn update_status(
        &self,
        id: LeagueTeamInvitationId,
        status: LeagueTeamInvitationStatus,
        response_message: Option<String>,
    ) -> Result<LeagueTeamInvitation, DomainError>;

    /// Cancel all pending invitations for a user on a specific team.
    async fn cancel_pending_for_user(
        &self,
        team_id: LeagueTeamId,
        user_id: UserId,
    ) -> Result<(), DomainError>;

    /// Count pending invitations for a user.
    async fn count_pending_for_user(&self, user_id: UserId) -> Result<i64, DomainError>;

    /// Expire all invitations past their expiration date.
    async fn expire_old_invitations(&self) -> Result<i64, DomainError>;
}

/// Data for creating a new league team invitation.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeamInvitation {
    pub team_id: LeagueTeamId,
    pub user_id: UserId,
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,
    pub invited_by: Option<UserId>,
}
