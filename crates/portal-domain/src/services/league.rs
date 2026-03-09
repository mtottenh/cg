//! League service with business logic.

use crate::entities::league::{
    CreateLeagueCommand, League, LeagueAccessType, LeagueInvitation, LeagueInvitationStatus,
    LeagueMember, LeagueMemberWithUser, LeagueMembershipType, UpdateLeagueCommand,
    UserLeagueMembership,
};
use crate::repositories::league::{
    AddLeagueMember, CreateLeague, CreateLeagueInvitation, LeagueInvitationRepository,
    LeagueMemberRepository, LeagueRepository, UpdateLeague,
};
use portal_core::{DomainError, GameId, LeagueId, LeagueInvitationId, UserId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for league-related business logic.
pub struct LeagueService<LR, LMR, LIR>
where
    LR: LeagueRepository,
    LMR: LeagueMemberRepository,
    LIR: LeagueInvitationRepository,
{
    league_repo: Arc<LR>,
    member_repo: Arc<LMR>,
    invitation_repo: Arc<LIR>,
}

impl<LR, LMR, LIR> LeagueService<LR, LMR, LIR>
where
    LR: LeagueRepository,
    LMR: LeagueMemberRepository,
    LIR: LeagueInvitationRepository,
{
    /// Create a new league service.
    pub const fn new(league_repo: Arc<LR>, member_repo: Arc<LMR>, invitation_repo: Arc<LIR>) -> Self {
        Self {
            league_repo,
            member_repo,
            invitation_repo,
        }
    }

    /// Get a league by ID.
    #[instrument(skip(self))]
    pub async fn get_league(&self, id: LeagueId) -> Result<League, DomainError> {
        self.league_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::LeagueNotFound(id.to_string()))
    }

    /// Get a league by slug.
    #[instrument(skip(self))]
    pub async fn get_league_by_slug(&self, slug: &str) -> Result<League, DomainError> {
        self.league_repo
            .find_by_slug(slug)
            .await?
            .ok_or_else(|| DomainError::LeagueNotFound(slug.to_string()))
    }

    /// Create a new league.
    ///
    /// The creating user automatically becomes the founding admin.
    #[instrument(skip(self))]
    pub async fn create_league(
        &self,
        creator_id: UserId,
        cmd: CreateLeagueCommand,
    ) -> Result<League, DomainError> {
        // Check slug uniqueness
        if self.league_repo.slug_exists(&cmd.slug).await? {
            return Err(DomainError::Conflict(format!(
                "league slug '{}' is already taken",
                cmd.slug
            )));
        }

        // Create the league
        let league = self
            .league_repo
            .create(CreateLeague {
                game_id: cmd.game_id,
                name: cmd.name,
                slug: cmd.slug,
                description: cmd.description,
                logo_url: cmd.logo_url,
                access_type: cmd.access_type.as_str().to_string(),
                settings: cmd.settings,
                created_by: creator_id,
            })
            .await?;

        // Add the creator as founding admin
        self.member_repo
            .add_member(AddLeagueMember {
                league_id: league.id,
                user_id: creator_id,
                membership_type: LeagueMembershipType::Admin,
            })
            .await?;

        info!(league_id = %league.id, creator_id = %creator_id, "League created");

        Ok(league)
    }

    /// Update a league (authorized version - caller must verify permissions).
    #[instrument(skip(self))]
    pub async fn update_league_authorized(
        &self,
        league_id: LeagueId,
        cmd: UpdateLeagueCommand,
    ) -> Result<League, DomainError> {
        // Check slug uniqueness if changing
        if let Some(ref slug) = cmd.slug {
            let league = self.get_league(league_id).await?;
            if slug != &league.slug && self.league_repo.slug_exists(slug).await? {
                return Err(DomainError::Conflict(format!(
                    "league slug '{slug}' is already taken"
                )));
            }
        }

        let league = self
            .league_repo
            .update(
                league_id,
                UpdateLeague {
                    name: cmd.name,
                    slug: cmd.slug,
                    description: cmd.description,
                    logo_url: cmd.logo_url,
                    access_type: cmd.access_type.map(|a| a.as_str().to_string()),
                    status: cmd.status.map(|s| s.as_str().to_string()),
                    settings: cmd.settings,
                },
            )
            .await?;

        info!(league_id = %league_id, "League updated (authorized)");

        Ok(league)
    }

    /// List leagues for a game with pagination.
    #[instrument(skip(self))]
    pub async fn list_leagues_by_game(
        &self,
        game_id: &GameId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<League>, i64), DomainError> {
        let leagues = self.league_repo.list_by_game(game_id, limit, offset).await?;
        let total = self.league_repo.count_by_game(game_id).await?;
        Ok((leagues, total))
    }

    /// Search leagues.
    #[instrument(skip(self))]
    pub async fn search_leagues(
        &self,
        query: &str,
        game_id: Option<GameId>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<League>, i64), DomainError> {
        let leagues = self.league_repo.search(query, game_id, limit, offset).await?;
        let total = self.league_repo.count_search(query, game_id).await?;
        Ok((leagues, total))
    }

    // =========================================================================
    // Member Management
    // =========================================================================

    /// Get league members with pagination.
    #[instrument(skip(self))]
    pub async fn get_members(
        &self,
        league_id: LeagueId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueMemberWithUser>, i64), DomainError> {
        // Verify league exists
        let _ = self.get_league(league_id).await?;
        let members = self.member_repo.list_members(league_id, limit, offset).await?;
        let total = self.member_repo.count_members(league_id).await?;
        Ok((members, total))
    }

    /// Join an open league.
    #[instrument(skip(self))]
    pub async fn join_league(&self, league_id: LeagueId, user_id: UserId) -> Result<LeagueMember, DomainError> {
        let league = self.get_league(league_id).await?;

        // Only open leagues can be joined directly
        if league.access_type != LeagueAccessType::Open {
            return Err(DomainError::LeagueInviteOnly);
        }

        // Check if already a member
        if self.member_repo.is_member(league_id, user_id).await? {
            return Err(DomainError::Conflict("Already a member of this league".to_string()));
        }

        let member = self
            .member_repo
            .add_member(AddLeagueMember {
                league_id,
                user_id,
                membership_type: LeagueMembershipType::Member,
            })
            .await?;

        info!(league_id = %league_id, user_id = %user_id, "User joined league");

        Ok(member)
    }

    /// Leave a league voluntarily.
    #[instrument(skip(self))]
    pub async fn leave_league(&self, league_id: LeagueId, user_id: UserId) -> Result<(), DomainError> {
        // Check if member
        let member = self
            .member_repo
            .find_member(league_id, user_id)
            .await?
            .ok_or(DomainError::NotLeagueMember)?;

        // If leaving as the last admin, prevent it
        if member.membership_type == LeagueMembershipType::Admin {
            let admin_count = self.member_repo.count_admins(league_id).await?;
            if admin_count <= 1 {
                return Err(DomainError::Conflict(
                    "Cannot leave: you are the last admin. Transfer admin role first.".to_string(),
                ));
            }
        }

        self.member_repo.remove_member(league_id, user_id).await?;

        info!(league_id = %league_id, user_id = %user_id, "User left league");

        Ok(())
    }

    /// Remove a member from a league (authorized).
    #[instrument(skip(self))]
    pub async fn remove_member_authorized(
        &self,
        league_id: LeagueId,
        target_user_id: UserId,
    ) -> Result<(), DomainError> {
        let member = self
            .member_repo
            .find_member(league_id, target_user_id)
            .await?
            .ok_or(DomainError::NotLeagueMember)?;

        // If removing an admin, ensure there's at least one other admin
        if member.membership_type == LeagueMembershipType::Admin {
            let admin_count = self.member_repo.count_admins(league_id).await?;
            if admin_count <= 1 {
                return Err(DomainError::Conflict(
                    "Cannot remove the last admin from the league".to_string(),
                ));
            }
        }

        self.member_repo.remove_member(league_id, target_user_id).await?;

        info!(
            league_id = %league_id,
            removed_id = %target_user_id,
            "Member removed from league (authorized)"
        );

        Ok(())
    }

    /// Update a member's role (authorized).
    #[instrument(skip(self))]
    pub async fn update_member_role_authorized(
        &self,
        league_id: LeagueId,
        target_user_id: UserId,
        new_role: LeagueMembershipType,
    ) -> Result<LeagueMember, DomainError> {
        let member = self
            .member_repo
            .find_member(league_id, target_user_id)
            .await?
            .ok_or(DomainError::NotLeagueMember)?;

        // If demoting an admin, ensure there's at least one other admin
        if member.membership_type == LeagueMembershipType::Admin && new_role != LeagueMembershipType::Admin {
            let admin_count = self.member_repo.count_admins(league_id).await?;
            if admin_count <= 1 {
                return Err(DomainError::Conflict(
                    "Cannot demote the last admin. Promote another admin first.".to_string(),
                ));
            }
        }

        let updated = self
            .member_repo
            .update_membership_type(league_id, target_user_id, new_role)
            .await?;

        info!(
            league_id = %league_id,
            target_id = %target_user_id,
            new_role = %new_role,
            "Member role updated (authorized)"
        );

        Ok(updated)
    }

    /// Get a user's league memberships.
    #[instrument(skip(self))]
    pub async fn get_user_leagues(&self, user_id: UserId) -> Result<Vec<UserLeagueMembership>, DomainError> {
        self.member_repo.list_memberships_for_user(user_id).await
    }

    /// Check if a user is a member of a league.
    pub async fn is_member(&self, league_id: LeagueId, user_id: UserId) -> Result<bool, DomainError> {
        self.member_repo.is_member(league_id, user_id).await
    }

    /// Check if a user is an admin of a league.
    pub async fn is_admin(&self, league_id: LeagueId, user_id: UserId) -> Result<bool, DomainError> {
        self.member_repo.is_admin(league_id, user_id).await
    }

    /// Check if a user is an admin or moderator of a league.
    pub async fn is_admin_or_moderator(
        &self,
        league_id: LeagueId,
        user_id: UserId,
    ) -> Result<bool, DomainError> {
        self.member_repo.is_admin_or_moderator(league_id, user_id).await
    }

    // =========================================================================
    // Invitation / Application Management
    // =========================================================================

    /// Apply to join a league (for application-based leagues).
    #[instrument(skip(self))]
    pub async fn apply_to_league(
        &self,
        league_id: LeagueId,
        user_id: UserId,
        message: Option<String>,
    ) -> Result<LeagueInvitation, DomainError> {
        let league = self.get_league(league_id).await?;

        // Check league access type
        if league.access_type != LeagueAccessType::Application {
            return Err(DomainError::Conflict(
                "This league does not accept applications".to_string(),
            ));
        }

        // Check if already a member
        if self.member_repo.is_member(league_id, user_id).await? {
            return Err(DomainError::Conflict("Already a member of this league".to_string()));
        }

        // Check if already has pending application
        if self.invitation_repo.find_pending(league_id, user_id).await?.is_some() {
            return Err(DomainError::Conflict(
                "You already have a pending application".to_string(),
            ));
        }

        let invitation = self
            .invitation_repo
            .create(CreateLeagueInvitation {
                league_id,
                user_id,
                invitation_type: "application".to_string(),
                message,
                invited_by: None,
                expires_at: None, // Applications don't expire
            })
            .await?;

        info!(league_id = %league_id, user_id = %user_id, "User applied to league");

        Ok(invitation)
    }

    /// Invite a user to a league (authorized).
    #[instrument(skip(self))]
    pub async fn invite_user_authorized(
        &self,
        league_id: LeagueId,
        target_user_id: UserId,
        invited_by: UserId,
        message: Option<String>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<LeagueInvitation, DomainError> {
        // Check if already a member
        if self.member_repo.is_member(league_id, target_user_id).await? {
            return Err(DomainError::Conflict("User is already a member".to_string()));
        }

        // Check if already has pending invitation
        if self
            .invitation_repo
            .find_pending(league_id, target_user_id)
            .await?
            .is_some()
        {
            return Err(DomainError::Conflict(
                "User already has a pending invitation".to_string(),
            ));
        }

        let invitation = self
            .invitation_repo
            .create(CreateLeagueInvitation {
                league_id,
                user_id: target_user_id,
                invitation_type: "invite".to_string(),
                message,
                invited_by: Some(invited_by),
                expires_at,
            })
            .await?;

        info!(
            league_id = %league_id,
            invited_user = %target_user_id,
            invited_by = %invited_by,
            "User invited to league"
        );

        Ok(invitation)
    }

    /// Accept an invitation.
    #[instrument(skip(self))]
    pub async fn accept_invitation(
        &self,
        invitation_id: LeagueInvitationId,
        user_id: UserId,
    ) -> Result<LeagueMember, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::InvitationInvalid)?;

        // Verify the invitation belongs to this user (for invites)
        if invitation.user_id != user_id {
            return Err(DomainError::InvitationInvalid);
        }

        // Check status
        if invitation.status != LeagueInvitationStatus::Pending {
            return Err(DomainError::InvitationInvalid);
        }

        // Check expiration
        if let Some(expires_at) = invitation.expires_at {
            if expires_at < chrono::Utc::now() {
                return Err(DomainError::InvitationExpired);
            }
        }

        // Update invitation status
        self.invitation_repo
            .update_status(invitation_id, LeagueInvitationStatus::Accepted, user_id)
            .await?;

        // Add user as member
        let member = self
            .member_repo
            .add_member(AddLeagueMember {
                league_id: invitation.league_id,
                user_id,
                membership_type: LeagueMembershipType::Member,
            })
            .await?;

        info!(
            league_id = %invitation.league_id,
            user_id = %user_id,
            invitation_id = %invitation_id,
            "Invitation accepted"
        );

        Ok(member)
    }

    /// Approve an application (authorized - for admins).
    #[instrument(skip(self))]
    pub async fn approve_application_authorized(
        &self,
        invitation_id: LeagueInvitationId,
        approved_by: UserId,
    ) -> Result<LeagueMember, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::InvitationInvalid)?;

        // Check status
        if invitation.status != LeagueInvitationStatus::Pending {
            return Err(DomainError::InvitationInvalid);
        }

        // Update invitation status
        self.invitation_repo
            .update_status(invitation_id, LeagueInvitationStatus::Accepted, approved_by)
            .await?;

        // Add user as member
        let member = self
            .member_repo
            .add_member(AddLeagueMember {
                league_id: invitation.league_id,
                user_id: invitation.user_id,
                membership_type: LeagueMembershipType::Member,
            })
            .await?;

        info!(
            league_id = %invitation.league_id,
            applicant = %invitation.user_id,
            approved_by = %approved_by,
            "Application approved"
        );

        Ok(member)
    }

    /// Reject an invitation/application.
    #[instrument(skip(self))]
    pub async fn reject_invitation_authorized(
        &self,
        invitation_id: LeagueInvitationId,
        rejected_by: UserId,
    ) -> Result<LeagueInvitation, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::InvitationInvalid)?;

        // Check status
        if invitation.status != LeagueInvitationStatus::Pending {
            return Err(DomainError::InvitationInvalid);
        }

        let updated = self
            .invitation_repo
            .update_status(invitation_id, LeagueInvitationStatus::Rejected, rejected_by)
            .await?;

        info!(
            league_id = %invitation.league_id,
            invitation_id = %invitation_id,
            rejected_by = %rejected_by,
            "Invitation/application rejected"
        );

        Ok(updated)
    }

    /// Get pending invitations for a user.
    #[instrument(skip(self))]
    pub async fn get_pending_invitations_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<LeagueInvitation>, DomainError> {
        self.invitation_repo.list_pending_for_user(user_id).await
    }

    /// Get pending invitations/applications for a league (authorized).
    #[instrument(skip(self))]
    pub async fn get_pending_by_league_authorized(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueInvitation>, DomainError> {
        self.invitation_repo.list_pending_by_league(league_id).await
    }

    /// Decline an invitation (for the invited user to decline).
    #[instrument(skip(self))]
    pub async fn decline_invitation(
        &self,
        invitation_id: LeagueInvitationId,
        user_id: UserId,
    ) -> Result<LeagueInvitation, DomainError> {
        let invitation = self
            .invitation_repo
            .find_by_id(invitation_id)
            .await?
            .ok_or(DomainError::InvitationInvalid)?;

        // Verify the invitation belongs to this user
        if invitation.user_id != user_id {
            return Err(DomainError::InvitationInvalid);
        }

        // Check status
        if invitation.status != LeagueInvitationStatus::Pending {
            return Err(DomainError::InvitationInvalid);
        }

        let updated = self
            .invitation_repo
            .update_status(invitation_id, LeagueInvitationStatus::Rejected, user_id)
            .await?;

        info!(
            league_id = %invitation.league_id,
            invitation_id = %invitation_id,
            user_id = %user_id,
            "User declined invitation"
        );

        Ok(updated)
    }
}

impl<LR, LMR, LIR> Clone for LeagueService<LR, LMR, LIR>
where
    LR: LeagueRepository,
    LMR: LeagueMemberRepository,
    LIR: LeagueInvitationRepository,
{
    fn clone(&self) -> Self {
        Self {
            league_repo: Arc::clone(&self.league_repo),
            member_repo: Arc::clone(&self.member_repo),
            invitation_repo: Arc::clone(&self.invitation_repo),
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests would use mockall to mock the repositories
    // Example structure:
    //
    // use super::*;
    // use crate::repositories::league::{MockLeagueRepository, MockLeagueMemberRepository, MockLeagueInvitationRepository};
    //
    // #[tokio::test]
    // async fn test_create_league() {
    //     let mut league_repo = MockLeagueRepository::new();
    //     league_repo.expect_slug_exists().returning(|_| Ok(false));
    //     // ... setup and test
    // }
}
