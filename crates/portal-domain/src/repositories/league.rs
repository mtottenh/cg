//! League repository traits.

use crate::entities::league::{
    League, LeagueInvitation, LeagueInvitationStatus, LeagueMember, LeagueMemberWithUser,
    LeagueMembershipType, UserLeagueMembership,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{DomainError, GameId, LeagueId, LeagueInvitationId, UserId};

/// Repository trait for league operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueRepository: Send + Sync {
    /// Find a league by ID.
    async fn find_by_id(&self, id: LeagueId) -> Result<Option<League>, DomainError>;

    /// Find a league by slug.
    async fn find_by_slug(&self, slug: &str) -> Result<Option<League>, DomainError>;

    /// Create a new league.
    async fn create(&self, league: CreateLeague) -> Result<League, DomainError>;

    /// Update a league.
    async fn update(&self, id: LeagueId, update: UpdateLeague) -> Result<League, DomainError>;

    /// List leagues for a game.
    async fn list_by_game(
        &self,
        game_id: &GameId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<League>, DomainError>;

    /// Count leagues for a game.
    async fn count_by_game(&self, game_id: &GameId) -> Result<i64, DomainError>;

    /// Check if a slug already exists.
    async fn slug_exists(&self, slug: &str) -> Result<bool, DomainError>;

    /// Search leagues by name.
    async fn search(
        &self,
        query: &str,
        game_id: Option<GameId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<League>, DomainError>;

    /// Count search results.
    async fn count_search(&self, query: &str, game_id: Option<GameId>) -> Result<i64, DomainError>;
}

/// Data for creating a new league.
#[derive(Debug, Clone)]
pub struct CreateLeague {
    pub game_id: GameId,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub access_type: String,
    pub created_by: UserId,
}

/// Data for updating a league.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeague {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub access_type: Option<String>,
    pub status: Option<String>,
    pub settings: Option<serde_json::Value>,
}

/// Repository trait for league member operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueMemberRepository: Send + Sync {
    /// Find a member by league and user.
    async fn find_member(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueMember>, DomainError>;

    /// List all members of a league with user info.
    async fn list_members(
        &self,
        league_id: LeagueId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<LeagueMemberWithUser>, DomainError>;

    /// Count members in a league.
    async fn count_members(&self, league_id: LeagueId) -> Result<i64, DomainError>;

    /// Add a member to a league.
    async fn add_member(&self, member: AddLeagueMember) -> Result<LeagueMember, DomainError>;

    /// Remove a member from a league.
    async fn remove_member(&self, league_id: LeagueId, user_id: UserId) -> Result<(), DomainError>;

    /// Update a member's role.
    async fn update_membership_type(
        &self,
        league_id: LeagueId,
        user_id: UserId,
        membership_type: LeagueMembershipType,
    ) -> Result<LeagueMember, DomainError>;

    /// Check if user is a member of a league.
    async fn is_member(&self, league_id: LeagueId, user_id: UserId) -> Result<bool, DomainError>;

    /// Check if user is an admin of a league.
    async fn is_admin(&self, league_id: LeagueId, user_id: UserId) -> Result<bool, DomainError>;

    /// Check if user is admin or moderator of a league.
    async fn is_admin_or_moderator(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, DomainError>;

    /// List all league memberships for a user.
    async fn list_memberships_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UserLeagueMembership>, DomainError>;

    /// Count admins in a league.
    async fn count_admins(&self, league_id: LeagueId) -> Result<i64, DomainError>;
}

/// Data for adding a league member.
#[derive(Debug, Clone)]
pub struct AddLeagueMember {
    pub league_id: LeagueId,
    pub user_id: UserId,
    pub membership_type: LeagueMembershipType,
}

/// Repository trait for league invitation/application operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueInvitationRepository: Send + Sync {
    /// Find an invitation by ID.
    async fn find_by_id(
        &self,
        id: LeagueInvitationId,
    ) -> Result<Option<LeagueInvitation>, DomainError>;

    /// Create a new invitation/application.
    async fn create(&self, invitation: CreateLeagueInvitation) -> Result<LeagueInvitation, DomainError>;

    /// Update invitation status (accept/reject).
    async fn update_status(
        &self,
        id: LeagueInvitationId,
        status: LeagueInvitationStatus,
        responded_by: UserId,
    ) -> Result<LeagueInvitation, DomainError>;

    /// Find pending invitation for a league and user.
    async fn find_pending(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<Option<LeagueInvitation>, DomainError>;

    /// List pending invitations for a league.
    async fn list_pending_by_league(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueInvitation>, DomainError>;

    /// List pending invitations/applications for a user.
    async fn list_pending_for_user(&self, user_id: UserId) -> Result<Vec<LeagueInvitation>, DomainError>;

    /// Cancel all pending invitations for a user in a league.
    async fn cancel_pending(&self, league_id: LeagueId, user_id: UserId) -> Result<(), DomainError>;

    /// Count pending applications for a league.
    async fn count_pending_applications(&self, league_id: LeagueId) -> Result<i64, DomainError>;
}

/// Data for creating a league invitation.
#[derive(Debug, Clone)]
pub struct CreateLeagueInvitation {
    pub league_id: LeagueId,
    pub user_id: UserId,
    pub invitation_type: String,
    pub message: Option<String>,
    pub invited_by: Option<UserId>,
    pub expires_at: Option<DateTime<Utc>>,
}
