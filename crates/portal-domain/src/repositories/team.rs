//! Team repository traits.

use crate::entities::team::{PlayerTeamMembership, Team, TeamInvitation, TeamMember};
use async_trait::async_trait;
use portal_core::types::{InvitationStatus, TeamRole};
use portal_core::{DomainError, PlayerId, TeamId, TeamInvitationId};

/// Repository trait for team operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TeamRepository: Send + Sync {
    /// Find a team by ID.
    async fn find_by_id(&self, id: TeamId) -> Result<Option<Team>, DomainError>;

    /// Find a team by name.
    async fn find_by_name(&self, name: &str) -> Result<Option<Team>, DomainError>;

    /// Find a team by tag.
    async fn find_by_tag(&self, tag: &str) -> Result<Option<Team>, DomainError>;

    /// Create a new team.
    async fn create(&self, team: CreateTeam) -> Result<Team, DomainError>;

    /// Update a team.
    async fn update(&self, id: TeamId, update: UpdateTeam) -> Result<Team, DomainError>;

    /// List teams for a player.
    async fn list_by_player(&self, player_id: PlayerId) -> Result<Vec<Team>, DomainError>;

    /// List teams with optional search and pagination.
    ///
    /// Returns teams matching the optional search query (searches name and tag)
    /// along with the total count for pagination.
    async fn list(
        &self,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Team>, i64), DomainError>;

    /// Check if a name is taken.
    async fn name_exists(&self, name: &str) -> Result<bool, DomainError>;

    /// Check if a tag is taken.
    async fn tag_exists(&self, tag: &str) -> Result<bool, DomainError>;
}

/// Data for creating a new team.
#[derive(Debug, Clone)]
pub struct CreateTeam {
    pub name: String,
    pub tag: String,
    pub created_by: PlayerId,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub game_id: Option<String>,
}

/// Data for updating a team.
#[derive(Debug, Clone, Default)]
pub struct UpdateTeam {
    pub name: Option<String>,
    pub tag: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub website_url: Option<String>,
    pub status: Option<String>,
}

/// Repository trait for team member operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TeamMemberRepository: Send + Sync {
    /// Find a team member by team and player.
    async fn find_member(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<Option<TeamMember>, DomainError>;

    /// List all active members of a team.
    async fn list_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, DomainError>;

    /// Count captains in a team.
    async fn count_captains(&self, team_id: TeamId) -> Result<i64, DomainError>;

    /// Count active members in a team.
    async fn count_members(&self, team_id: TeamId) -> Result<i64, DomainError>;

    /// Add a member to a team.
    async fn add_member(&self, member: AddMember) -> Result<TeamMember, DomainError>;

    /// Update a member's role.
    async fn update_role(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
        new_role: TeamRole,
    ) -> Result<TeamMember, DomainError>;

    /// Remove a member from a team.
    async fn remove_member(&self, team_id: TeamId, player_id: PlayerId) -> Result<(), DomainError>;

    /// Check if a player is a member of a team.
    async fn is_member(&self, team_id: TeamId, player_id: PlayerId) -> Result<bool, DomainError>;

    /// Check if a player is a captain of a team.
    async fn is_captain(&self, team_id: TeamId, player_id: PlayerId) -> Result<bool, DomainError>;

    /// List all team memberships for a player (with team details).
    async fn list_memberships_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerTeamMembership>, DomainError>;
}

/// Data for adding a team member.
#[derive(Debug, Clone)]
pub struct AddMember {
    pub team_id: TeamId,
    pub player_id: PlayerId,
    pub role: TeamRole,
    pub is_founder: bool,
    pub invited_by: Option<PlayerId>,
}

/// Repository trait for team invitation operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TeamInvitationRepository: Send + Sync {
    /// Create a new invitation.
    async fn create(&self, invitation: CreateInvitation) -> Result<TeamInvitation, DomainError>;

    /// Find an invitation by ID.
    async fn find_by_id(&self, id: TeamInvitationId) -> Result<Option<TeamInvitation>, DomainError>;

    /// Find all pending invitations for a team.
    async fn find_pending_by_team(&self, team_id: TeamId) -> Result<Vec<TeamInvitation>, DomainError>;

    /// Find all pending invitations for a player.
    async fn find_pending_for_player(&self, player_id: PlayerId) -> Result<Vec<TeamInvitation>, DomainError>;

    /// Check if there's an existing pending invitation for this player/team.
    async fn find_existing_pending(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<Option<TeamInvitation>, DomainError>;

    /// Update invitation status.
    async fn update_status(
        &self,
        id: TeamInvitationId,
        status: InvitationStatus,
        response_message: Option<String>,
    ) -> Result<TeamInvitation, DomainError>;

    /// Cancel all pending invitations for a player on a specific team.
    async fn cancel_pending_for_player(
        &self,
        team_id: TeamId,
        player_id: PlayerId,
    ) -> Result<(), DomainError>;

    /// Count pending invitations for a player.
    async fn count_pending_for_player(&self, player_id: PlayerId) -> Result<i64, DomainError>;
}

/// Data for creating a new invitation.
#[derive(Debug, Clone)]
pub struct CreateInvitation {
    pub team_id: TeamId,
    pub player_id: PlayerId,
    pub invitation_type: String,
    pub role: TeamRole,
    pub message: Option<String>,
    pub invited_by: Option<PlayerId>,
}
