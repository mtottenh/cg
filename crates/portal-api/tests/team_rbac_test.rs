//! Team RBAC integration tests (TDD - Test Driven Development).
//!
//! These tests verify that team operations properly assign RBAC scoped roles.
//! Run with: `cargo test -p portal-api --test team_rbac_test --features test-utils`

mod common;

use axum::http::StatusCode;
use common::TestApp;
use portal_core::ScopeType;
use portal_db::repositories::{PermissionRepository, RoleRepository};
use portal_test::prelude::*;
use serde_json::json;

// ===========================================
// Helper Functions
// ===========================================

/// Generate a JWT token for a specific user.
fn generate_test_token(user_id: uuid::Uuid, player_id: uuid::Uuid, username: &str) -> String {
    use portal_domain::generate_access_token;
    generate_access_token(user_id, player_id, username, "test-jwt-secret")
        .expect("Failed to generate test token")
}

/// Check if a user has a specific scoped role.
async fn user_has_scoped_role(
    pool: &portal_db::DbPool,
    user_id: uuid::Uuid,
    role_name: &str,
    scope_type: ScopeType,
    scope_id: uuid::Uuid,
) -> bool {
    let result = sqlx::query_as::<_, (i64,)>(
        r#"
        SELECT COUNT(*) FROM user_roles ur
        JOIN roles r ON ur.role_id = r.id
        WHERE ur.user_id = $1
          AND r.name = $2
          AND ur.scope_type = $3
          AND ur.scope_id = $4
          AND ur.revoked_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(role_name)
    .bind(scope_type.as_str())
    .bind(scope_id)
    .fetch_one(pool)
    .await
    .expect("Query failed");

    result.0 > 0
}

/// Get all scoped roles for a user in a specific scope.
async fn get_user_scoped_roles(
    pool: &portal_db::DbPool,
    user_id: uuid::Uuid,
    scope_type: ScopeType,
    scope_id: uuid::Uuid,
) -> Vec<String> {
    let rows = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT r.name FROM user_roles ur
        JOIN roles r ON ur.role_id = r.id
        WHERE ur.user_id = $1
          AND ur.scope_type = $2
          AND ur.scope_id = $3
          AND ur.revoked_at IS NULL
        ORDER BY r.name
        "#,
    )
    .bind(user_id)
    .bind(scope_type.as_str())
    .bind(scope_id)
    .fetch_all(pool)
    .await
    .expect("Query failed");

    rows.into_iter().map(|r| r.0).collect()
}

// ===========================================
// Tests for Team Creation RBAC Role Assignment
// ===========================================

/// Test that creating a team assigns the team_captain role to the founder.
///
/// Given: A user creates a new team
/// When: The team is successfully created
/// Then: The founder should have the team_captain scoped role for that team
#[tokio::test]
async fn test_create_team_assigns_captain_role_to_founder() {
    let app = TestApp::new().await;

    // Create a user and player
    let user = UserBuilder::new()
        .username("team_creator")
        .email("creator@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Team Creator")
        .build_persisted(app.pool())
        .await;

    // Generate token for this user
    let token = generate_test_token(user.id, player.id, "team_creator");

    // Create a team via API
    let response = app
        .post_json_with_token(
            "/v1/teams",
            &json!({
                "name": "RBAC Test Team",
                "tag": "RTT"
            }),
            &token,
        )
        .await;

    // Should succeed
    assert!(
        response.status.is_success(),
        "Team creation should succeed. Status: {}, Body: {}",
        response.status,
        response.text()
    );

    // Parse the response to get the team ID
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let team_id: uuid::Uuid = body["data"]["id"]
        .as_str()
        .expect("Team ID should be present")
        .parse()
        .expect("Team ID should be valid UUID");

    // Verify the founder has the team_captain scoped role
    let has_captain_role =
        user_has_scoped_role(app.pool(), user.id, "team_captain", ScopeType::Team, team_id).await;

    assert!(
        has_captain_role,
        "Team founder should have team_captain role for the created team"
    );
}

