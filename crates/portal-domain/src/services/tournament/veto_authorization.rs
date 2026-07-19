//! Veto authorization service.
//!
//! Handles permission checks for veto (pick/ban) operations, including:
//! - Team captain authorization
//! - Team owner authorization
//! - Delegate authorization
//! - Tournament admin authorization

use std::sync::Arc;
use tracing::{debug, instrument};

use portal_core::{
    DomainError, LeagueTeamSeasonId, PlayerId, TournamentId, TournamentRegistrationId, UserId,
    VetoDelegateId,
};

use crate::entities::veto_delegate::{DelegatedByRole, VetoDelegate};
use crate::repositories::{
    CreateVetoDelegate, LeagueTeamMemberRepository, LeagueTeamRepository,
    LeagueTeamSeasonRepository, PermissionRepository, TournamentRegistrationRepository,
    VetoDelegateRepository,
};

// =============================================================================
// AUTHORIZATION RESULT
// =============================================================================

/// The role that authorized a veto action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VetoAuthorizationRole {
    /// User is a team captain.
    Captain,
    /// User is the team owner.
    Owner,
    /// User is a delegate.
    Delegate,
    /// User is a tournament admin.
    TournamentAdmin,
}

// =============================================================================
// SERVICE
// =============================================================================

/// Service for checking veto authorization.
pub struct VetoAuthorizationService<VDR, TRR, LTSR, LTR, LTMR, PR>
where
    VDR: VetoDelegateRepository,
    TRR: TournamentRegistrationRepository,
    LTSR: LeagueTeamSeasonRepository,
    LTR: LeagueTeamRepository,
    LTMR: LeagueTeamMemberRepository,
    PR: PermissionRepository,
{
    delegate_repo: Arc<VDR>,
    registration_repo: Arc<TRR>,
    team_season_repo: Arc<LTSR>,
    team_repo: Arc<LTR>,
    member_repo: Arc<LTMR>,
    permission_repo: Arc<PR>,
}

