//! Team invitation service with business logic.

use crate::entities::team::{InvitePlayerCommand, TeamInvitation, TeamMember};
use crate::repositories::team::{AddMember, CreateInvitation, TeamInvitationRepository, TeamMemberRepository, TeamRepository};
use crate::repositories::PlayerRepository;
use portal_core::types::{InvitationStatus, TeamRole};
use portal_core::{DomainError, PlayerId, TeamId, TeamInvitationId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for team invitation business logic.
pub struct TeamInvitationService<TIR, TR, TMR, PR>
where
    TIR: TeamInvitationRepository,
    TR: TeamRepository,
    TMR: TeamMemberRepository,
    PR: PlayerRepository,
{
    invitation_repo: Arc<TIR>,
    team_repo: Arc<TR>,
    member_repo: Arc<TMR>,
    player_repo: Arc<PR>,
}

impl<TIR, TR, TMR, PR> TeamInvitationService<TIR, TR, TMR, PR>
where
    TIR: TeamInvitationRepository,
    TR: TeamRepository,
    TMR: TeamMemberRepository,
    PR: PlayerRepository,
{
    /// Create a new team invitation service.
    pub fn new(
        invitation_repo: Arc<TIR>,
        team_repo: Arc<TR>,
        member_repo: Arc<TMR>,
        player_repo: Arc<PR>,
    ) -> Self {
        Self {
            invitation_repo,
            team_repo,
            member_repo,
            player_repo,
        }
    }

    /// Invite a player to a team.
    ///
    /// The inviter must be a captain of the team.
    #[instrument(skip(self))]
    pub async fn invite_player(
        &self,
        team_id: TeamId,
        inviter_id: PlayerId,
        cmd: InvitePlayerCommand,
    ) -> Result<TeamInvitation, DomainError> {
        // Verify team exists
        let _team = self
            .team_repo
            .find_by_id(team_id)
            .await?
            .ok_or_else(|| DomainError::TeamNotFound(team_id.to_string()))?;

        // Verify inviter is a captain
        let is_captain = self.member_repo.is_captain(team_id, inviter_id).await?;
        if !is_captain {
            return Err(DomainError::not_authorized("only captains can invite players"));
        }

        // Verify target player exists
        let _target_player = self
            .player_repo
            .find_by_id(cmd.player_id)
            .await?
            .ok_or_else(|| DomainError::PlayerNotFound(cmd.player_id.to_string()))?;

        // Check if player is already a member
        let is_member = self.member_repo.is_member(team_id, cmd.player_id).await?;
        if is_member {
            return Err(DomainError::Conflict(
                "player is already a member of this team".to_string(),
            ));
        }

        // Check if there's already a pending invitation
        let existing = self
            .invitation_repo
            .find_existing_pending(team_id, cmd.player_id)
            .await?;
        if existing.is_some() {
            return Err(DomainError::Conflict(
                "there is already a pending invitation for this player".to_string(),
            ));
        }

        // Create the invitation
        let invitation = self
            .invitation_repo
            .create(CreateInvitation {
                team_id,
                player_id: cmd.player_id,
                invitation_type: "invite".to_string(),
                role: cmd.role,
                message: cmd.message,
                invited_by: Some(inviter_id),
            })
            .await?;

        info!(
            team_id = %team_id,
            inviter_id = %inviter_id,
            invitee_id = %cmd.player_id,
            "Player invited to team"
        );

        Ok(invitation)
    }

    /// Accept a team invitation.
    ///
    /// The player becomes a member of the team.
    #[instrument(skip(self))]
    pub async fn accept_invitation(
        &self,
        invitation_id: TeamInvitationId,
        player_id: PlayerId,
    ) -> Result<TeamMember, DomainError> {
        // Find the invitation
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::InvitationInvalid)?;

        // Verify this invitation is for this player
        if invitation.player_id != player_id {
            return Err(DomainError::not_authorized(
                "this invitation is not for you",
            ));
        }

        // Verify invitation is actionable
        if !invitation.is_actionable() {
            if invitation.is_expired() {
                return Err(DomainError::InvitationExpired);
            }
            return Err(DomainError::InvitationInvalid);
        }

        // Check if player is already a member (edge case, in case they joined another way)
        let is_member = self
            .member_repo
            .is_member(invitation.team_id, player_id)
            .await?;
        if is_member {
            // Cancel the invitation since they're already a member
            self.invitation_repo
                .update_status(invitation_id, InvitationStatus::Cancelled, None)
                .await?;
            return Err(DomainError::Conflict(
                "you are already a member of this team".to_string(),
            ));
        }

        // Update invitation status
        self.invitation_repo
            .update_status(invitation_id, InvitationStatus::Accepted, None)
            .await?;

        // Add player to team
        let member = self
            .member_repo
            .add_member(AddMember {
                team_id: invitation.team_id,
                player_id,
                role: invitation.role,
                is_founder: false,
                invited_by: invitation.invited_by,
            })
            .await?;

        info!(
            invitation_id = %invitation_id,
            team_id = %invitation.team_id,
            player_id = %player_id,
            "Player accepted team invitation"
        );

        Ok(member)
    }

    /// Decline a team invitation.
    #[instrument(skip(self))]
    pub async fn decline_invitation(
        &self,
        invitation_id: TeamInvitationId,
        player_id: PlayerId,
    ) -> Result<TeamInvitation, DomainError> {
        // Find the invitation
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::InvitationInvalid)?;

        // Verify this invitation is for this player
        if invitation.player_id != player_id {
            return Err(DomainError::not_authorized(
                "this invitation is not for you",
            ));
        }

        // Verify invitation is actionable
        if !invitation.is_actionable() {
            if invitation.is_expired() {
                return Err(DomainError::InvitationExpired);
            }
            return Err(DomainError::InvitationInvalid);
        }

        // Update invitation status
        let updated = self
            .invitation_repo
            .update_status(invitation_id, InvitationStatus::Declined, None)
            .await?;

        info!(
            invitation_id = %invitation_id,
            team_id = %invitation.team_id,
            player_id = %player_id,
            "Player declined team invitation"
        );

        Ok(updated)
    }

    /// Cancel a pending invitation (by team captain).
    #[instrument(skip(self))]
    pub async fn cancel_invitation(
        &self,
        invitation_id: TeamInvitationId,
        captain_id: PlayerId,
    ) -> Result<(), DomainError> {
        // Find the invitation
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::InvitationInvalid)?;

        // Verify captain has permission
        let is_captain = self
            .member_repo
            .is_captain(invitation.team_id, captain_id)
            .await?;
        if !is_captain {
            return Err(DomainError::not_authorized(
                "only captains can cancel invitations",
            ));
        }

        // Verify invitation is pending
        if !invitation.is_pending() {
            return Err(DomainError::InvitationInvalid);
        }

        // Cancel the invitation
        self.invitation_repo
            .update_status(invitation_id, InvitationStatus::Cancelled, None)
            .await?;

        info!(
            invitation_id = %invitation_id,
            team_id = %invitation.team_id,
            captain_id = %captain_id,
            "Captain cancelled team invitation"
        );

        Ok(())
    }

    /// Get pending invitations for a player.
    #[instrument(skip(self))]
    pub async fn get_pending_invitations(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<TeamInvitation>, DomainError> {
        self.invitation_repo.find_pending_for_player(player_id).await
    }

    /// Get pending invitations for a team (for captains).
    #[instrument(skip(self))]
    pub async fn get_team_invitations(
        &self,
        team_id: TeamId,
        actor_id: PlayerId,
    ) -> Result<Vec<TeamInvitation>, DomainError> {
        // Verify actor is a captain or member with appropriate permissions
        let is_captain = self.member_repo.is_captain(team_id, actor_id).await?;
        if !is_captain {
            return Err(DomainError::not_authorized(
                "only captains can view team invitations",
            ));
        }

        self.invitation_repo.find_pending_by_team(team_id).await
    }

    /// Count pending invitations for a player (for UI badge).
    #[instrument(skip(self))]
    pub async fn count_pending_invitations(&self, player_id: PlayerId) -> Result<i64, DomainError> {
        self.invitation_repo.count_pending_for_player(player_id).await
    }
}

