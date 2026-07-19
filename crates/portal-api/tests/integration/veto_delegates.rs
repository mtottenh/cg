//! Veto delegate API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_db::{NewUserRole, PermissionRepository, RoleRepository};
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

// =============================================================================
// TEST DATA STRUCTURES
// =============================================================================

struct VetoTestSetup {
    league_id: Uuid,
    // Kept (underscore-prefixed) so the scenario still persists the season and
    // team B even though no test reads them back from the struct.
    _season_id: Uuid,
    team_a: TeamSetup,
    _team_b: TeamSetup,
}

struct TeamSetup {
    team_id: Uuid,
    team_season_id: Uuid,
    captain: UserSetup,
    owner: UserSetup,
    regular_member: UserSetup,
}

struct UserSetup {
    user_id: Uuid,
    player_id: Uuid,
    token: String,
}

// =============================================================================
// TEST HELPERS
// =============================================================================

/// Create a JWT token for a user.
fn create_token_for_user(user_id: Uuid) -> String {
    use portal_domain::generate_access_token;
    // User and player have the same ID per UserBuilder
    generate_access_token(user_id, user_id, "testuser", "test-jwt-secret")
        .expect("Failed to create token")
}

/// Grant tournament.manage permission to a user via the super_admin role.
///
/// Uses repository methods rather than raw SQL for maintainability.
async fn grant_tournament_admin_permission(app: &TestApp, user_id: Uuid) {
    let role_repo = RoleRepository::new(app.pool().clone());
    let permission_repo = PermissionRepository::new(app.pool().clone());

    // Find the super_admin role (seeded in migrations, has ALL permissions)
    let admin_role = role_repo
        .find_by_name("super_admin")
        .await
        .expect("Query should succeed")
        .expect("super_admin role should exist from migrations");

    // Assign admin role to user (admin role has tournament.manage permission)
    role_repo
        .assign_to_user(NewUserRole {
            user_id,
            role_id: admin_role.id,
            scope_type: None,
            scope_id: None,
            granted_by: None,
            expires_at: None,
        })
        .await
        .expect("Failed to assign admin role");

    // Verify the permission was granted correctly
    let has_permission = permission_repo
        .user_has_permission(portal_core::UserId::from(user_id), "tournament.manage")
        .await
        .expect("Failed to check permission");

    assert!(
        has_permission,
        "Failed to grant tournament.manage permission to user {user_id}"
    );
}

/// Create a user and return setup info.
async fn create_user(app: &TestApp, username: &str) -> UserSetup {
    let user = UserBuilder::new()
        .username(username)
        .build_persisted(app.pool())
        .await;

    UserSetup {
        user_id: user.id,
        player_id: user.id, // UserBuilder creates player with same ID
        token: create_token_for_user(user.id),
    }
}

/// Set up a complete test scenario with two teams.
async fn setup_test_scenario(app: &TestApp) -> VetoTestSetup {
    // Create league
    let league = LeagueBuilder::new()
        .name("Test League")
        .slug("test-league")
        .build_persisted(app.pool())
        .await;

    // Create season
    let season = LeagueSeasonBuilder::new()
        .league_id(league.id)
        .name("Test Season")
        .slug("test-season")
        .registration()
        .build_persisted(app.pool())
        .await;

    // Create Team A with captain, owner, and regular member
    let team_a_owner = create_user(app, "team_a_owner").await;
    let team_a_captain = create_user(app, "team_a_captain").await;
    let team_a_member = create_user(app, "team_a_member").await;

    let team_a = LeagueTeamBuilder::new()
        .name("Team Alpha")
        .tag("ALPHA")
        .league_id(league.id)
        .owner(team_a_owner.player_id)
        .build_persisted(app.pool())
        .await;

    let team_a_season = LeagueTeamSeasonBuilder::new()
        .team_id(team_a.id)
        .season_id(season.id)
        .build_persisted(app.pool())
        .await;

    // Add members to Team A
    LeagueTeamMemberBuilder::new()
        .team_season_id(team_a_season.id)
        .player_id(team_a_owner.player_id)
        .role("player")
        .build_persisted(app.pool())
        .await;

    LeagueTeamMemberBuilder::new()
        .team_season_id(team_a_season.id)
        .player_id(team_a_captain.player_id)
        .captain()
        .build_persisted(app.pool())
        .await;

    LeagueTeamMemberBuilder::new()
        .team_season_id(team_a_season.id)
        .player_id(team_a_member.player_id)
        .role("player")
        .build_persisted(app.pool())
        .await;

    // Create Team B with captain, owner, and regular member
    let team_b_owner = create_user(app, "team_b_owner").await;
    let team_b_captain = create_user(app, "team_b_captain").await;
    let team_b_member = create_user(app, "team_b_member").await;

    let team_b = LeagueTeamBuilder::new()
        .name("Team Beta")
        .tag("BETA")
        .league_id(league.id)
        .owner(team_b_owner.player_id)
        .build_persisted(app.pool())
        .await;

    let team_b_season = LeagueTeamSeasonBuilder::new()
        .team_id(team_b.id)
        .season_id(season.id)
        .build_persisted(app.pool())
        .await;

    // Add members to Team B
    LeagueTeamMemberBuilder::new()
        .team_season_id(team_b_season.id)
        .player_id(team_b_owner.player_id)
        .role("player")
        .build_persisted(app.pool())
        .await;

    LeagueTeamMemberBuilder::new()
        .team_season_id(team_b_season.id)
        .player_id(team_b_captain.player_id)
        .captain()
        .build_persisted(app.pool())
        .await;

    LeagueTeamMemberBuilder::new()
        .team_season_id(team_b_season.id)
        .player_id(team_b_member.player_id)
        .role("player")
        .build_persisted(app.pool())
        .await;

    VetoTestSetup {
        league_id: league.id,
        _season_id: season.id,
        team_a: TeamSetup {
            team_id: team_a.id,
            team_season_id: team_a_season.id,
            captain: team_a_captain,
            owner: team_a_owner,
            regular_member: team_a_member,
        },
        _team_b: TeamSetup {
            team_id: team_b.id,
            team_season_id: team_b_season.id,
            captain: team_b_captain,
            owner: team_b_owner,
            regular_member: team_b_member,
        },
    }
}

