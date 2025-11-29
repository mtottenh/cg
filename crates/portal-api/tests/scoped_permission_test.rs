//! Scoped permission extractor tests (TDD - Test Driven Development).
//!
//! These tests define the expected behavior of the RequireTeamPermission extractor.
//! Run with: `cargo test -p portal-api --test scoped_permission_test --features test-utils`

mod common;

use axum::http::StatusCode;
use common::TestApp;
use portal_core::ScopeType;
use portal_db::repositories::RoleRepository;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

// ===========================================
// Helper Functions
// ===========================================

/// Get role ID by name from the seeded roles.
async fn get_role_id(pool: &portal_db::DbPool, role_name: &str) -> Uuid {
    let role = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM roles WHERE name = $1")
        .bind(role_name)
        .fetch_one(pool)
        .await
        .expect(&format!("Role '{}' should exist from migrations", role_name));
    role.0
}

/// Assign a scoped role to a user.
async fn assign_team_role(pool: &portal_db::DbPool, user_id: Uuid, team_id: Uuid, role_name: &str) {
    let role_repo = RoleRepository::new(pool.clone());
    role_repo
        .assign_scoped_role(user_id, role_name, ScopeType::Team, team_id, None)
        .await
        .expect("Failed to assign role");
}

/// Generate a JWT token for a specific user.
/// This uses the test JWT secret and creates a valid token.
fn generate_test_token(user_id: Uuid, player_id: Uuid, username: &str) -> String {
    // For testing, we use portal_domain's token generation
    use portal_domain::generate_access_token;
    generate_access_token(user_id, player_id, username, "test-jwt-secret")
        .expect("Failed to generate test token")
}

// ===========================================
// Tests for RequireTeamPermission Extractor
// ===========================================

/// Test that a user with the correct scoped role can access a protected endpoint.
///
/// Given: User has team_captain role for team A
/// When: Accessing a team settings endpoint for team A
/// Then: Access is granted
#[tokio::test]
async fn test_team_permission_granted_with_scoped_role() {
    let app = TestApp::new().await;

    // Create a user and player
    let user = UserBuilder::new()
        .username("captain_user")
        .email("captain@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Captain User")
        .build_persisted(app.pool())
        .await;

    // Create a team
    let team = TeamBuilder::new()
        .name("Permission Test Team")
        .tag("PTT")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    // Assign team_captain role to user for this team
    assign_team_role(app.pool(), user.id, team.team.id, "team_captain").await;

    // Generate a token for this user
    let token = generate_test_token(user.id, player.id, "captain_user");

    // Access the protected endpoint (update team settings)
    // This endpoint should use RequireTeamPermission<{team::SETTINGS_MANAGE}>
    let response = app
        .patch_json_with_token(
            &format!("/v1/teams/{}", team.team.id),
            &json!({
                "name": "Updated Team Name"
            }),
            &token,
        )
        .await;

    // Should succeed (200 OK or other success status)
    assert!(
        response.status.is_success(),
        "User with team_captain role should be able to update team settings. Status: {}, Body: {}",
        response.status,
        response.text()
    );
}

/// Test that a user without the required permission gets 403 Forbidden.
///
/// Given: User has team_player role for team A (no settings.manage permission)
/// When: Accessing a team settings endpoint for team A
/// Then: Access is denied with 403 Forbidden
#[tokio::test]
async fn test_team_permission_denied_without_permission() {
    let app = TestApp::new().await;

    // Create a user and player
    let user = UserBuilder::new()
        .username("player_user")
        .email("player@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Player User")
        .build_persisted(app.pool())
        .await;

    // Create a captain user to create the team
    let captain = UserBuilder::new()
        .username("team_founder")
        .email("founder@example.com")
        .build_persisted(app.pool())
        .await;

    let captain_player = PlayerBuilder::new()
        .user_id(captain.id)
        .display_name("Founder")
        .build_persisted(app.pool())
        .await;

    // Create a team
    let team = TeamBuilder::new()
        .name("Permission Denied Test")
        .tag("PDT")
        .with_founder(captain_player.id)
        .build_persisted(app.pool())
        .await;

    // Assign only team_player role (not captain, no settings.manage permission)
    assign_team_role(app.pool(), user.id, team.team.id, "team_player").await;

    // Generate a token for the player user
    let token = generate_test_token(user.id, player.id, "player_user");

    // Try to update team settings
    let response = app
        .patch_json_with_token(
            &format!("/v1/teams/{}", team.team.id),
            &json!({
                "name": "Unauthorized Update"
            }),
            &token,
        )
        .await;

    // Should be forbidden
    response.assert_status(StatusCode::FORBIDDEN);
}