/// Test that creating a team also grants the founder the team.settings.manage permission.
///
/// Given: A user creates a new team
/// When: The team is successfully created
/// Then: The founder should be able to update team settings (has team.settings.manage permission)
#[tokio::test]
async fn test_founder_can_update_team_after_creation() {
    let app = TestApp::new().await;

    // Create a user and player
    let user = UserBuilder::new()
        .username("founder_test")
        .email("founder_test@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Founder Test")
        .build_persisted(app.pool())
        .await;

    // Generate token
    let token = generate_test_token(user.id, player.id, "founder_test");

    // Create a team
    let create_response = app
        .post_json_with_token(
            "/v1/teams",
            &json!({
                "name": "Founder Permission Test",
                "tag": "FPT"
            }),
            &token,
        )
        .await;

    assert!(create_response.status.is_success());

    let body: serde_json::Value = serde_json::from_slice(&create_response.body).unwrap();
    let team_id = body["data"]["id"].as_str().unwrap();

    // Now try to update the team - should succeed because founder has team_captain role
    let update_response = app
        .patch_json_with_token(
            &format!("/v1/teams/{}", team_id),
            &json!({
                "name": "Updated Founder Team"
            }),
            &token,
        )
        .await;

    assert!(
        update_response.status.is_success(),
        "Founder should be able to update team. Status: {}, Body: {}",
        update_response.status,
        update_response.text()
    );

    // Verify the name was updated
    let updated_body: serde_json::Value = serde_json::from_slice(&update_response.body).unwrap();
    assert_eq!(
        updated_body["data"]["name"].as_str(),
        Some("Updated Founder Team")
    );
}

/// Test that a non-founder user cannot update a team they didn't create.
///
/// Given: User A creates a team, User B exists but is not a member
/// When: User B tries to update the team
/// Then: 403 Forbidden (no scoped role)
#[tokio::test]
async fn test_non_member_cannot_update_team() {
    let app = TestApp::new().await;

    // Create founder user and team
    let founder = UserBuilder::new()
        .username("actual_founder")
        .email("actual_founder@example.com")
        .build_persisted(app.pool())
        .await;

    let founder_player = PlayerBuilder::new()
        .user_id(founder.id)
        .display_name("Actual Founder")
        .build_persisted(app.pool())
        .await;

    let founder_token = generate_test_token(founder.id, founder_player.id, "actual_founder");

    // Create team
    let create_response = app
        .post_json_with_token(
            "/v1/teams",
            &json!({
                "name": "Founders Only Team",
                "tag": "FOT"
            }),
            &founder_token,
        )
        .await;

    assert!(create_response.status.is_success());
    let body: serde_json::Value = serde_json::from_slice(&create_response.body).unwrap();
    let team_id = body["data"]["id"].as_str().unwrap();

    // Create a different user (not a member)
    let outsider = UserBuilder::new()
        .username("team_outsider")
        .email("outsider@example.com")
        .build_persisted(app.pool())
        .await;

    let outsider_player = PlayerBuilder::new()
        .user_id(outsider.id)
        .display_name("Team Outsider")
        .build_persisted(app.pool())
        .await;

    let outsider_token = generate_test_token(outsider.id, outsider_player.id, "team_outsider");

    // Outsider tries to update the team - should fail
    let update_response = app
        .patch_json_with_token(
            &format!("/v1/teams/{}", team_id),
            &json!({
                "name": "Hacked Team Name"
            }),
            &outsider_token,
        )
        .await;

    assert_eq!(
        update_response.status,
        StatusCode::FORBIDDEN,
        "Non-member should not be able to update team. Body: {}",
        update_response.text()
    );
}

// ===========================================
// Tests for TeamRole to RBAC Role Mapping
// ===========================================

/// Test that TeamRole::Captain maps to team_captain RBAC role.
#[tokio::test]
async fn test_team_role_captain_maps_to_rbac_role() {
    let app = TestApp::new().await;

    // Create user
    let user = UserBuilder::new()
        .username("role_mapping_test")
        .email("role_mapping@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Role Mapping Test")
        .build_persisted(app.pool())
        .await;

    let token = generate_test_token(user.id, player.id, "role_mapping_test");

    // Create team (user becomes Captain via TeamRole)
    let response = app
        .post_json_with_token(
            "/v1/teams",
            &json!({
                "name": "Role Mapping Team",
                "tag": "RMT"
            }),
            &token,
        )
        .await;

    assert!(response.status.is_success());
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let team_id: uuid::Uuid = body["data"]["id"].as_str().unwrap().parse().unwrap();

    // Verify RBAC role assignment
    let roles = get_user_scoped_roles(app.pool(), user.id, ScopeType::Team, team_id).await;

    assert!(
        roles.contains(&"team_captain".to_string()),
        "Captain should have team_captain RBAC role. Actual roles: {:?}",
        roles
    );
}

