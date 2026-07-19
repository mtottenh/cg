//! League team repository traits.
//!
//! These repositories handle league-scoped teams with seasonal rosters.
//!
//! Architecture:
//!   - `LeagueTeam`: Persistent team identity (belongs to league)
//!   - `LeagueTeamSeason`: Team's participation in a specific season
//!   - `LeagueTeamMember`: Roster for a team-season
//!   - `LeagueTeamInvitation`: Invites/requests to join a team-season roster
//!   - `LeagueSeasonParticipant`: Individual players in individual-format leagues
//!
//! Note on `UserId` vs `PlayerId`:
//! - `PlayerId` is used for player-related operations (team membership, invitations)
//! - `UserId` is used for admin/audit fields (`created_by`, `added_by`, `invited_by`, `locked_by`)

use crate::entities::league_team::{
    LeagueSeason, LeagueSeasonParticipant, LeagueTeam, LeagueTeamInvitation,
    LeagueTeamInvitationWithTeam, LeagueTeamMember, LeagueTeamMemberWithPlayer, LeagueTeamSeason,
    LeagueTeamSummary, PlayerLeagueTeamMembership,
};
use async_trait::async_trait;
use portal_core::types::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamSeasonStatus, LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
use portal_core::{
    DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamInvitationId,
    LeagueTeamMemberId, LeagueTeamSeasonId, PlayerId, UserId,
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

    /// Count teams registered in a season.
    async fn count_teams(&self, season_id: LeagueSeasonId) -> Result<i64, DomainError>;

    /// Count individual participants in a season (for individual format).
    async fn count_participants(&self, season_id: LeagueSeasonId) -> Result<i64, DomainError>;
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
    pub team_size_min: Option<i32>,
    pub team_size_max: Option<i32>,
    pub max_substitutes: Option<i32>,
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
// LEAGUE TEAM REPOSITORY (Persistent Identity)
// =============================================================================

/// Repository trait for league team operations.
///
/// Teams have persistent identity at the league level (not season level).
/// Seasonal participation is handled by `LeagueTeamSeasonRepository`.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueTeamRepository: Send + Sync {
    /// Find a team by ID.
    async fn find_by_id(&self, id: LeagueTeamId) -> Result<Option<LeagueTeam>, DomainError>;

    /// Find a team by league and name (case-insensitive).
    async fn find_by_name(
        &self,
        league_id: LeagueId,
        name: &str,
    ) -> Result<Option<LeagueTeam>, DomainError>;

    /// Find a team by league and tag (case-insensitive).
    async fn find_by_tag(
        &self,
        league_id: LeagueId,
        tag: &str,
    ) -> Result<Option<LeagueTeam>, DomainError>;

    /// Create a new team.
    async fn create(&self, team: CreateLeagueTeam) -> Result<LeagueTeam, DomainError>;

    /// Atomically create a team, register it for a season, and add the
    /// founding captain to the seasonal roster — all in a single database
    /// transaction.
    ///
    /// Before this existed, the service composed three separate repository
    /// calls, which ran on three different connections with no rollback on
    /// partial failure. A team insert that succeeded followed by a
    /// team_season insert that hit a constraint left an orphan row in
    /// `league_teams`. This method binds the three writes to one
    /// transaction so either everything commits or nothing does.
    ///
    /// The member is not returned because callers of
    /// `LeagueTeamService::create_team` only ever consumed the team and
    /// team_season; if you need the member, look it up after the call
    /// (it's keyed by `(team_season_id, captain_player_id)`).
    async fn create_team_with_season_and_captain(
        &self,
        team: CreateLeagueTeam,
        season_id: LeagueSeasonId,
        captain_player_id: PlayerId,
    ) -> Result<(LeagueTeam, LeagueTeamSeason), DomainError>;

    /// Update a team's profile.
    async fn update(
        &self,
        id: LeagueTeamId,
        update: UpdateLeagueTeam,
    ) -> Result<LeagueTeam, DomainError>;

    /// List teams in a league with optional filters and pagination.
    async fn list_by_league(
        &self,
        league_id: LeagueId,
        status_filter: Option<LeagueTeamStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeam>, i64), DomainError>;

    /// List all teams owned by a player in a league.
    async fn list_by_owner(
        &self,
        league_id: LeagueId,
        owner_player_id: PlayerId,
    ) -> Result<Vec<LeagueTeam>, DomainError>;

    /// Update team status.
    async fn update_status(
        &self,
        id: LeagueTeamId,
        status: LeagueTeamStatus,
    ) -> Result<LeagueTeam, DomainError>;

    /// Transfer team ownership.
    async fn transfer_ownership(
        &self,
        id: LeagueTeamId,
        new_owner_player_id: PlayerId,
    ) -> Result<LeagueTeam, DomainError>;

    /// Check if a name is taken in a league.
    async fn name_exists(&self, league_id: LeagueId, name: &str) -> Result<bool, DomainError>;

    /// Check if a tag is taken in a league.
    async fn tag_exists(&self, league_id: LeagueId, tag: &str) -> Result<bool, DomainError>;

    /// Delete a team (should rarely be used - prefer disbanding).
    async fn delete(&self, id: LeagueTeamId) -> Result<(), DomainError>;
}

