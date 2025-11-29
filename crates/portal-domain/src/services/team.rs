//! Team service with business logic.

use crate::entities::team::{
    CreateTeamCommand, Team, TeamMember, UpdateMemberRoleCommand, UpdateTeamCommand,
};
use crate::repositories::team::{AddMember, CreateTeam, TeamMemberRepository, TeamRepository, UpdateTeam};
use crate::repositories::PlayerRepository;
use portal_core::types::TeamRole;
use portal_core::{DomainError, PlayerId, TeamId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for team-related business logic.
pub struct TeamService<TR, TMR, PR>
where
    TR: TeamRepository,
    TMR: TeamMemberRepository,
    PR: PlayerRepository,
{
    team_repo: Arc<TR>,
    member_repo: Arc<TMR>,
    player_repo: Arc<PR>,
}

impl<TR, TMR, PR> TeamService<TR, TMR, PR>
where
    TR: TeamRepository,
    TMR: TeamMemberRepository,
    PR: PlayerRepository,
{
    /// Create a new team service.
    pub fn new(team_repo: Arc<TR>, member_repo: Arc<TMR>, player_repo: Arc<PR>) -> Self {
        Self {
            team_repo,
            member_repo,
            player_repo,
        }
    }

    /// Get a team by ID.
    #[instrument(skip(self))]
    pub async fn get_team(&self, id: TeamId) -> Result<Team, DomainError> {
        self.team_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::TeamNotFound(id.to_string()))
    }

    /// Get team members.
    #[instrument(skip(self))]
    pub async fn get_members(&self, team_id: TeamId) -> Result<Vec<TeamMember>, DomainError> {
        // Verify team exists
        let _ = self.get_team(team_id).await?;
        self.member_repo.list_members(team_id).await
    }

    /// Create a new team.
    ///
    /// The creating player automatically becomes the founding captain.
    #[instrument(skip(self))]
    pub async fn create_team(
        &self,
        creator_id: PlayerId,
        cmd: CreateTeamCommand,
    ) -> Result<Team, DomainError> {
        // Verify the creator exists
        let _creator = self
            .player_repo
            .find_by_id(creator_id)
            .await?
            .ok_or_else(|| DomainError::PlayerNotFound(creator_id.to_string()))?;

        // Check name uniqueness
        if self.team_repo.name_exists(&cmd.name).await? {
            return Err(DomainError::Conflict(format!(
                "team name '{}' is already taken",
                cmd.name
            )));
        }

        // Check tag uniqueness
        if self.team_repo.tag_exists(&cmd.tag).await? {
            return Err(DomainError::Conflict(format!(
                "team tag '{}' is already taken",
                cmd.tag
            )));
        }

        // Create the team
        let team = self
            .team_repo
            .create(CreateTeam {
                name: cmd.name,
                tag: cmd.tag,
                created_by: creator_id,
                description: cmd.description,
                logo_url: cmd.logo_url,
                game_id: cmd.game_id,
            })
            .await?;

        // Add the creator as founding captain
        self.member_repo
            .add_member(AddMember {
                team_id: team.id,
                player_id: creator_id,
                role: TeamRole::Captain,
                is_founder: true,
                invited_by: None,
            })
            .await?;

        info!(team_id = %team.id, creator_id = %creator_id, "Team created");

        Ok(team)
    }

    /// Update a team (authorized version - caller must verify permissions).
    ///
    /// This method skips internal permission checks and should only be called
    /// after the API layer has verified the user has the required RBAC permissions.
    #[instrument(skip(self))]
    pub async fn update_team_authorized(
        &self,
        team_id: TeamId,
        cmd: UpdateTeamCommand,
    ) -> Result<Team, DomainError> {
        // Check name uniqueness if changing
        if let Some(ref name) = cmd.name {
            let team = self.get_team(team_id).await?;
            if name.to_lowercase() != team.name.to_lowercase() && self.team_repo.name_exists(name).await? {
                return Err(DomainError::Conflict(format!(
                    "team name '{}' is already taken",
                    name
                )));
            }
        }

        // Check tag uniqueness if changing
        if let Some(ref tag) = cmd.tag {
            let team = self.get_team(team_id).await?;
            if tag.to_lowercase() != team.tag.to_lowercase() && self.team_repo.tag_exists(tag).await? {
                return Err(DomainError::Conflict(format!(
                    "team tag '{}' is already taken",
                    tag
                )));
            }
        }

        let team = self
            .team_repo
            .update(
                team_id,
                UpdateTeam {
                    name: cmd.name,
                    tag: cmd.tag,
                    description: cmd.description,
                    logo_url: cmd.logo_url,
                    banner_url: cmd.banner_url,
                    primary_color: cmd.primary_color,
                    secondary_color: cmd.secondary_color,
                    website_url: cmd.website_url,
                    status: None,
                },
            )
            .await?;

        info!(team_id = %team_id, "Team updated (authorized)");

        Ok(team)
    }

    /// Update a member's role (authorized version - caller must verify RBAC permissions).
    ///
    /// This method skips internal permission checks and should only be called
    /// after the API layer has verified the user has the required RBAC permissions
    /// (team.roles.manage scoped to this team).
    ///
    /// Domain invariants are still enforced:
    /// - Cannot demote founders
    /// - Team must have at least one captain
    #[instrument(skip(self))]
    pub async fn update_member_role_authorized(
        &self,
        team_id: TeamId,
        cmd: UpdateMemberRoleCommand,
    ) -> Result<TeamMember, DomainError> {
        let target = self
            .member_repo
            .find_member(team_id, cmd.player_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        // Cannot change founder's role (domain invariant)
        if target.is_founder && cmd.new_role != TeamRole::Captain {
            return Err(DomainError::CannotDemoteFounder);
        }

        // If demoting a captain, ensure there's at least one other captain (domain invariant)
        if target.role == TeamRole::Captain && cmd.new_role != TeamRole::Captain {
            let captain_count = self.member_repo.count_captains(team_id).await?;
            if captain_count <= 1 {
                return Err(DomainError::TeamMustHaveCaptain);
            }
        }

        let updated = self
            .member_repo
            .update_role(team_id, cmd.player_id, cmd.new_role)
            .await?;

        info!(
            team_id = %team_id,
            target_id = %cmd.player_id,
            new_role = %cmd.new_role,
            "Member role updated (authorized)"
        );

        Ok(updated)
    }

    /// Remove a member from a team (authorized version - caller must verify RBAC permissions).
    ///
    /// This method skips internal permission checks and should only be called
    /// after the API layer has verified the user has the required RBAC permissions
    /// (team.roster.manage scoped to this team).
    ///
    /// Domain invariants are still enforced:
    /// - Cannot remove founders
    /// - Team must have at least one captain
    #[instrument(skip(self))]
    pub async fn remove_member_authorized(
        &self,
        team_id: TeamId,
        target_id: PlayerId,
    ) -> Result<(), DomainError> {
        let target = self
            .member_repo
            .find_member(team_id, target_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        // Cannot remove the founder (domain invariant)
        if target.is_founder {
            return Err(DomainError::CannotRemoveFounder);
        }

        // If removing a captain, ensure there's at least one other captain (domain invariant)
        if target.role == TeamRole::Captain {
            let captain_count = self.member_repo.count_captains(team_id).await?;
            if captain_count <= 1 {
                return Err(DomainError::TeamMustHaveCaptain);
            }
        }

        self.member_repo.remove_member(team_id, target_id).await?;

        info!(
            team_id = %team_id,
            removed_id = %target_id,
            "Member removed from team (authorized)"
        );

        Ok(())
    }

    /// Leave a team voluntarily.
    #[instrument(skip(self))]
    pub async fn leave_team(&self, team_id: TeamId, player_id: PlayerId) -> Result<(), DomainError> {
        let member = self
            .member_repo
            .find_member(team_id, player_id)
            .await?
            .ok_or(DomainError::NotTeamMember)?;

        // Founders cannot leave - they must transfer ownership or disband
        if member.is_founder {
            return Err(DomainError::not_authorized(
                "founders cannot leave the team; transfer ownership or disband instead",
            ));
        }

        // If leaving as the last captain, must promote someone else first
        if member.role == TeamRole::Captain {
            let captain_count = self.member_repo.count_captains(team_id).await?;
            if captain_count <= 1 {
                return Err(DomainError::TeamMustHaveCaptain);
            }
        }

        self.member_repo.remove_member(team_id, player_id).await?;

        info!(team_id = %team_id, player_id = %player_id, "Player left team");

        Ok(())
    }

    /// Get teams for a player.
    #[instrument(skip(self))]
    pub async fn get_player_teams(&self, player_id: PlayerId) -> Result<Vec<Team>, DomainError> {
        self.team_repo.list_by_player(player_id).await
    }

    /// List teams with optional search and pagination.
    #[instrument(skip(self))]
    pub async fn list_teams(
        &self,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Team>, i64), DomainError> {
        self.team_repo.list(search, limit, offset).await
    }

    /// Check if a player is a member of a team.
    pub async fn is_member(&self, team_id: TeamId, player_id: PlayerId) -> Result<bool, DomainError> {
        self.member_repo.is_member(team_id, player_id).await
    }

    /// Check if a player is a captain of a team.
    pub async fn is_captain(&self, team_id: TeamId, player_id: PlayerId) -> Result<bool, DomainError> {
        self.member_repo.is_captain(team_id, player_id).await
    }
}

impl<TR, TMR, PR> Clone for TeamService<TR, TMR, PR>
where
    TR: TeamRepository,
    TMR: TeamMemberRepository,
    PR: PlayerRepository,
{
    fn clone(&self) -> Self {
        Self {
            team_repo: Arc::clone(&self.team_repo),
            member_repo: Arc::clone(&self.member_repo),
            player_repo: Arc::clone(&self.player_repo),
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests would use mockall to mock the repositories
    // Example structure:
    //
    // use mockall::predicate::*;
    // use super::*;
    //
    // mock! {
    //     TeamRepo {}
    //     #[async_trait]
    //     impl TeamRepository for TeamRepo {
    //         async fn find_by_id(&self, id: TeamId) -> Result<Option<Team>, DomainError>;
    //         // ... other methods
    //     }
    // }
    //
    // #[tokio::test]
    // async fn test_create_team() {
    //     let mut team_repo = MockTeamRepo::new();
    //     team_repo.expect_name_exists().returning(|_| Ok(false));
    //     // ... setup and test
    // }
}