impl<TIR, TR, TMR, PR> Clone for TeamInvitationService<TIR, TR, TMR, PR>
where
    TIR: TeamInvitationRepository,
    TR: TeamRepository,
    TMR: TeamMemberRepository,
    PR: PlayerRepository,
{
    fn clone(&self) -> Self {
        Self {
            invitation_repo: Arc::clone(&self.invitation_repo),
            team_repo: Arc::clone(&self.team_repo),
            member_repo: Arc::clone(&self.member_repo),
            player_repo: Arc::clone(&self.player_repo),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::team::{InvitationType, Team, TeamMember, TeamMemberStatus};
    use crate::entities::player::{Player, SocialLinks};
    use crate::repositories::team::{MockTeamInvitationRepository, MockTeamMemberRepository, MockTeamRepository};
    use crate::repositories::user::MockPlayerRepository;
    use chrono::{Duration, Utc};
    use mockall::predicate::*;
    use portal_core::types::TeamStatus;
    use portal_core::UserId;

    fn make_team() -> Team {
        Team {
            id: TeamId::new(),
            name: "Test Team".to_string(),
            tag: "TST".to_string(),
            description: None,
            logo_url: None,
            banner_url: None,
            primary_color: None,
            secondary_color: None,
            created_by: PlayerId::new(),
            game_id: None,
            status: TeamStatus::Active,
            disbanded_at: None,
            disbanded_reason: None,
            total_matches: 0,
            total_wins: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_player(id: PlayerId) -> Player {
        Player {
            id,
            user_id: UserId::new(),
            display_name: "TestPlayer".to_string(),
            avatar_url: None,
            banner_url: None,
            bio: None,
            country_code: None,
            region: None,
            timezone: None,
            social_links: SocialLinks::default(),
            steam_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_invitation(
        team_id: TeamId,
        player_id: PlayerId,
        status: InvitationStatus,
        expires_in: Duration,
    ) -> TeamInvitation {
        TeamInvitation {
            id: TeamInvitationId::new(),
            team_id,
            player_id,
            invitation_type: InvitationType::Invite,
            role: TeamRole::Player,
            message: None,
            invited_by: Some(PlayerId::new()),
            status,
            responded_at: None,
            response_message: None,
            expires_at: Utc::now() + expires_in,
            created_at: Utc::now(),
        }
    }

    fn make_member(team_id: TeamId, player_id: PlayerId, role: TeamRole, is_founder: bool) -> TeamMember {
        TeamMember {
            team_id,
            player_id,
            display_name: "TestPlayer".to_string(),
            avatar_url: None,
            role,
            role_title: None,
            is_founder,
            primary_position: None,
            secondary_position: None,
            status: TeamMemberStatus::Active,
            jersey_number: None,
            invited_by: None,
            joined_at: Utc::now(),
            left_at: None,
        }
    }

    fn make_service(
        invitation_repo: MockTeamInvitationRepository,
        team_repo: MockTeamRepository,
        member_repo: MockTeamMemberRepository,
        player_repo: MockPlayerRepository,
    ) -> TeamInvitationService<MockTeamInvitationRepository, MockTeamRepository, MockTeamMemberRepository, MockPlayerRepository>
    {
        TeamInvitationService::new(
            Arc::new(invitation_repo),
            Arc::new(team_repo),
            Arc::new(member_repo),
            Arc::new(player_repo),
        )
    }

    // ==================== invite_player tests ====================

    #[tokio::test]
    async fn test_invite_player_success() {
        let team = make_team();
        let team_id = team.id;
        let inviter_id = PlayerId::new();
        let target_id = PlayerId::new();

        let expected_invitation = make_invitation(team_id, target_id, InvitationStatus::Pending, Duration::days(7));

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let mut team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let mut player_repo = MockPlayerRepository::new();

        team_repo
            .expect_find_by_id()
            .with(eq(team_id))
            .returning(move |_| Ok(Some(make_team())));

        member_repo
            .expect_is_captain()
            .with(eq(team_id), eq(inviter_id))
            .returning(|_, _| Ok(true));

        player_repo
            .expect_find_by_id()
            .with(eq(target_id))
            .returning(move |id| Ok(Some(make_player(id))));

        member_repo
            .expect_is_member()
            .with(eq(team_id), eq(target_id))
            .returning(|_, _| Ok(false));

        invitation_repo
            .expect_find_existing_pending()
            .with(eq(team_id), eq(target_id))
            .returning(|_, _| Ok(None));

        let inv_clone = expected_invitation.clone();
        invitation_repo
            .expect_create()
            .returning(move |_| Ok(inv_clone.clone()));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service
            .invite_player(
                team_id,
                inviter_id,
                InvitePlayerCommand {
                    player_id: target_id,
                    role: TeamRole::Player,
                    message: None,
                },
            )
            .await;

        assert!(result.is_ok());
        let invitation = result.unwrap();
        assert_eq!(invitation.player_id, target_id);
        assert_eq!(invitation.status, InvitationStatus::Pending);
    }

    #[tokio::test]
    async fn test_invite_player_team_not_found() {
        let team_id = TeamId::new();
        let inviter_id = PlayerId::new();
        let target_id = PlayerId::new();

        let invitation_repo = MockTeamInvitationRepository::new();
        let mut team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        team_repo
            .expect_find_by_id()
            .with(eq(team_id))
            .returning(|_| Ok(None));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service
            .invite_player(
                team_id,
                inviter_id,
                InvitePlayerCommand {
                    player_id: target_id,
                    role: TeamRole::Player,
                    message: None,
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::TeamNotFound(_))));
    }

    #[tokio::test]
    async fn test_invite_player_not_captain() {
        let team = make_team();
        let team_id = team.id;
        let inviter_id = PlayerId::new();
        let target_id = PlayerId::new();

        let invitation_repo = MockTeamInvitationRepository::new();
        let mut team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        team_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(make_team())));

        member_repo
            .expect_is_captain()
            .returning(|_, _| Ok(false));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service
            .invite_player(
                team_id,
                inviter_id,
                InvitePlayerCommand {
                    player_id: target_id,
                    role: TeamRole::Player,
                    message: None,
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotAuthorized(_))));
    }

    #[tokio::test]
    async fn test_invite_player_already_member() {
        let team = make_team();
        let team_id = team.id;
        let inviter_id = PlayerId::new();
        let target_id = PlayerId::new();

        let invitation_repo = MockTeamInvitationRepository::new();
        let mut team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let mut player_repo = MockPlayerRepository::new();

        team_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(make_team())));

        member_repo
            .expect_is_captain()
            .returning(|_, _| Ok(true));

        player_repo
            .expect_find_by_id()
            .returning(move |id| Ok(Some(make_player(id))));

        member_repo
            .expect_is_member()
            .returning(|_, _| Ok(true));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service
            .invite_player(
                team_id,
                inviter_id,
                InvitePlayerCommand {
                    player_id: target_id,
                    role: TeamRole::Player,
                    message: None,
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn test_invite_player_pending_exists() {
        let team = make_team();
        let team_id = team.id;
        let inviter_id = PlayerId::new();
        let target_id = PlayerId::new();

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let mut team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let mut player_repo = MockPlayerRepository::new();

        team_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(make_team())));

        member_repo
            .expect_is_captain()
            .returning(|_, _| Ok(true));

        player_repo
            .expect_find_by_id()
            .returning(move |id| Ok(Some(make_player(id))));

        member_repo
            .expect_is_member()
            .returning(|_, _| Ok(false));

        let existing = make_invitation(team_id, target_id, InvitationStatus::Pending, Duration::days(7));
        invitation_repo
            .expect_find_existing_pending()
            .returning(move |_, _| Ok(Some(existing.clone())));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service
            .invite_player(
                team_id,
                inviter_id,
                InvitePlayerCommand {
                    player_id: target_id,
                    role: TeamRole::Player,
                    message: None,
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    // ==================== accept_invitation tests ====================

    #[tokio::test]
    async fn test_accept_invitation_success() {
        let team_id = TeamId::new();
        let player_id = PlayerId::new();
        let invitation = make_invitation(team_id, player_id, InvitationStatus::Pending, Duration::days(7));
        let invitation_id = invitation.id;

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        let inv_clone = invitation.clone();
        invitation_repo
            .expect_find_by_id()
            .with(eq(invitation_id))
            .returning(move |_| Ok(Some(inv_clone.clone())));

        member_repo
            .expect_is_member()
            .returning(|_, _| Ok(false));

        let updated_inv = make_invitation(team_id, player_id, InvitationStatus::Accepted, Duration::days(7));
        invitation_repo
            .expect_update_status()
            .returning(move |_, _, _| Ok(updated_inv.clone()));

        let member = make_member(team_id, player_id, TeamRole::Player, false);
        member_repo
            .expect_add_member()
            .returning(move |_| Ok(member.clone()));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.accept_invitation(invitation_id, player_id).await;

        assert!(result.is_ok());
        let member = result.unwrap();
        assert_eq!(member.player_id, player_id);
        assert_eq!(member.team_id, team_id);
    }

    #[tokio::test]
    async fn test_accept_invitation_not_found() {
        let invitation_id = TeamInvitationId::new();
        let player_id = PlayerId::new();

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        invitation_repo
            .expect_find_by_id()
            .returning(|_| Ok(None));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.accept_invitation(invitation_id, player_id).await;

        assert!(matches!(result, Err(DomainError::InvitationInvalid)));
    }

    #[tokio::test]
    async fn test_accept_invitation_wrong_player() {
        let team_id = TeamId::new();
        let invitation_player_id = PlayerId::new();
        let wrong_player_id = PlayerId::new();
        let invitation = make_invitation(team_id, invitation_player_id, InvitationStatus::Pending, Duration::days(7));
        let invitation_id = invitation.id;

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        invitation_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(invitation.clone())));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.accept_invitation(invitation_id, wrong_player_id).await;

        assert!(matches!(result, Err(DomainError::NotAuthorized(_))));
    }

    #[tokio::test]
    async fn test_accept_invitation_expired() {
        let team_id = TeamId::new();
        let player_id = PlayerId::new();
        // Create expired invitation
        let invitation = make_invitation(team_id, player_id, InvitationStatus::Pending, Duration::days(-1));
        let invitation_id = invitation.id;

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        invitation_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(invitation.clone())));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.accept_invitation(invitation_id, player_id).await;

        assert!(matches!(result, Err(DomainError::InvitationExpired)));
    }

    #[tokio::test]
    async fn test_accept_invitation_already_accepted() {
        let team_id = TeamId::new();
        let player_id = PlayerId::new();
        let invitation = make_invitation(team_id, player_id, InvitationStatus::Accepted, Duration::days(7));
        let invitation_id = invitation.id;

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        invitation_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(invitation.clone())));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.accept_invitation(invitation_id, player_id).await;

        assert!(matches!(result, Err(DomainError::InvitationInvalid)));
    }

    // ==================== decline_invitation tests ====================

    #[tokio::test]
    async fn test_decline_invitation_success() {
        let team_id = TeamId::new();
        let player_id = PlayerId::new();
        let invitation = make_invitation(team_id, player_id, InvitationStatus::Pending, Duration::days(7));
        let invitation_id = invitation.id;

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        let inv_clone = invitation.clone();
        invitation_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(inv_clone.clone())));

        let declined_inv = make_invitation(team_id, player_id, InvitationStatus::Declined, Duration::days(7));
        invitation_repo
            .expect_update_status()
            .returning(move |_, _, _| Ok(declined_inv.clone()));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.decline_invitation(invitation_id, player_id).await;

        assert!(result.is_ok());
    }

    // ==================== cancel_invitation tests ====================

    #[tokio::test]
    async fn test_cancel_invitation_success() {
        let team_id = TeamId::new();
        let target_player_id = PlayerId::new();
        let captain_id = PlayerId::new();
        let invitation = make_invitation(team_id, target_player_id, InvitationStatus::Pending, Duration::days(7));
        let invitation_id = invitation.id;

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        let inv_clone = invitation.clone();
        invitation_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(inv_clone.clone())));

        member_repo
            .expect_is_captain()
            .with(eq(team_id), eq(captain_id))
            .returning(|_, _| Ok(true));

        let cancelled_inv = make_invitation(team_id, target_player_id, InvitationStatus::Cancelled, Duration::days(7));
        invitation_repo
            .expect_update_status()
            .returning(move |_, _, _| Ok(cancelled_inv.clone()));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.cancel_invitation(invitation_id, captain_id).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cancel_invitation_not_captain() {
        let team_id = TeamId::new();
        let target_player_id = PlayerId::new();
        let non_captain_id = PlayerId::new();
        let invitation = make_invitation(team_id, target_player_id, InvitationStatus::Pending, Duration::days(7));
        let invitation_id = invitation.id;

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        let inv_clone = invitation.clone();
        invitation_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(inv_clone.clone())));

        member_repo
            .expect_is_captain()
            .returning(|_, _| Ok(false));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.cancel_invitation(invitation_id, non_captain_id).await;

        assert!(matches!(result, Err(DomainError::NotAuthorized(_))));
    }

    // ==================== get_pending_invitations tests ====================

    #[tokio::test]
    async fn test_get_pending_invitations() {
        let player_id = PlayerId::new();
        let team_id = TeamId::new();
        let invitations = vec![
            make_invitation(team_id, player_id, InvitationStatus::Pending, Duration::days(7)),
            make_invitation(TeamId::new(), player_id, InvitationStatus::Pending, Duration::days(7)),
        ];

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        let invitations_clone = invitations.clone();
        invitation_repo
            .expect_find_pending_for_player()
            .with(eq(player_id))
            .returning(move |_| Ok(invitations_clone.clone()));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.get_pending_invitations(player_id).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    // ==================== get_team_invitations tests ====================

    #[tokio::test]
    async fn test_get_team_invitations_as_captain() {
        let team_id = TeamId::new();
        let captain_id = PlayerId::new();
        let invitations = vec![
            make_invitation(team_id, PlayerId::new(), InvitationStatus::Pending, Duration::days(7)),
        ];

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        member_repo
            .expect_is_captain()
            .with(eq(team_id), eq(captain_id))
            .returning(|_, _| Ok(true));

        let invitations_clone = invitations.clone();
        invitation_repo
            .expect_find_pending_by_team()
            .with(eq(team_id))
            .returning(move |_| Ok(invitations_clone.clone()));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.get_team_invitations(team_id, captain_id).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_get_team_invitations_not_captain() {
        let team_id = TeamId::new();
        let non_captain_id = PlayerId::new();

        let invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let mut member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        member_repo
            .expect_is_captain()
            .returning(|_, _| Ok(false));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.get_team_invitations(team_id, non_captain_id).await;

        assert!(matches!(result, Err(DomainError::NotAuthorized(_))));
    }

    // ==================== count_pending_invitations tests ====================

    #[tokio::test]
    async fn test_count_pending_invitations() {
        let player_id = PlayerId::new();

        let mut invitation_repo = MockTeamInvitationRepository::new();
        let team_repo = MockTeamRepository::new();
        let member_repo = MockTeamMemberRepository::new();
        let player_repo = MockPlayerRepository::new();

        invitation_repo
            .expect_count_pending_for_player()
            .with(eq(player_id))
            .returning(|_| Ok(5));

        let service = make_service(invitation_repo, team_repo, member_repo, player_repo);

        let result = service.count_pending_invitations(player_id).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }
}
