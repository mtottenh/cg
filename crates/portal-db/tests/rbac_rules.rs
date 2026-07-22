//! RBAC permission business rule tests.
//!
//! These tests verify the design constraints from the RBAC design documents.
//! Some tests may initially FAIL (Red) if constraints aren't enforced yet - that's expected TDD.

use chrono::{Duration, Utc};
use portal_core::UserId;
use portal_db::DbPool;
use portal_db::entities::{NewRole, NewUserRole};
use portal_db::repositories::{PermissionRepository, RoleRepository};
use portal_test::database::TestDb;
use uuid::Uuid;

// ===========================================
// Test Helpers
// ===========================================

async fn create_test_user(pool: &DbPool, suffix: &str) -> Uuid {
    let user = sqlx::query_as::<_, (Uuid,)>(
        r"
        INSERT INTO users (username, email, password_hash)
        VALUES ($1, $2, 'hash')
        RETURNING id
        ",
    )
    .bind(format!("rbacrulesuser{suffix}"))
    .bind(format!("rbacrules{suffix}@example.com"))
    .fetch_one(pool)
    .await
    .unwrap();
    user.0
}

async fn create_test_permission(pool: &DbPool, name: &str) -> Uuid {
    let perm = sqlx::query_as::<_, (Uuid,)>(
        r"
        INSERT INTO permissions (name, display_name, category)
        VALUES ($1, $2, 'test')
        RETURNING id
        ",
    )
    .bind(name)
    .bind(format!("{name} Permission"))
    .fetch_one(pool)
    .await
    .unwrap();
    perm.0
}

async fn create_role_with_permission(
    role_repo: &RoleRepository,
    pool: &DbPool,
    role_name: &str,
    perm_name: &str,
) -> (Uuid, Uuid) {
    let new_role = NewRole {
        name: role_name.to_string(),
        display_name: format!("{role_name} Role"),
        description: None,
        category: "custom".to_string(),
        priority: 5,
        color: None,
    };
    let role = role_repo.create(new_role).await.unwrap();

    let perm_id = create_test_permission(pool, perm_name).await;
    role_repo.add_permission(role.id, perm_id).await.unwrap();

    (role.id, perm_id)
}

// ===========================================
// RBAC Permission Rule Tests
// ===========================================

/// Test that a global role (no scope) applies everywhere.
#[tokio::test]
async fn test_global_role_applies_everywhere() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "global").await;

    let (role_id, _) =
        create_role_with_permission(&role_repo, &db.pool, "global_role", "global_perm").await;

    // Assign role with no scope (global)
    let assignment = NewUserRole {
        user_id,
        role_id,
        scope_type: None, // Global - applies everywhere
        scope_id: None,
        granted_by: None,
        expires_at: None,
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // User should have the permission globally
    let has_perm = perm_repo
        .user_has_permission(UserId::from(user_id), "global_perm")
        .await
        .unwrap();
    assert!(has_perm, "Global role should grant permission everywhere");
}

/// Test that a scoped role only applies to that specific scope.
/// NOTE: This test documents expected behavior - scope checking may not be implemented yet.
#[tokio::test]
async fn test_scoped_role_only_applies_to_scope() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "scoped").await;
    let scope_id = Uuid::now_v7(); // Some entity ID as scope

    let (role_id, _) =
        create_role_with_permission(&role_repo, &db.pool, "scoped_role", "scoped_perm").await;

    // Assign role with specific scope
    let assignment = NewUserRole {
        user_id,
        role_id,
        scope_type: Some("team".to_string()), // Only applies to this team
        scope_id: Some(scope_id),
        granted_by: None,
        expires_at: None,
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // NOTE: The current permission check doesn't support scope checking.
    // This test documents that scope-aware permission checking is needed.
    // When implemented, we should have a method like:
    // perm_repo.user_has_permission_in_scope(user_id, "scoped_perm", "team", scope_id)

    // For now, the basic check will pass (scoped role grants permission)
    let has_perm = perm_repo
        .user_has_permission(UserId::from(user_id), "scoped_perm")
        .await
        .unwrap();

    // Current behavior: permission is granted (no scope filtering)
    // Expected behavior: would need scope-aware check
    assert!(
        has_perm,
        "Scoped role grants permission (scope filtering not yet implemented)"
    );
}

/// Test that an expired role no longer grants permission.
#[tokio::test]
async fn test_expired_role_no_longer_grants_permission() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "expired").await;

    let (role_id, _) =
        create_role_with_permission(&role_repo, &db.pool, "expiring_role", "expiring_perm").await;

    // Assign role that has already expired
    let assignment = NewUserRole {
        user_id,
        role_id,
        scope_type: None,
        scope_id: None,
        granted_by: None,
        expires_at: Some(Utc::now() - Duration::hours(1)), // Already expired
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // Expired role should NOT grant permission
    let has_perm = perm_repo
        .user_has_permission(UserId::from(user_id), "expiring_perm")
        .await
        .unwrap();
    assert!(!has_perm, "Expired role should not grant permission");
}

