//! League team invitation service.

use crate::entities::league_team::{
    LeagueTeamInvitation, LeagueTeamInvitationWithTeam, LeagueTeamMember,
};
use crate::repositories::league_team::{
    AddLeagueTeamMember, CreateLeagueTeamInvitation, LeagueSeasonRepository,
    LeagueTeamInvitationRepository, LeagueTeamMemberRepository, LeagueTeamRepository,
    LeagueTeamSeasonRepository,
};
use portal_core::types::{LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamRole};
use portal_core::{DomainError, LeagueTeamInvitationId, LeagueTeamSeasonId, PlayerId, UserId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for league team invitation business logic.
///
/// Invitations now target team seasons (not teams directly).
pub struct LeagueTeamInvitationService<IR, TR, TSR, TMR, SR>
where
    IR: LeagueTeamInvitationRepository,
    TR: LeagueTeamRepository,
    TSR: LeagueTeamSeasonRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    invitation_repo: Arc<IR>,
    #[allow(dead_code)]
    team_repo: Arc<TR>,
    team_season_repo: Arc<TSR>,
    member_repo: Arc<TMR>,
    season_repo: Arc<SR>,
}

impl<IR, TR, TSR, TMR, SR> LeagueTeamInvitationService<IR, TR, TSR, TMR, SR>
where
    IR: LeagueTeamInvitationRepository,
    TR: LeagueTeamRepository,
    TSR: LeagueTeamSeasonRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    /// Create a new invitation service.
    pub const fn new(
        invitation_repo: Arc<IR>,
        team_repo: Arc<TR>,
        team_season_repo: Arc<TSR>,
        member_repo: Arc<TMR>,
        season_repo: Arc<SR>,
    ) -> Self {
        Self {
            invitation_repo,
            team_repo,
            team_season_repo,
            member_repo,
            season_repo,
        }
    }

    /// Create an invitation (captain invites player to seasonal roster).
    #[instrument(skip(self))]
    pub async fn create_invitation(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        role: LeagueTeamRole,
        message: Option<String>,
        invited_by: UserId,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let team_season = self
            .team_season_repo
            .find_by_id(team_season_id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "league team season",
                query: team_season_id.to_string(),
            })?;

        let season = self
            .season_repo
            .find_by_id(team_season.season_id)
            .await?
            .ok_or(DomainError::LeagueSeasonNotFound(team_season.season_id))?;

        // Check roster lock status
        if role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for primary member invitations".to_string(),
            ));
        }

        // Check if player is already a member
        if self
            .member_repo
            .is_member(team_season_id, player_id)
            .await?
        {
            return Err(DomainError::AlreadyTeamMember);
        }

        // For primary roles, check one-team-per-season constraint
        if role.is_primary()
            && let Some(existing_team_season_id) = self
                .member_repo
                .find_primary_team_in_season(team_season.season_id, player_id)
                .await?
        {
            return Err(DomainError::Conflict(format!(
                "player is already a primary member of team {existing_team_season_id} in this season"
            )));
        }

        // Check for existing pending invitation
        if self
            .invitation_repo
            .find_existing_pending(team_season_id, player_id)
            .await?
            .is_some()
        {
            return Err(DomainError::InvitationAlreadyExists);
        }

        let invitation = self
            .invitation_repo
            .create(CreateLeagueTeamInvitation {
                team_season_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role,
                message,
                invited_by: Some(invited_by),
            })
            .await?;

        info!(
            invitation_id = %invitation.id,
            team_season_id = %team_season_id,
            player_id = %player_id,
            "League team invitation created"
        );

        Ok(invitation)
    }

    /// Create a join request (player requests to join seasonal roster).
    #[instrument(skip(self))]
    pub async fn create_join_request(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        role: LeagueTeamRole,
        message: Option<String>,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let team_season = self
            .team_season_repo
            .find_by_id(team_season_id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "league team season",
                query: team_season_id.to_string(),
            })?;

        let season = self
            .season_repo
            .find_by_id(team_season.season_id)
            .await?
            .ok_or(DomainError::LeagueSeasonNotFound(team_season.season_id))?;

        if !season.is_registration_open() {
            return Err(DomainError::RegistrationClosed);
        }

        // Check if player is already a member
        if self
            .member_repo
            .is_member(team_season_id, player_id)
            .await?
        {
            return Err(DomainError::AlreadyTeamMember);
        }

        // For primary roles, check one-team-per-season constraint
        if role.is_primary()
            && let Some(existing_team_season_id) = self
                .member_repo
                .find_primary_team_in_season(team_season.season_id, player_id)
                .await?
        {
            return Err(DomainError::Conflict(format!(
                "player is already a primary member of team {existing_team_season_id} in this season"
            )));
        }

        // Check for existing pending request
        if self
            .invitation_repo
            .find_existing_pending(team_season_id, player_id)
            .await?
            .is_some()
        {
            return Err(DomainError::InvitationAlreadyExists);
        }

        let invitation = self
            .invitation_repo
            .create(CreateLeagueTeamInvitation {
                team_season_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Request,
                role,
                message,
                invited_by: None,
            })
            .await?;

        info!(
            invitation_id = %invitation.id,
            team_season_id = %team_season_id,
            player_id = %player_id,
            "League team join request created"
        );

        Ok(invitation)
    }

    /// Accept an invitation/request.
    ///
    /// For invites: the invited player accepts
    /// For requests: a team captain accepts
    #[instrument(skip(self))]
    pub async fn accept_invitation(
        &self,
        invitation_id: LeagueTeamInvitationId,
        accepted_by_player_id: PlayerId,
    ) -> Result<LeagueTeamMember, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::LeagueTeamInvitationNotFound(invitation_id))?;

        if !invitation.is_actionable() {
            if invitation.is_expired() {
                return Err(DomainError::InvitationExpired);
            }
            return Err(DomainError::InvitationInvalid);
        }

        let team_season = self
            .team_season_repo
            .find_by_id(invitation.team_season_id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "league team season",
                query: invitation.team_season_id.to_string(),
            })?;

        // Verify the acceptor is the appropriate party
        match invitation.invitation_type {
            LeagueTeamInvitationType::Invite => {
                // Invitee (player) accepts
                if invitation.player_id != accepted_by_player_id {
                    return Err(DomainError::NotAuthorized(
                        "only the invited player can accept this invitation".to_string(),
                    ));
                }
            }
            LeagueTeamInvitationType::Request => {
                // A team captain accepts
                if !self
                    .member_repo
                    .is_captain(invitation.team_season_id, accepted_by_player_id)
                    .await?
                {
                    return Err(DomainError::NotAuthorized(
                        "only a team captain can accept join requests".to_string(),
                    ));
                }
            }
        }

        let season = self
            .season_repo
            .find_by_id(team_season.season_id)
            .await?
            .ok_or(DomainError::LeagueSeasonNotFound(team_season.season_id))?;

        // Re-verify roster lock status
        if invitation.role.is_primary() && !season.allows_primary_roster_changes() {
            return Err(DomainError::InvalidState(
                "roster is locked for primary member changes".to_string(),
            ));
        }

        // Re-verify one-team-per-season constraint for primary roles
        if invitation.role.is_primary()
            && let Some(existing_team_season_id) = self
                .member_repo
                .find_primary_team_in_season(team_season.season_id, invitation.player_id)
                .await?
        {
            return Err(DomainError::Conflict(format!(
                "player is already a primary member of team {existing_team_season_id} in this season"
            )));
        }

        // Check roster size limits
        if invitation.role.is_primary() {
            let primary_count = self
                .member_repo
                .count_primary_members(invitation.team_season_id)
                .await?;
            if let Some(max) = season.team_size_max
                && primary_count >= i64::from(max)
            {
                return Err(DomainError::TeamFull);
            }
        } else {
            let sub_count = self
                .member_repo
                .count_substitutes(invitation.team_season_id)
                .await?;
            if let Some(max_subs) = season.max_substitutes
                && sub_count >= i64::from(max_subs)
            {
                return Err(DomainError::Conflict(
                    "maximum number of substitutes reached".to_string(),
                ));
            }
        }

        // Atomic: flipping the invitation to Accepted and seating the
        // player on the roster commit together or not at all. The prior
        // two-call version could leave an Accepted invitation with no
        // matching roster row on partial failure — the player saw their
        // invite accepted but was silently missing from the team, and
        // retrying returned "invitation already used". See audit I5.
        let member = self
            .invitation_repo
            .accept_and_add_member(
                invitation_id,
                AddLeagueTeamMember {
                    team_season_id: invitation.team_season_id,
                    player_id: invitation.player_id,
                    role: invitation.role,
                    position: None,
                    jersey_number: None,
                    added_by: invitation.invited_by,
                },
            )
            .await?;

        info!(
            invitation_id = %invitation_id,
            team_season_id = %invitation.team_season_id,
            player_id = %invitation.player_id,
            "League team invitation accepted"
        );

        Ok(member)
    }

    /// Decline an invitation/request.
    ///
    /// For invites: the invited player declines
    /// For requests: a team captain declines
    #[instrument(skip(self))]
    pub async fn decline_invitation(
        &self,
        invitation_id: LeagueTeamInvitationId,
        declined_by_player_id: PlayerId,
        response_message: Option<String>,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::LeagueTeamInvitationNotFound(invitation_id))?;

        if !invitation.is_actionable() {
            return Err(DomainError::InvitationInvalid);
        }

        // Verify the decliner is the appropriate party
        match invitation.invitation_type {
            LeagueTeamInvitationType::Invite => {
                // Invitee declines
                if invitation.player_id != declined_by_player_id {
                    return Err(DomainError::NotAuthorized(
                        "only the invited player can decline this invitation".to_string(),
                    ));
                }
            }
            LeagueTeamInvitationType::Request => {
                // A team captain declines
                if !self
                    .member_repo
                    .is_captain(invitation.team_season_id, declined_by_player_id)
                    .await?
                {
                    return Err(DomainError::NotAuthorized(
                        "only a team captain can decline join requests".to_string(),
                    ));
                }
            }
        }

        let updated = self
            .invitation_repo
            .update_status(
                invitation_id,
                LeagueTeamInvitationStatus::Declined,
                response_message,
            )
            .await?;

        info!(
            invitation_id = %invitation_id,
            "League team invitation declined"
        );

        Ok(updated)
    }

    /// Cancel an invitation (by a team captain).
    #[instrument(skip(self))]
    pub async fn cancel_invitation(
        &self,
        invitation_id: LeagueTeamInvitationId,
        cancelled_by_player_id: PlayerId,
    ) -> Result<LeagueTeamInvitation, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::LeagueTeamInvitationNotFound(invitation_id))?;

        if !invitation.is_pending() {
            return Err(DomainError::InvitationInvalid);
        }

        // Verify the canceller is a team captain
        if !self
            .member_repo
            .is_captain(invitation.team_season_id, cancelled_by_player_id)
            .await?
        {
            return Err(DomainError::NotAuthorized(
                "only a team captain can cancel invitations".to_string(),
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

    /// Get pending invitations for a team season.
    #[instrument(skip(self))]
    pub async fn get_team_invitations(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamInvitation>, DomainError> {
        self.invitation_repo
            .find_pending_by_team_season(team_season_id)
            .await
    }

    /// Get pending invitations for a player.
    #[instrument(skip(self))]
    pub async fn get_player_invitations(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError> {
        self.invitation_repo
            .find_pending_for_player(player_id)
            .await
    }

    /// Count pending invitations for a player.
    pub async fn count_player_invitations(&self, player_id: PlayerId) -> Result<i64, DomainError> {
        self.invitation_repo
            .count_pending_for_player(player_id)
            .await
    }

    /// Get an invitation by ID with team details.
    pub async fn get_invitation_with_team(
        &self,
        invitation_id: LeagueTeamInvitationId,
    ) -> Result<LeagueTeamInvitationWithTeam, DomainError> {
        self.invitation_repo
            .find_by_id_with_team(invitation_id)
            .await?
            .ok_or(DomainError::LeagueTeamInvitationNotFound(invitation_id))
    }
}

impl<IR, TR, TSR, TMR, SR> Clone for LeagueTeamInvitationService<IR, TR, TSR, TMR, SR>
where
    IR: LeagueTeamInvitationRepository,
    TR: LeagueTeamRepository,
    TSR: LeagueTeamSeasonRepository,
    TMR: LeagueTeamMemberRepository,
    SR: LeagueSeasonRepository,
{
    fn clone(&self) -> Self {
        Self {
            invitation_repo: Arc::clone(&self.invitation_repo),
            team_repo: Arc::clone(&self.team_repo),
            team_season_repo: Arc::clone(&self.team_season_repo),
            member_repo: Arc::clone(&self.member_repo),
            season_repo: Arc::clone(&self.season_repo),
        }
    }
}
