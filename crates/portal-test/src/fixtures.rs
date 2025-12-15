//! Pre-composed test fixtures for common scenarios.
//!
//! These fixtures replace lengthy setup functions that were duplicated across multiple
//! test files. Each fixture creates a complete, ready-to-use test scenario.
//!
//! ## Example
//!
//! ```ignore
//! use portal_test::fixtures::TwoTeamMatchFixture;
//!
//! let fixture = TwoTeamMatchFixture::new(&pool, jwt_secret).await;
//!
//! // Access team A's captain token
//! let response = client.get(&format!("/v1/veto/{}/session", fixture.match_id))
//!     .header("Authorization", format!("Bearer {}", fixture.team_a.captain.token))
//!     .send()
//!     .await;
//! ```

use crate::builders::{
    LeagueBuilder, LeagueSeasonBuilder, LeagueTeamBuilder, LeagueTeamMemberBuilder,
    LeagueTeamSeasonBuilder, TournamentBracketBuilder, TournamentBuilder, TournamentMatchBuilder,
    TournamentRegistrationBuilder, TournamentStageBuilder, UserBuilder, VetoSessionBuilder,
};
use crate::helpers::{assign_role_to_user, create_test_token};
use portal_db::DbPool;
use uuid::Uuid;

/// User fixture with both user and player IDs plus authentication token.
#[derive(Debug, Clone)]
pub struct UserFixture {
    pub user_id: Uuid,
    pub player_id: Uuid,
    pub token: String,
}

impl UserFixture {
    /// Create a new user fixture from builder output.
    fn new(user_id: Uuid, player_id: Uuid, username: &str, jwt_secret: &str) -> Self {
        Self {
            user_id,
            player_id,
            token: create_test_token(user_id, player_id, username, jwt_secret),
        }
    }
}

/// Team fixture with team info and member credentials.
#[derive(Debug, Clone)]
pub struct TeamFixture {
    /// Team ID.
    pub team_id: Uuid,
    /// Team season ID (for seasonal participation).
    pub team_season_id: Uuid,
    /// Team name.
    pub name: String,
    /// Team tag.
    pub tag: String,
    /// Team owner (also a captain).
    pub owner: UserFixture,
    /// Team captain (distinct from owner).
    pub captain: UserFixture,
    /// Regular team member (player role).
    pub member: UserFixture,
}

/// Collection of auth tokens for common test roles.
#[derive(Debug, Clone)]
pub struct FixtureTokens {
    /// Admin token with super_admin role.
    pub admin: String,
    /// Spectator token (no team membership).
    pub spectator: String,
    /// Admin user info.
    pub admin_user: UserFixture,
    /// Spectator user info.
    pub spectator_user: UserFixture,
}

/// Complete two-team match scenario.
///
/// This fixture creates:
/// - A league with a season
/// - Two teams (Alpha and Beta) with owners, captains, and members
/// - A tournament with stage and bracket
/// - Tournament registrations for both teams
/// - A match between the two teams
/// - Optionally, a veto session for the match
///
/// This replaces ~260 lines of setup code that was duplicated in veto_test.rs,
/// veto_ws_test.rs, and similar files.
#[derive(Debug, Clone)]
pub struct TwoTeamMatchFixture {
    // League/Season
    pub league_id: Uuid,
    pub season_id: Uuid,

    // Teams
    pub team_a: TeamFixture,
    pub team_b: TeamFixture,

    // Tournament infrastructure
    pub tournament_id: Uuid,
    pub stage_id: Uuid,
    pub bracket_id: Uuid,
    pub match_id: Uuid,

    // Registrations
    pub reg_a_id: Uuid,
    pub reg_b_id: Uuid,

    // Optional veto session
    pub veto_session_id: Option<Uuid>,

    // Common auth tokens
    pub tokens: FixtureTokens,
}

impl TwoTeamMatchFixture {
    /// Create a new two-team match fixture without a veto session.
    pub async fn new(pool: &DbPool, jwt_secret: &str) -> Self {
        Self::build(pool, jwt_secret, false).await
    }

    /// Create a new two-team match fixture with a veto session.
    pub async fn with_veto(pool: &DbPool, jwt_secret: &str) -> Self {
        Self::build(pool, jwt_secret, true).await
    }

