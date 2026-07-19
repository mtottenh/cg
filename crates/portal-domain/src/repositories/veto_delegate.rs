//! Repository trait for veto delegate persistence.

use async_trait::async_trait;
use portal_core::{
    DomainError, LeagueTeamSeasonId, PlayerId, TournamentId, UserId, VetoDelegateId,
};

use crate::entities::veto_delegate::{DelegatedByRole, VetoDelegate};

/// Command to create a new veto delegate.
#[derive(Debug, Clone)]
pub struct CreateVetoDelegate {
    /// The team-season this delegation applies to.
    pub team_season_id: LeagueTeamSeasonId,
    /// The player being delegated authority.
    pub player_id: PlayerId,
    /// User who created this delegation.
    pub delegated_by_user_id: UserId,
    /// Role that authorized the delegation.
    pub delegated_by_role: DelegatedByRole,
    /// Optional scope to specific tournament (None = all tournaments).
    pub tournament_id: Option<TournamentId>,
}

/// Repository trait for veto delegate persistence.
#[async_trait]
pub trait VetoDelegateRepository: Send + Sync {
    /// Create a new delegation.
    async fn create(&self, cmd: CreateVetoDelegate) -> Result<VetoDelegate, DomainError>;

    /// Find a delegation by ID.
    async fn find_by_id(&self, id: VetoDelegateId) -> Result<Option<VetoDelegate>, DomainError>;

    /// Check if a player is an active delegate for a team.
    ///
    /// Considers both tournament-specific and global delegations.
    async fn is_delegate(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<bool, DomainError>;

    /// List all active delegations for a team-season.
    async fn list_active(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<VetoDelegate>, DomainError>;

    /// List all active delegations for a player.
    async fn list_by_player(&self, player_id: PlayerId) -> Result<Vec<VetoDelegate>, DomainError>;

    /// Revoke a delegation.
    async fn revoke(
        &self,
        id: VetoDelegateId,
        revoked_by_user_id: UserId,
    ) -> Result<VetoDelegate, DomainError>;
}