impl<VDR, TRR, LTSR, LTR, LTMR, PR> VetoAuthorizationService<VDR, TRR, LTSR, LTR, LTMR, PR>
where
    VDR: VetoDelegateRepository,
    TRR: TournamentRegistrationRepository,
    LTSR: LeagueTeamSeasonRepository,
    LTR: LeagueTeamRepository,
    LTMR: LeagueTeamMemberRepository,
    PR: PermissionRepository,
{
    /// Create a new veto authorization service.
    pub fn new(
        delegate_repo: Arc<VDR>,
        registration_repo: Arc<TRR>,
        team_season_repo: Arc<LTSR>,
        team_repo: Arc<LTR>,
        member_repo: Arc<LTMR>,
        permission_repo: Arc<PR>,
    ) -> Self {
        Self {
            delegate_repo,
            registration_repo,
            team_season_repo,
            team_repo,
            member_repo,
            permission_repo,
        }
    }

    /// Check if a user can perform veto actions for a registration.
    ///
    /// Returns the role that authorizes the action, or an error if not authorized.
    #[instrument(skip(self), fields(%registration_id, %user_id, %player_id))]
    pub async fn can_perform_veto_action(
        &self,
        registration_id: TournamentRegistrationId,
        user_id: UserId,
        player_id: PlayerId,
    ) -> Result<VetoAuthorizationRole, DomainError> {
        // Get the registration
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))?;

        // Get team_season_id (required for team registrations)
        let team_season_id = registration.team_season_id.ok_or_else(|| {
            DomainError::NotAuthorized(
                "Individual registrations do not support veto delegation".to_string(),
            )
        })?;

        // Check authorization in order of precedence
        // 1. Tournament admin (can always act)
        if self.is_tournament_admin(user_id).await? {
            debug!("User authorized as tournament admin");
            return Ok(VetoAuthorizationRole::TournamentAdmin);
        }

        // 2. Team captain
        if self.is_captain(team_season_id, player_id).await? {
            debug!("User authorized as team captain");
            return Ok(VetoAuthorizationRole::Captain);
        }

        // 3. Team owner
        if self.is_owner(team_season_id, player_id).await? {
            debug!("User authorized as team owner");
            return Ok(VetoAuthorizationRole::Owner);
        }

        // 4. Active delegate
        if self
            .is_delegate(team_season_id, player_id, Some(registration.tournament_id))
            .await?
        {
            debug!("User authorized as delegate");
            return Ok(VetoAuthorizationRole::Delegate);
        }

        Err(DomainError::NotAuthorized(
            "User is not authorized to perform veto actions for this team".to_string(),
        ))
    }

    /// Check if a user can create a delegation for a team.
    ///
    /// Returns the role that authorizes the delegation.
    #[instrument(skip(self), fields(%team_season_id, %user_id, %player_id))]
    pub async fn can_create_delegation(
        &self,
        team_season_id: LeagueTeamSeasonId,
        user_id: UserId,
        player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<DelegatedByRole, DomainError> {
        // Tournament admin can always delegate
        if self.is_tournament_admin(user_id).await? {
            return Ok(DelegatedByRole::TournamentAdmin);
        }

        // Owner can delegate
        if self.is_owner(team_season_id, player_id).await? {
            return Ok(DelegatedByRole::Owner);
        }

        // Captain can delegate
        if self.is_captain(team_season_id, player_id).await? {
            return Ok(DelegatedByRole::Captain);
        }

        Err(DomainError::NotAuthorized(
            "Only captains, owners, or tournament admins can create delegations".to_string(),
        ))
    }

    /// Check if a user can revoke a delegation.
    #[instrument(skip(self), fields(%delegate_id, %user_id, %player_id))]
    pub async fn can_revoke_delegation(
        &self,
        delegate_id: VetoDelegateId,
        user_id: UserId,
        player_id: PlayerId,
    ) -> Result<(), DomainError> {
        let delegate = self
            .delegate_repo
            .find_by_id(delegate_id)
            .await?
            .ok_or_else(|| DomainError::Internal(format!("Delegation {delegate_id} not found")))?;

        if !delegate.is_active() {
            return Err(DomainError::InvalidState(
                "Delegation is already revoked".to_string(),
            ));
        }

        let team_season_id = delegate.team_season_id;

        // Determine the revoking user's role
        let revoking_role = if self.is_tournament_admin(user_id).await? {
            DelegatedByRole::TournamentAdmin
        } else if self.is_owner(team_season_id, player_id).await? {
            DelegatedByRole::Owner
        } else if self.is_captain(team_season_id, player_id).await? {
            DelegatedByRole::Captain
        } else {
            return Err(DomainError::NotAuthorized(
                "Only captains, owners, or tournament admins can revoke delegations".to_string(),
            ));
        };

        // Check if the revoking role can revoke this delegation
        if !revoking_role.can_revoke(delegate.delegated_by_role) {
            return Err(DomainError::NotAuthorized(format!(
                "A {} cannot revoke a delegation made by a {}",
                revoking_role, delegate.delegated_by_role
            )));
        }

        Ok(())
    }

    /// Create a new delegation.
    #[instrument(skip(self), fields(%team_season_id, %delegate_player_id, %delegating_user_id))]
    pub async fn create_delegation(
        &self,
        team_season_id: LeagueTeamSeasonId,
        delegate_player_id: PlayerId,
        delegating_user_id: UserId,
        delegating_player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<VetoDelegate, DomainError> {
        // Check authorization
        let delegated_by_role = self
            .can_create_delegation(
                team_season_id,
                delegating_user_id,
                delegating_player_id,
                tournament_id,
            )
            .await?;

        // Verify the delegate is a member of the team
        let is_member = self
            .member_repo
            .is_member(team_season_id, delegate_player_id)
            .await?;

        if !is_member {
            return Err(DomainError::InvalidState(
                "Delegate must be a member of the team".to_string(),
            ));
        }

        // Create the delegation
        self.delegate_repo
            .create(CreateVetoDelegate {
                team_season_id,
                player_id: delegate_player_id,
                delegated_by_user_id: delegating_user_id,
                delegated_by_role,
                tournament_id,
            })
            .await
    }

    /// Revoke a delegation.
    #[instrument(skip(self), fields(%delegate_id, %revoking_user_id))]
    pub async fn revoke_delegation(
        &self,
        delegate_id: VetoDelegateId,
        revoking_user_id: UserId,
        revoking_player_id: PlayerId,
    ) -> Result<VetoDelegate, DomainError> {
        // Check authorization
        self.can_revoke_delegation(delegate_id, revoking_user_id, revoking_player_id)
            .await?;

        // Revoke
        self.delegate_repo
            .revoke(delegate_id, revoking_user_id)
            .await
    }

    /// List active delegations for a team.
    pub async fn list_delegations(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<VetoDelegate>, DomainError> {
        self.delegate_repo.list_active(team_season_id).await
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    async fn is_tournament_admin(&self, user_id: UserId) -> Result<bool, DomainError> {
        self.permission_repo
            .user_has_permission(user_id, "tournament.manage")
            .await
    }

    async fn is_captain(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        self.member_repo.is_captain(team_season_id, player_id).await
    }

    async fn is_owner(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError> {
        // Get the team-season to find the team_id
        let team_season = self
            .team_season_repo
            .find_by_id(team_season_id)
            .await?
            .ok_or_else(|| {
                DomainError::Internal(format!("Team season {team_season_id} not found"))
            })?;

        // Get the team to check ownership
        let team = self
            .team_repo
            .find_by_id(team_season.team_id)
            .await?
            .ok_or(DomainError::LeagueTeamNotFound(team_season.team_id))?;

        Ok(team.owner_player_id == player_id)
    }

    async fn is_delegate(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<bool, DomainError> {
        self.delegate_repo
            .is_delegate(team_season_id, player_id, tournament_id)
            .await
    }
}

impl<VDR, TRR, LTSR, LTR, LTMR, PR> Clone
    for VetoAuthorizationService<VDR, TRR, LTSR, LTR, LTMR, PR>
where
    VDR: VetoDelegateRepository,
    TRR: TournamentRegistrationRepository,
    LTSR: LeagueTeamSeasonRepository,
    LTR: LeagueTeamRepository,
    LTMR: LeagueTeamMemberRepository,
    PR: PermissionRepository,
{
    fn clone(&self) -> Self {
        Self {
            delegate_repo: Arc::clone(&self.delegate_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            team_season_repo: Arc::clone(&self.team_season_repo),
            team_repo: Arc::clone(&self.team_repo),
            member_repo: Arc::clone(&self.member_repo),
            permission_repo: Arc::clone(&self.permission_repo),
        }
    }
}
