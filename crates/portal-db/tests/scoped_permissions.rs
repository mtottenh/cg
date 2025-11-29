//! Scoped permission tests (TDD - Test Driven Development).
//!
//! These tests are written FIRST, before the implementation.
//! They define the expected behavior of the scoped RBAC system.
//!
//! Run with: `cargo test -p portal-db --test scoped_permissions`

use portal_core::{PermissionScope, ScopeType, UserId};
use portal_db::entities::NewUserRole;
use portal_db::repositories::{PermissionRepository, RoleRepository};
use portal_db::DbPool;
use portal_test::database::TestDb;
use uuid::Uuid;

// ===========================================
// Test Helpers
// ===========================================

async fn create_test_user(pool: &DbPool, suffix: &str) -> Uuid {
    let user = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO users (username, email, password_hash)
        VALUES ($1, $2, 'hash')
        RETURNING id
        "#,
    )
    .bind(format!("scopeduser{}", suffix))
    .bind(format!("scoped{}@example.com", suffix))
    .fetch_one(pool)
    .await
    .expect("Failed to create test user");
    user.0
}

/// Get role ID by name from the seeded roles.
async fn get_role_id(pool: &DbPool, role_name: &str) -> Uuid {
    let role = sqlx::query_as::<_, (Uuid,)>("SELECT id FROM roles WHERE name = $1")
        .bind(role_name)
        .fetch_one(pool)
        .await
        .expect(&format!("Role '{}' should exist from migrations", role_name));
    role.0
}

// ===========================================
// Scoped Permission Tests
// ===========================================

/// Test that a scoped role only grants permission within that scope.
///
/// Given: User has team_captain role for team A
/// When: Checking permission for team A and team B
/// Then: Permission granted only for team A
#[tokio::test]
async fn test_scoped_permission_granted_in_correct_scope() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "scopedperm").await;
    let team_a_id = Uuid::now_v7();
    let team_b_id = Uuid::now_v7();

    // Get the team_captain role (seeded by migration 0016)
    let captain_role_id = get_role_id(&db.pool, "team_captain").await;

    // Assign team_captain role for team A
    let assignment = NewUserRole {
        user_id,
        role_id: captain_role_id,
        scope_type: Some("team".to_string()),
        scope_id: Some(team_a_id),
        granted_by: None,
        expires_at: None,
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // Check permission in team A scope - should be granted
    let team_a_scope = PermissionScope::team(team_a_id);
    let has_perm_a = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "team.settings.manage", Some(&team_a_scope))
        .await
        .unwrap();
    assert!(has_perm_a, "User should have team.settings.manage in team A");

    // Check permission in team B scope - should be denied
    let team_b_scope = PermissionScope::team(team_b_id);
    let has_perm_b = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "team.settings.manage", Some(&team_b_scope))
        .await
        .unwrap();
    assert!(!has_perm_b, "User should NOT have team.settings.manage in team B");
}

/// Test that a global role grants permission in any scope.
///
/// Given: User has super_admin role (global, no scope)
/// When: Checking permission in any team scope
/// Then: Permission is granted (global roles apply everywhere)
#[tokio::test]
async fn test_global_role_applies_to_all_scopes() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "globalscope").await;
    let any_team_id = Uuid::now_v7();

    // Get super_admin role (has admin.teams.manage_any permission)
    let admin_role_id = get_role_id(&db.pool, "super_admin").await;

    // Assign super_admin role globally (no scope)
    let assignment = NewUserRole {
        user_id,
        role_id: admin_role_id,
        scope_type: None, // Global
        scope_id: None,
        granted_by: None,
        expires_at: None,
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // Check permission in any team scope - should be granted due to global role
    let team_scope = PermissionScope::team(any_team_id);
    let has_admin_perm = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "admin.teams.manage_any", Some(&team_scope))
        .await
        .unwrap();
    assert!(has_admin_perm, "Global admin role should grant admin.teams.manage_any in any scope");
}