// =============================================================================
// DELEGATION CRUD TESTS
// =============================================================================

#[tokio::test]
async fn test_create_delegation_as_captain() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Captain creates delegation for regular member
    let response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.captain.token,
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["player_id"],
        setup.team_a.regular_member.player_id.to_string()
    );
    assert_eq!(body["data"]["delegated_by_role"], "captain");
    assert!(body["data"]["is_active"].as_bool().unwrap());
}

#[tokio::test]
async fn test_create_delegation_as_owner() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Owner creates delegation for regular member
    let response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.owner.token,
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["delegated_by_role"], "owner");
}

#[tokio::test]
async fn test_create_delegation_as_tournament_admin() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Create an admin user
    let admin = create_user(&app, "admin_user").await;
    grant_tournament_admin_permission(&app, admin.user_id).await;

    // Admin creates delegation for regular member
    let response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &admin.token,
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["delegated_by_role"], "tournament_admin");
}

#[tokio::test]
async fn test_create_delegation_unauthorized_regular_member() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Regular member tries to create delegation (should fail)
    let response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.captain.player_id.to_string()
            }),
            &setup.team_a.regular_member.token,
        )
        .await;

    // API returns 403 Forbidden for authorization failures (NotAuthorized error)
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_create_delegation_non_member() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Create a user who is not a member of the team
    let non_member = create_user(&app, "non_member").await;

    // Captain tries to delegate to non-member (should fail)
    let response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": non_member.player_id.to_string()
            }),
            &setup.team_a.captain.token,
        )
        .await;

    // Should fail because delegate must be a team member
    assert!(
        response.status == StatusCode::BAD_REQUEST || response.status == StatusCode::CONFLICT,
        "Expected BAD_REQUEST or CONFLICT, got {}: {}",
        response.status,
        response.text()
    );
}

#[tokio::test]
async fn test_create_delegation_duplicate() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Captain creates delegation
    let response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.captain.token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Try to create duplicate delegation (should fail)
    let response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.captain.token,
        )
        .await;

    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_list_delegations() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Create a delegation first
    let create_response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.captain.token,
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    // List delegations
    let response = app
        .get_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &setup.team_a.captain.token,
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let delegates = body["data"]["delegates"].as_array().unwrap();
    assert_eq!(delegates.len(), 1);
    assert_eq!(
        delegates[0]["player_id"],
        setup.team_a.regular_member.player_id.to_string()
    );
}

#[tokio::test]
async fn test_revoke_delegation_as_captain() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Create a delegation
    let create_response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.captain.token,
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let delegate_id = create_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Captain revokes delegation
    let response = app
        .delete_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates/{}",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id, delegate_id
            ),
            &setup.team_a.captain.token,
        )
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(!body["data"]["is_active"].as_bool().unwrap());
    assert!(body["data"]["revoked_at"].is_string());
}

#[tokio::test]
async fn test_revoke_delegation_hierarchy_owner_revokes_captain() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Captain creates a delegation
    let create_response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.captain.token,
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let delegate_id = create_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Owner revokes captain's delegation (should succeed - owner > captain)
    let response = app
        .delete_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates/{}",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id, delegate_id
            ),
            &setup.team_a.owner.token,
        )
        .await;

    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_revoke_delegation_unauthorized_captain_revokes_owner() {
    let app = TestApp::new().await;
    let setup = setup_test_scenario(&app).await;

    // Owner creates a delegation
    let create_response = app
        .post_json_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id
            ),
            &json!({
                "player_id": setup.team_a.regular_member.player_id.to_string()
            }),
            &setup.team_a.owner.token,
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let delegate_id = create_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Captain tries to revoke owner's delegation (should fail - captain < owner)
    let response = app
        .delete_with_token(
            &format!(
                "/v1/leagues/{}/teams/{}/seasons/{}/veto-delegates/{}",
                setup.league_id, setup.team_a.team_id, setup.team_a.team_season_id, delegate_id
            ),
            &setup.team_a.captain.token,
        )
        .await;

    // API returns 403 Forbidden for authorization failures (NotAuthorized error)
    response.assert_status(StatusCode::FORBIDDEN);
}