/// Data for creating a new league team.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeam {
    pub league_id: LeagueId,
    pub name: String,
    pub tag: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub owner_player_id: PlayerId,
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
// LEAGUE TEAM SEASON REPOSITORY (Seasonal Participation)
// =============================================================================

/// Repository trait for team seasonal participation.
///
/// Tracks a team's registration and performance in a specific season.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueTeamSeasonRepository: Send + Sync {
    /// Find a team-season by ID.
    async fn find_by_id(
        &self,
        id: LeagueTeamSeasonId,
    ) -> Result<Option<LeagueTeamSeason>, DomainError>;

    /// Find a team's registration for a specific season.
    async fn find_by_team_and_season(
        &self,
        team_id: LeagueTeamId,
        season_id: LeagueSeasonId,
    ) -> Result<Option<LeagueTeamSeason>, DomainError>;

    /// Register a team for a season.
    async fn create(
        &self,
        registration: CreateLeagueTeamSeason,
    ) -> Result<LeagueTeamSeason, DomainError>;

    /// Register a team for a new season *and* seat the captain on the
    /// seasonal roster in a single transaction.
    ///
    /// Both writes commit or neither does. Splitting these into two
    /// separate calls (as [`Self::create`] + `member_repo.add_member`)
    /// left orphaned `league_team_seasons` rows with no roster captain
    /// on partial failure — see audit I5. Prefer this method from
    /// services; keep the non-atomic `create` for paths that don't need
    /// a captain (e.g. admin re-registration of a team whose captain
    /// isn't changing).
    async fn create_with_captain(
        &self,
        team_id: LeagueTeamId,
        season_id: LeagueSeasonId,
        captain_player_id: PlayerId,
    ) -> Result<LeagueTeamSeason, DomainError>;

    /// Update a team-season.
    async fn update(
        &self,
        id: LeagueTeamSeasonId,
        update: UpdateLeagueTeamSeason,
    ) -> Result<LeagueTeamSeason, DomainError>;

    /// List all team registrations for a season.
    async fn list_by_season(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<LeagueTeamSeasonStatus>,
        search: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeamSeason>, i64), DomainError>;

    /// List all seasons a team has participated in.
    async fn list_by_team(
        &self,
        team_id: LeagueTeamId,
    ) -> Result<Vec<LeagueTeamSeason>, DomainError>;

    /// Update team-season status.
    async fn update_status(
        &self,
        id: LeagueTeamSeasonId,
        status: LeagueTeamSeasonStatus,
    ) -> Result<LeagueTeamSeason, DomainError>;

    /// Get team summaries with member counts for a season.
    async fn list_summaries(
        &self,
        season_id: LeagueSeasonId,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeamSummary>, i64), DomainError>;

    /// Check if a team is registered for a season.
    async fn is_registered(
        &self,
        team_id: LeagueTeamId,
        season_id: LeagueSeasonId,
    ) -> Result<bool, DomainError>;
}