/// Test that revoking scoped roles removes permissions.
///
/// Given: User has team_captain role for team A
/// When: Revoking all roles in team A scope
/// Then: Permission is no longer granted
#[tokio::test]
async fn test_revoke_scoped_roles() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "revokescope").await;
    let team_id = Uuid::now_v7();

    // Get team_captain role
    let captain_role_id = get_role_id(&db.pool, "team_captain").await;

    // Assign role
    let assignment = NewUserRole {
        user_id,
        role_id: captain_role_id,
        scope_type: Some("team".to_string()),
        scope_id: Some(team_id),
        granted_by: None,
        expires_at: None,
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // Verify permission exists
    let scope = PermissionScope::team(team_id);
    let has_perm = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "team.settings.manage", Some(&scope))
        .await
        .unwrap();
    assert!(has_perm, "Permission should exist before revocation");

    // Revoke all roles in this scope
    let revoked_count = role_repo
        .revoke_scoped_roles(user_id, ScopeType::Team, team_id, None)
        .await
        .unwrap();
    assert_eq!(revoked_count, 1, "Should have revoked 1 role");

    // Verify permission is now denied
    let has_perm_after = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "team.settings.manage", Some(&scope))
        .await
        .unwrap();
    assert!(!has_perm_after, "Permission should be denied after revocation");
}

/// Test counting role holders in a scope.
///
/// Used for business rules like "team must have at least one captain".
#[tokio::test]
async fn test_count_scoped_role_holders() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());

    let team_id = Uuid::now_v7();

    // Create two users and make them both captains of the same team
    let user1_id = create_test_user(&db.pool, "captain1").await;
    let user2_id = create_test_user(&db.pool, "captain2").await;

    let captain_role_id = get_role_id(&db.pool, "team_captain").await;

    for user_id in [user1_id, user2_id] {
        let assignment = NewUserRole {
            user_id,
            role_id: captain_role_id,
            scope_type: Some("team".to_string()),
            scope_id: Some(team_id),
            granted_by: None,
            expires_at: None,
        };
        role_repo.assign_to_user(assignment).await.unwrap();
    }

    // Count captains in this team
    let count = role_repo
        .count_scoped_role_holders("team_captain", ScopeType::Team, team_id)
        .await
        .unwrap();
    assert_eq!(count, 2, "Should have 2 captains");

    // Different team should have 0 captains
    let other_team_id = Uuid::now_v7();
    let other_count = role_repo
        .count_scoped_role_holders("team_captain", ScopeType::Team, other_team_id)
        .await
        .unwrap();
    assert_eq!(other_count, 0, "Other team should have 0 captains");
}

/// Test assigning a scoped role by name.
///
/// Convenience method for assigning roles without looking up role ID.
#[tokio::test]
async fn test_assign_scoped_role_by_name() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "assignbyname").await;
    let team_id = Uuid::now_v7();

    // Assign role by name
    let user_role = role_repo
        .assign_scoped_role(user_id, "team_player", ScopeType::Team, team_id, None)
        .await
        .unwrap();

    assert_eq!(user_role.user_id, user_id);
    assert_eq!(user_role.scope_type, Some("team".to_string()));
    assert_eq!(user_role.scope_id, Some(team_id));

    // Verify permission is granted
    let scope = PermissionScope::team(team_id);
    let has_perm = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "team.matches.play", Some(&scope))
        .await
        .unwrap();
    assert!(has_perm, "team_player should have team.matches.play permission");
}