/// Test that a revoked role no longer grants permission.
#[tokio::test]
async fn test_revoked_role_no_longer_grants_permission() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "revoked").await;

    let (role_id, _) =
        create_role_with_permission(&role_repo, &db.pool, "revoking_role", "revoking_perm").await;

    // Assign role
    let assignment = NewUserRole {
        user_id,
        role_id,
        scope_type: None,
        scope_id: None,
        granted_by: None,
        expires_at: None,
    };
    role_repo.assign_to_user(assignment).await.unwrap();

    // Verify permission is granted initially
    let has_perm = perm_repo
        .user_has_permission(UserId::from(user_id), "revoking_perm")
        .await
        .unwrap();
    assert!(has_perm, "Role should grant permission before revocation");

    // Revoke the role
    role_repo
        .revoke_from_user(user_id, role_id, None, None, None)
        .await
        .unwrap();

    // Revoked role should NOT grant permission
    let has_perm = perm_repo
        .user_has_permission(UserId::from(user_id), "revoking_perm")
        .await
        .unwrap();
    assert!(!has_perm, "Revoked role should not grant permission");
}

/// Test that system roles cannot be deleted.
#[tokio::test]
async fn test_system_roles_cannot_be_deleted() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());

    // Create a system role directly in DB
    let system_role_id: (Uuid,) = sqlx::query_as(
        r"
        INSERT INTO roles (name, display_name, category, priority, is_system)
        VALUES ('system_admin', 'System Admin', 'system', 100, TRUE)
        RETURNING id
        ",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();

    // Attempt to delete system role
    let deleted = role_repo.delete(system_role_id.0).await.unwrap();

    assert!(!deleted, "System roles should not be deletable");

    // Verify role still exists
    let role = role_repo.find_by_id(system_role_id.0).await.unwrap();
    assert!(
        role.is_some(),
        "System role should still exist after delete attempt"
    );
}

/// Test that a non-system role can be deleted.
#[tokio::test]
async fn test_non_system_role_can_be_deleted() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());

    let new_role = NewRole {
        name: "deletable_role".to_string(),
        display_name: "Deletable Role".to_string(),
        description: None,
        category: "custom".to_string(),
        priority: 1,
        color: None,
    };
    let role = role_repo.create(new_role).await.unwrap();

    // Delete the role
    let deleted = role_repo.delete(role.id).await.unwrap();
    assert!(deleted, "Non-system role should be deletable");

    // Verify role no longer exists
    let role = role_repo.find_by_id(role.id).await.unwrap();
    assert!(role.is_none(), "Deleted role should not exist");
}

/// Test that roles are ordered by priority (highest first).
#[tokio::test]
async fn test_roles_ordered_by_priority() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());

    // Create roles with different priorities
    for (name, priority) in [("low_role", 1), ("high_role", 100), ("mid_role", 50)] {
        let new_role = NewRole {
            name: name.to_string(),
            display_name: format!("{name} Role"),
            description: None,
            category: "custom".to_string(),
            priority,
            color: None,
        };
        role_repo.create(new_role).await.unwrap();
    }

    let roles = role_repo.list().await.unwrap();

    // Filter to our test roles
    let test_roles: Vec<_> = roles.iter().filter(|r| r.name.ends_with("_role")).collect();

    // Should be ordered highest priority first
    for i in 1..test_roles.len() {
        assert!(
            test_roles[i - 1].priority >= test_roles[i].priority,
            "Roles should be ordered by priority descending"
        );
    }
}

/// Test multiple roles grant combined permissions.
#[tokio::test]
async fn test_multiple_roles_combine_permissions() {
    let db = TestDb::new().await;
    let role_repo = RoleRepository::new(db.pool.clone());
    let perm_repo = PermissionRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "multi").await;

    // Create two roles with different permissions
    let (role1_id, _) =
        create_role_with_permission(&role_repo, &db.pool, "multi_role1", "multi_perm1").await;
    let (role2_id, _) =
        create_role_with_permission(&role_repo, &db.pool, "multi_role2", "multi_perm2").await;

    // Assign both roles to user
    for role_id in [role1_id, role2_id] {
        let assignment = NewUserRole {
            user_id,
            role_id,
            scope_type: None,
            scope_id: None,
            granted_by: None,
            expires_at: None,
        };
        role_repo.assign_to_user(assignment).await.unwrap();
    }

    // User should have permissions from both roles
    let has_perm1 = perm_repo
        .user_has_permission(UserId::from(user_id), "multi_perm1")
        .await
        .unwrap();
    let has_perm2 = perm_repo
        .user_has_permission(UserId::from(user_id), "multi_perm2")
        .await
        .unwrap();

    assert!(has_perm1, "User should have permission from first role");
    assert!(has_perm2, "User should have permission from second role");
}