/// Data for registering a team for a season.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeamSeason {
    pub team_id: LeagueTeamId,
    pub season_id: LeagueSeasonId,
}

/// Data for updating a team-season.
#[derive(Debug, Clone, Default)]
pub struct UpdateLeagueTeamSeason {
    pub status: Option<LeagueTeamSeasonStatus>,
    pub registration_notes: Option<String>,
    pub seed: Option<i32>,
    pub rating: Option<i32>,
}

// =============================================================================
// LEAGUE TEAM MEMBER REPOSITORY (Seasonal Roster)
// =============================================================================

/// Repository trait for league team member operations.
///
/// Members belong to a team-season (roster is per-season).
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueTeamMemberRepository: Send + Sync {
    /// Find a member by ID.
    async fn find_by_id(
        &self,
        id: LeagueTeamMemberId,
    ) -> Result<Option<LeagueTeamMember>, DomainError>;

    /// Find a member by team-season and player.
    async fn find_member(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueTeamMember>, DomainError>;

    /// Add a member to a team-season roster.
    async fn add_member(
        &self,
        member: AddLeagueTeamMember,
    ) -> Result<LeagueTeamMember, DomainError>;

    /// Update a member's role.
    async fn update_role(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        new_role: LeagueTeamRole,
    ) -> Result<LeagueTeamMember, DomainError>;

    /// Update a member's status.
    async fn update_status(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        status: LeagueTeamMemberStatus,
    ) -> Result<LeagueTeamMember, DomainError>;

    /// Remove a member from a team-season (set `left_at` timestamp, status = left).
    async fn remove_member(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<(), DomainError>;

    /// List all active members of a team-season.
    async fn list_members(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamMember>, DomainError>;

    /// List all active members of a team-season with player details.
    async fn list_members_with_players(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamMemberWithPlayer>, DomainError>;

    /// Count members by role in a team-season.
    async fn count_by_role(
        &self,
        team_season_id: LeagueTeamSeasonId,
        role: LeagueTeamRole,
    ) -> Result<i64, DomainError>;

    /// Count all active members in a team-season.
    async fn count_active_members(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<i64, DomainError>;

    /// Count primary members (captain + players) in a team-season.
    async fn count_primary_members(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<i64, DomainError>;

    /// Count substitutes in a team-season.
    async fn count_substitutes(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<i64, DomainError>;

    /// Count captains in a team-season.
    async fn count_captains(&self, team_season_id: LeagueTeamSeasonId) -> Result<i64, DomainError>;

    /// Check if a player is a member of a team-season.
    async fn is_member(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError>;

    /// Check if a player is a captain of a team-season.
    async fn is_captain(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError>;

    /// Check if a player is already a primary member of another team in the same season.
    /// Returns the team-season ID if they are, None otherwise.
    async fn find_primary_team_in_season(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueTeamSeasonId>, DomainError>;

    /// List all team memberships for a player across all seasons.
    async fn list_memberships_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerLeagueTeamMembership>, DomainError>;

    /// List all team memberships for a player in a specific season.
    async fn list_memberships_in_season(
        &self,
        player_id: PlayerId,
        season_id: LeagueSeasonId,
    ) -> Result<Vec<PlayerLeagueTeamMembership>, DomainError>;
}

/// Data for adding a league team member.
#[derive(Debug, Clone)]
pub struct AddLeagueTeamMember {
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,
    pub role: LeagueTeamRole,
    pub position: Option<String>,
    pub jersey_number: Option<i32>,
    pub added_by: Option<UserId>,
}

// =============================================================================
// LEAGUE TEAM INVITATION REPOSITORY
// =============================================================================

/// Repository trait for league team invitation operations.
///
/// Invitations are for joining a team-season roster.
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

    /// Find all pending invitations for a team-season.
    async fn find_pending_by_team_season(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<LeagueTeamInvitation>, DomainError>;

    /// Find all pending invitations/requests for a player.
    async fn find_pending_for_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError>;

    /// Find all pending invitations for a player in a specific season.
    async fn find_pending_for_player_in_season(
        &self,
        player_id: PlayerId,
        season_id: LeagueSeasonId,
    ) -> Result<Vec<LeagueTeamInvitationWithTeam>, DomainError>;

    /// Check if there's an existing pending invitation for this player/team-season.
    async fn find_existing_pending(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueTeamInvitation>, DomainError>;

    /// Update invitation status.
    async fn update_status(
        &self,
        id: LeagueTeamInvitationId,
        status: LeagueTeamInvitationStatus,
        response_message: Option<String>,
    ) -> Result<LeagueTeamInvitation, DomainError>;

    /// Mark an invitation `Accepted` **and** insert the player onto the
    /// team-season roster as a single transaction.
    ///
    /// Replaces the two-call `update_status(Accepted) + add_member`
    /// pattern in `LeagueTeamInvitationService::accept_invitation`. If
    /// the member insert failed in that pattern, the invitation was
    /// already flipped to Accepted and the player was silently missing
    /// from the roster — they saw "invitation accepted" but were not on
    /// the team, and a retry failed with "invitation already used".
    /// See audit I5.
    async fn accept_and_add_member(
        &self,
        invitation_id: LeagueTeamInvitationId,
        member: AddLeagueTeamMember,
    ) -> Result<LeagueTeamMember, DomainError>;

    /// Cancel all pending invitations for a player on a specific team-season.
    async fn cancel_pending_for_player(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
    ) -> Result<(), DomainError>;

    /// Count pending invitations for a player.
    async fn count_pending_for_player(&self, player_id: PlayerId) -> Result<i64, DomainError>;

    /// Expire all invitations past their expiration date.
    async fn expire_old_invitations(&self) -> Result<i64, DomainError>;
}

/// Data for creating a new league team invitation.
#[derive(Debug, Clone)]
pub struct CreateLeagueTeamInvitation {
    pub team_season_id: LeagueTeamSeasonId,
    pub player_id: PlayerId,
    pub invitation_type: LeagueTeamInvitationType,
    pub role: LeagueTeamRole,
    pub message: Option<String>,
    pub invited_by: Option<UserId>,
}

// =============================================================================
// LEAGUE SEASON PARTICIPANT REPOSITORY (Individual Format)
// =============================================================================

/// Repository trait for individual format league participants.
///
/// Used for 1v1 tournaments and other formats where teams are not required.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait LeagueSeasonParticipantRepository: Send + Sync {
    /// Find a participant by ID.
    async fn find_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<LeagueSeasonParticipant>, DomainError>;

    /// Find a participant by season and player.
    async fn find_by_season_and_player(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<Option<LeagueSeasonParticipant>, DomainError>;

    /// Register a player for an individual format season.
    async fn register(
        &self,
        registration: RegisterLeagueSeasonParticipant,
    ) -> Result<LeagueSeasonParticipant, DomainError>;

    /// List all participants in a season.
    async fn list_by_season(
        &self,
        season_id: LeagueSeasonId,
        status_filter: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueSeasonParticipant>, i64), DomainError>;

    /// Update participant status.
    async fn update_status(
        &self,
        id: uuid::Uuid,
        status: String,
    ) -> Result<LeagueSeasonParticipant, DomainError>;

    /// Withdraw a participant.
    async fn withdraw(&self, id: uuid::Uuid) -> Result<LeagueSeasonParticipant, DomainError>;

    /// Check if a player is registered for a season.
    async fn is_registered(
        &self,
        season_id: LeagueSeasonId,
        player_id: PlayerId,
    ) -> Result<bool, DomainError>;
}

/// Data for registering an individual participant.
#[derive(Debug, Clone)]
pub struct RegisterLeagueSeasonParticipant {
    pub season_id: LeagueSeasonId,
    pub player_id: PlayerId,
}