/// Test that different team roles have different permissions.
///
/// Verifies the role-permission hierarchy is correct.
#[tokio::test]
async fn test_team_role_permission_hierarchy() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let team_id = Uuid::now_v7();
    let scope = PermissionScope::team(team_id);

    // Test captain has all permissions
    let captain_id = create_test_user(&db.pool, "hier_captain").await;
    role_repo
        .assign_scoped_role(captain_id, "team_captain", ScopeType::Team, team_id, None)
        .await
        .unwrap();

    // Test officer has roster and play, but not settings or delete
    let officer_id = create_test_user(&db.pool, "hier_officer").await;
    role_repo
        .assign_scoped_role(officer_id, "team_officer", ScopeType::Team, team_id, None)
        .await
        .unwrap();

    // Test player has only play
    let player_id = create_test_user(&db.pool, "hier_player").await;
    role_repo
        .assign_scoped_role(player_id, "team_player", ScopeType::Team, team_id, None)
        .await
        .unwrap();

    // Captain should have all team permissions
    for perm in ["team.roster.manage", "team.settings.manage", "team.roles.manage", "team.matches.play", "team.delete"] {
        let has = perm_repo
            .user_has_scoped_permission(UserId::from(captain_id), perm, Some(&scope))
            .await
            .unwrap();
        assert!(has, "Captain should have {}", perm);
    }

    // Officer should have roster and play, but not settings, roles, or delete
    let has_roster = perm_repo
        .user_has_scoped_permission(UserId::from(officer_id), "team.roster.manage", Some(&scope))
        .await
        .unwrap();
    assert!(has_roster, "Officer should have team.roster.manage");

    let has_play = perm_repo
        .user_has_scoped_permission(UserId::from(officer_id), "team.matches.play", Some(&scope))
        .await
        .unwrap();
    assert!(has_play, "Officer should have team.matches.play");

    let has_settings = perm_repo
        .user_has_scoped_permission(UserId::from(officer_id), "team.settings.manage", Some(&scope))
        .await
        .unwrap();
    assert!(!has_settings, "Officer should NOT have team.settings.manage");

    // Player should only have play
    let player_has_play = perm_repo
        .user_has_scoped_permission(UserId::from(player_id), "team.matches.play", Some(&scope))
        .await
        .unwrap();
    assert!(player_has_play, "Player should have team.matches.play");

    let player_has_roster = perm_repo
        .user_has_scoped_permission(UserId::from(player_id), "team.roster.manage", Some(&scope))
        .await
        .unwrap();
    assert!(!player_has_roster, "Player should NOT have team.roster.manage");
}

/// Test getting all permissions for a user in a specific scope.
#[tokio::test]
async fn test_get_user_permissions_in_scope() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "permsscope").await;
    let team_id = Uuid::now_v7();

    // Assign officer role
    role_repo
        .assign_scoped_role(user_id, "team_officer", ScopeType::Team, team_id, None)
        .await
        .unwrap();

    // Get permissions in team scope
    let scope = PermissionScope::team(team_id);
    let permissions = perm_repo
        .get_user_permissions_in_scope(UserId::from(user_id), Some(&scope))
        .await
        .unwrap();

    // Officer should have team.roster.manage, team.matches.play, team.view.internal
    let perm_names: Vec<_> = permissions.iter().map(|p| p.name.as_str()).collect();
    assert!(perm_names.contains(&"team.roster.manage"), "Should have roster permission");
    assert!(perm_names.contains(&"team.matches.play"), "Should have play permission");
    assert!(!perm_names.contains(&"team.settings.manage"), "Should NOT have settings permission");
}

/// Test that permission check without scope uses global permissions only.
#[tokio::test]
async fn test_permission_without_scope_checks_global_only() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "noscopeperm").await;
    let team_id = Uuid::now_v7();

    // Assign scoped role (team captain for specific team)
    role_repo
        .assign_scoped_role(user_id, "team_captain", ScopeType::Team, team_id, None)
        .await
        .unwrap();

    // Checking without scope should NOT find the scoped permission
    // (scoped permissions require a scope context)
    let has_perm_global = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "team.settings.manage", None)
        .await
        .unwrap();
    assert!(!has_perm_global, "Scoped permission should NOT be found without scope context");

    // But with scope, it should work
    let scope = PermissionScope::team(team_id);
    let has_perm_scoped = perm_repo
        .user_has_scoped_permission(UserId::from(user_id), "team.settings.manage", Some(&scope))
        .await
        .unwrap();
    assert!(has_perm_scoped, "Permission should be found with scope context");
}