// ===========================================
// Tests for Permission Checking via RBAC
// ===========================================

/// Test that the permission repository correctly checks scoped permissions after team creation.
#[tokio::test]
async fn test_permission_check_after_team_creation() {
    let app = TestApp::new().await;

    // Create user and team
    let user = UserBuilder::new()
        .username("perm_check_user")
        .email("perm_check@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Perm Check User")
        .build_persisted(app.pool())
        .await;

    let token = generate_test_token(user.id, player.id, "perm_check_user");

    // Create team
    let response = app
        .post_json_with_token(
            "/v1/teams",
            &json!({
                "name": "Permission Check Team",
                "tag": "PCT"
            }),
            &token,
        )
        .await;

    assert!(response.status.is_success());
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let team_id: uuid::Uuid = body["data"]["id"].as_str().unwrap().parse().unwrap();

    // Use PermissionRepository to verify the user has scoped permission
    let perm_repo = PermissionRepository::new(app.pool().clone());
    let scope = portal_core::PermissionScope {
        scope_type: ScopeType::Team,
        scope_id: team_id,
    };

    // Check team.settings.manage permission (team_captain should have this)
    let has_settings_permission = perm_repo
        .user_has_scoped_permission(
            portal_core::UserId::from(user.id),
            "team.settings.manage",
            Some(&scope),
        )
        .await
        .expect("Permission check should succeed");

    assert!(
        has_settings_permission,
        "Team founder (captain) should have team.settings.manage permission"
    );

    // Check team.roster.manage permission (team_captain should have this)
    let has_roster_permission = perm_repo
        .user_has_scoped_permission(
            portal_core::UserId::from(user.id),
            "team.roster.manage",
            Some(&scope),
        )
        .await
        .expect("Permission check should succeed");

    assert!(
        has_roster_permission,
        "Team founder (captain) should have team.roster.manage permission"
    );

    // Check team.delete permission (team_captain should have this)
    let has_delete_permission = perm_repo
        .user_has_scoped_permission(
            portal_core::UserId::from(user.id),
            "team.delete",
            Some(&scope),
        )
        .await
        .expect("Permission check should succeed");

    assert!(
        has_delete_permission,
        "Team founder (captain) should have team.delete permission"
    );
}

/// Test that a user without scoped role has no scoped permissions.
#[tokio::test]
async fn test_user_without_scoped_role_has_no_permissions() {
    let app = TestApp::new().await;

    // Create founder and team
    let founder = UserBuilder::new()
        .username("team_owner_2")
        .email("team_owner_2@example.com")
        .build_persisted(app.pool())
        .await;

    let founder_player = PlayerBuilder::new()
        .user_id(founder.id)
        .display_name("Team Owner 2")
        .build_persisted(app.pool())
        .await;

    let founder_token = generate_test_token(founder.id, founder_player.id, "team_owner_2");

    let response = app
        .post_json_with_token(
            "/v1/teams",
            &json!({
                "name": "Owner Only Team",
                "tag": "OOT"
            }),
            &founder_token,
        )
        .await;

    assert!(response.status.is_success());
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let team_id: uuid::Uuid = body["data"]["id"].as_str().unwrap().parse().unwrap();

    // Create a different user (no role in the team)
    let other_user = UserBuilder::new()
        .username("no_role_user")
        .email("no_role@example.com")
        .build_persisted(app.pool())
        .await;

    // Check that other_user has no permissions for this team
    let perm_repo = PermissionRepository::new(app.pool().clone());
    let scope = portal_core::PermissionScope {
        scope_type: ScopeType::Team,
        scope_id: team_id,
    };

    let has_settings_permission = perm_repo
        .user_has_scoped_permission(
            portal_core::UserId::from(other_user.id),
            "team.settings.manage",
            Some(&scope),
        )
        .await
        .expect("Permission check should succeed");

    assert!(
        !has_settings_permission,
        "User without scoped role should NOT have team.settings.manage permission"
    );
}