    async fn build(pool: &DbPool, jwt_secret: &str, with_veto: bool) -> Self {
        // Create league
        let league = LeagueBuilder::new()
            .name("Fixture Test League")
            .build_persisted(pool)
            .await;

        // Create season
        let season = LeagueSeasonBuilder::new()
            .league_id(league.id)
            .name("Fixture Test Season")
            .registration()
            .build_persisted(pool)
            .await;

        // Create Team A users
        let team_a_owner = UserBuilder::new()
            .username("fixture_team_a_owner")
            .build_persisted(pool)
            .await;
        let team_a_captain = UserBuilder::new()
            .username("fixture_team_a_captain")
            .build_persisted(pool)
            .await;
        let team_a_member = UserBuilder::new()
            .username("fixture_team_a_member")
            .build_persisted(pool)
            .await;

        // Create Team B users
        let team_b_owner = UserBuilder::new()
            .username("fixture_team_b_owner")
            .build_persisted(pool)
            .await;
        let team_b_captain = UserBuilder::new()
            .username("fixture_team_b_captain")
            .build_persisted(pool)
            .await;
        let team_b_member = UserBuilder::new()
            .username("fixture_team_b_member")
            .build_persisted(pool)
            .await;

        // Create admin and spectator
        let admin = UserBuilder::new()
            .username("fixture_admin")
            .build_persisted(pool)
            .await;
        let spectator = UserBuilder::new()
            .username("fixture_spectator")
            .build_persisted(pool)
            .await;

        // Grant admin role
        assign_role_to_user(pool, admin.id, "super_admin").await;

        // Create Team A
        let team_a = LeagueTeamBuilder::new()
            .name("Team Alpha")
            .tag("ALPHA")
            .league_id(league.id)
            .owner(team_a_owner.id)
            .build_persisted(pool)
            .await;

        // Register Team A for season
        let team_a_season = LeagueTeamSeasonBuilder::new()
            .team_id(team_a.id)
            .season_id(season.id)
            .build_persisted(pool)
            .await;

        // Add Team A members
        LeagueTeamMemberBuilder::new()
            .team_season_id(team_a_season.id)
            .player_id(team_a_captain.id)
            .captain()
            .build_persisted(pool)
            .await;
        LeagueTeamMemberBuilder::new()
            .team_season_id(team_a_season.id)
            .player_id(team_a_member.id)
            .player()
            .build_persisted(pool)
            .await;

        // Create Team B
        let team_b = LeagueTeamBuilder::new()
            .name("Team Beta")
            .tag("BETA")
            .league_id(league.id)
            .owner(team_b_owner.id)
            .build_persisted(pool)
            .await;

        // Register Team B for season
        let team_b_season = LeagueTeamSeasonBuilder::new()
            .team_id(team_b.id)
            .season_id(season.id)
            .build_persisted(pool)
            .await;

        // Add Team B members
        LeagueTeamMemberBuilder::new()
            .team_season_id(team_b_season.id)
            .player_id(team_b_captain.id)
            .captain()
            .build_persisted(pool)
            .await;
        LeagueTeamMemberBuilder::new()
            .team_season_id(team_b_season.id)
            .player_id(team_b_member.id)
            .player()
            .build_persisted(pool)
            .await;

        // Create tournament
        let tournament = TournamentBuilder::new()
            .name("Fixture Test Tournament")
            .league_id(league.id)
            .season_id(season.id)
            .in_progress()
            .build_persisted(pool)
            .await;

        // Create stage using builder
        let stage = TournamentStageBuilder::new()
            .tournament_id_from_uuid(tournament.id)
            .name("Main Bracket")
            .single_elimination()
            .build_persisted(pool)
            .await;

        // Create bracket using builder
        let bracket = TournamentBracketBuilder::new()
            .stage_id(stage.id)
            .tournament_id_from_uuid(tournament.id)
            .name("Main")
            .single_elimination()
            .total_rounds(1)
            .build_persisted(pool)
            .await;

        // Create tournament registrations using builder
        let reg_a = TournamentRegistrationBuilder::new()
            .tournament_id_from_uuid(tournament.id)
            .team_season_id_from_uuid(team_a_season.id)
            .participant_name("Team Alpha")
            .registered_by_uuid(team_a_captain.id)
            .build_persisted(pool)
            .await;

        let reg_b = TournamentRegistrationBuilder::new()
            .tournament_id_from_uuid(tournament.id)
            .team_season_id_from_uuid(team_b_season.id)
            .participant_name("Team Beta")
            .registered_by_uuid(team_b_captain.id)
            .build_persisted(pool)
            .await;

        // Create match using builder
        let match_ = TournamentMatchBuilder::new()
            .bracket_id(bracket.id)
            .stage_id(stage.id)
            .tournament_id_from_uuid(tournament.id)
            .round(1)
            .match_number(1)
            .bracket_position("R1M1")
            .participant1(reg_a.id, "Team Alpha")
            .participant2(reg_b.id, "Team Beta")
            .bo3()
            .build_persisted(pool)
            .await;

        // Optionally create veto session
        let veto_session_id = if with_veto {
            let session = VetoSessionBuilder::new()
                .match_id(match_.id)
                .bo3()
                .build_persisted(pool)
                .await;

            // Update session to be in progress with team A going first
            sqlx::query(
                r"UPDATE veto_sessions SET
                    first_action_registration_id = $1,
                    current_team_turn = $1,
                    status = 'in_progress',
                    current_action_number = 1,
                    started_at = NOW()
                 WHERE id = $2",
            )
            .bind(reg_a.id.as_uuid())
            .bind(session.id.as_uuid())
            .execute(pool)
            .await
            .expect("Failed to update veto session");

            Some(session.id.as_uuid())
        } else {
            None
        };

        // Create user fixtures with tokens
        let team_a_fixture = TeamFixture {
            team_id: team_a.id,
            team_season_id: team_a_season.id,
            name: "Team Alpha".to_string(),
            tag: "ALPHA".to_string(),
            owner: UserFixture::new(
                team_a_owner.id,
                team_a_owner.id,
                "fixture_team_a_owner",
                jwt_secret,
            ),
            captain: UserFixture::new(
                team_a_captain.id,
                team_a_captain.id,
                "fixture_team_a_captain",
                jwt_secret,
            ),
            member: UserFixture::new(
                team_a_member.id,
                team_a_member.id,
                "fixture_team_a_member",
                jwt_secret,
            ),
        };

        let team_b_fixture = TeamFixture {
            team_id: team_b.id,
            team_season_id: team_b_season.id,
            name: "Team Beta".to_string(),
            tag: "BETA".to_string(),
            owner: UserFixture::new(
                team_b_owner.id,
                team_b_owner.id,
                "fixture_team_b_owner",
                jwt_secret,
            ),
            captain: UserFixture::new(
                team_b_captain.id,
                team_b_captain.id,
                "fixture_team_b_captain",
                jwt_secret,
            ),
            member: UserFixture::new(
                team_b_member.id,
                team_b_member.id,
                "fixture_team_b_member",
                jwt_secret,
            ),
        };

        let admin_fixture = UserFixture::new(admin.id, admin.id, "fixture_admin", jwt_secret);
        let spectator_fixture =
            UserFixture::new(spectator.id, spectator.id, "fixture_spectator", jwt_secret);

        Self {
            league_id: league.id,
            season_id: season.id,
            team_a: team_a_fixture,
            team_b: team_b_fixture,
            tournament_id: tournament.id,
            stage_id: stage.id.as_uuid(),
            bracket_id: bracket.id.as_uuid(),
            match_id: match_.id.as_uuid(),
            reg_a_id: reg_a.id.as_uuid(),
            reg_b_id: reg_b.id.as_uuid(),
            veto_session_id,
            tokens: FixtureTokens {
                admin: admin_fixture.token.clone(),
                spectator: spectator_fixture.token.clone(),
                admin_user: admin_fixture,
                spectator_user: spectator_fixture,
            },
        }
    }

    /// Get Team A captain's token (convenience method).
    #[must_use]
    pub fn team_a_captain_token(&self) -> &str {
        &self.team_a.captain.token
    }

    /// Get Team B captain's token (convenience method).
    #[must_use]
    pub fn team_b_captain_token(&self) -> &str {
        &self.team_b.captain.token
    }

    /// Get admin token (convenience method).
    #[must_use]
    pub fn admin_token(&self) -> &str {
        &self.tokens.admin
    }

    /// Get spectator token (convenience method).
    #[must_use]
    pub fn spectator_token(&self) -> &str {
        &self.tokens.spectator
    }
}