/// Test that a user with a scoped role for team A cannot access team B.
///
/// Given: User has team_captain role for team A
/// When: Accessing a team settings endpoint for team B
/// Then: Access is denied with 403 Forbidden
#[tokio::test]
async fn test_team_permission_denied_for_different_team() {
    let app = TestApp::new().await;

    // Create a user and player
    let user = UserBuilder::new()
        .username("other_captain")
        .email("other_captain@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Other Captain")
        .build_persisted(app.pool())
        .await;

    // Create two teams with different founders
    let founder1 = UserBuilder::new()
        .username("founder1")
        .email("founder1@example.com")
        .build_persisted(app.pool())
        .await;
    let founder1_player = PlayerBuilder::new()
        .user_id(founder1.id)
        .display_name("Founder 1")
        .build_persisted(app.pool())
        .await;

    let founder2 = UserBuilder::new()
        .username("founder2")
        .email("founder2@example.com")
        .build_persisted(app.pool())
        .await;
    let founder2_player = PlayerBuilder::new()
        .user_id(founder2.id)
        .display_name("Founder 2")
        .build_persisted(app.pool())
        .await;

    let team_a = TeamBuilder::new()
        .name("Team Alpha")
        .tag("TA")
        .with_founder(founder1_player.id)
        .build_persisted(app.pool())
        .await;

    let team_b = TeamBuilder::new()
        .name("Team Beta")
        .tag("TB")
        .with_founder(founder2_player.id)
        .build_persisted(app.pool())
        .await;

    // Assign team_captain role only for Team A
    assign_team_role(app.pool(), user.id, team_a.team.id, "team_captain").await;

    // Generate a token for this user
    let token = generate_test_token(user.id, player.id, "other_captain");

    // Try to update Team B settings (should fail - no role for Team B)
    let response = app
        .patch_json_with_token(
            &format!("/v1/teams/{}", team_b.team.id),
            &json!({
                "name": "Unauthorized Cross-Team Update"
            }),
            &token,
        )
        .await;

    // Should be forbidden
    response.assert_status(StatusCode::FORBIDDEN);
}

/// Test that a global admin can access any team.
///
/// Given: User has super_admin role (global, no scope)
/// When: Accessing any team settings endpoint
/// Then: Access is granted (admin override)
#[tokio::test]
async fn test_global_admin_can_access_any_team() {
    let app = TestApp::new().await;

    // Create a user and player for admin
    let admin_user = UserBuilder::new()
        .username("super_admin")
        .email("admin@example.com")
        .build_persisted(app.pool())
        .await;

    let admin_player = PlayerBuilder::new()
        .user_id(admin_user.id)
        .display_name("Super Admin")
        .build_persisted(app.pool())
        .await;

    // Assign super_admin role globally (no scope)
    let role_repo = RoleRepository::new(app.pool().clone());
    let admin_role_id = get_role_id(app.pool(), "super_admin").await;

    // Use the regular assign_to_user for global role
    let assignment = portal_db::entities::NewUserRole {
        user_id: admin_user.id,
        role_id: admin_role_id,
        scope_type: None, // Global
        scope_id: None,
        granted_by: None,
        expires_at: None,
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // Create a team owned by someone else
    let founder = UserBuilder::new()
        .username("team_owner")
        .email("owner@example.com")
        .build_persisted(app.pool())
        .await;
    let founder_player = PlayerBuilder::new()
        .user_id(founder.id)
        .display_name("Team Owner")
        .build_persisted(app.pool())
        .await;

    let team = TeamBuilder::new()
        .name("Admin Target Team")
        .tag("ATT")
        .with_founder(founder_player.id)
        .build_persisted(app.pool())
        .await;

    // Generate token for admin
    let token = generate_test_token(admin_user.id, admin_player.id, "super_admin");

    // Admin should be able to update any team
    let response = app
        .patch_json_with_token(
            &format!("/v1/teams/{}", team.team.id),
            &json!({
                "name": "Admin Updated Name"
            }),
            &token,
        )
        .await;

    // Should succeed
    assert!(
        response.status.is_success(),
        "Global admin should be able to update any team. Status: {}, Body: {}",
        response.status,
        response.text()
    );
}

/// Test that unauthenticated requests get 401 Unauthorized.
///
/// Given: No authentication token provided
/// When: Accessing a protected team endpoint
/// Then: 401 Unauthorized is returned
#[tokio::test]
async fn test_unauthenticated_request_rejected() {
    let app = TestApp::new().await;

    // Create a team using a founder
    let founder = UserBuilder::new()
        .username("anon_test_founder")
        .email("anon_founder@example.com")
        .build_persisted(app.pool())
        .await;
    let founder_player = PlayerBuilder::new()
        .user_id(founder.id)
        .display_name("Anon Test Founder")
        .build_persisted(app.pool())
        .await;

    let team = TeamBuilder::new()
        .name("Anonymous Test Team")
        .tag("ANT")
        .with_founder(founder_player.id)
        .build_persisted(app.pool())
        .await;

    // Try to update without authentication
    let response = app
        .patch_json_no_auth(
            &format!("/v1/teams/{}", team.team.id),
            &json!({
                "name": "Unauthorized Update"
            }),
        )
        .await;

    // Should be unauthorized
    response.assert_status(StatusCode::UNAUTHORIZED);
}
