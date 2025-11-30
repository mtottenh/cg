//! RBAC (Role-Based Access Control) repositories.

use crate::entities::{BanRow, NewBan, NewRole, NewUserRole, PermissionRow, RoleRow, UserRoleRow};
use crate::error::RepositoryError;
use crate::DbPool;
use portal_core::{PermissionScope, ScopeType, UserId};
use uuid::Uuid;

/// Repository for role operations.
#[derive(Clone)]
pub struct RoleRepository {
    pool: DbPool,
}

impl RoleRepository {
    /// Create a new role repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a role by ID.
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<RoleRow>, RepositoryError> {
        let role = sqlx::query_as::<_, RoleRow>("SELECT * FROM roles WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(role)
    }

    /// Find a role by name.
    pub async fn find_by_name(&self, name: &str) -> Result<Option<RoleRow>, RepositoryError> {
        let role = sqlx::query_as::<_, RoleRow>("SELECT * FROM roles WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        Ok(role)
    }

    /// Find a role by ID or name.
    pub async fn find_by_id_or_name(&self, id_or_name: &str) -> Result<Option<RoleRow>, RepositoryError> {
        let role = sqlx::query_as::<_, RoleRow>(
            "SELECT * FROM roles WHERE id::text = $1 OR name = $1",
        )
        .bind(id_or_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(role)
    }

    /// List all roles.
    pub async fn list(&self) -> Result<Vec<RoleRow>, RepositoryError> {
        let roles = sqlx::query_as::<_, RoleRow>(
            "SELECT * FROM roles ORDER BY priority DESC, name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(roles)
    }

    /// Create a new role.
    pub async fn create(&self, new_role: NewRole) -> Result<RoleRow, RepositoryError> {
        let role = sqlx::query_as::<_, RoleRow>(
            r"
            INSERT INTO roles (name, display_name, description, category, priority, color)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(&new_role.name)
        .bind(&new_role.display_name)
        .bind(&new_role.description)
        .bind(&new_role.category)
        .bind(new_role.priority)
        .bind(&new_role.color)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, &new_role.name))?;

        Ok(role)
    }

    /// Delete a role (only non-system roles).
    pub async fn delete(&self, id: Uuid) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "DELETE FROM roles WHERE id = $1 AND is_system = FALSE",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get permissions for a role.
    pub async fn get_permissions(&self, role_id: Uuid) -> Result<Vec<PermissionRow>, RepositoryError> {
        let permissions = sqlx::query_as::<_, PermissionRow>(
            r"
            SELECT p.* FROM permissions p
            JOIN role_permissions rp ON rp.permission_id = p.id
            WHERE rp.role_id = $1
            ORDER BY p.category, p.name
            ",
        )
        .bind(role_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(permissions)
    }

    /// Add a permission to a role.
    pub async fn add_permission(
        &self,
        role_id: Uuid,
        permission_id: Uuid,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r"
            INSERT INTO role_permissions (role_id, permission_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            ",
        )
        .bind(role_id)
        .bind(permission_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Remove a permission from a role.
    pub async fn remove_permission(
        &self,
        role_id: Uuid,
        permission_id: Uuid,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "DELETE FROM role_permissions WHERE role_id = $1 AND permission_id = $2",
        )
        .bind(role_id)
        .bind(permission_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Assign a role to a user.
    pub async fn assign_to_user(&self, assignment: NewUserRole) -> Result<UserRoleRow, RepositoryError> {
        let user_role = sqlx::query_as::<_, UserRoleRow>(
            r"
            INSERT INTO user_roles (user_id, role_id, scope_type, scope_id, granted_by, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(assignment.user_id)
        .bind(assignment.role_id)
        .bind(&assignment.scope_type)
        .bind(assignment.scope_id)
        .bind(assignment.granted_by)
        .bind(assignment.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "user role assignment"))?;

        Ok(user_role)
    }

    /// Revoke a role from a user.
    pub async fn revoke_from_user(
        &self,
        user_id: Uuid,
        role_id: Uuid,
        scope_type: Option<&str>,
        scope_id: Option<Uuid>,
        revoked_by: Option<Uuid>,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            r"
            UPDATE user_roles SET
                revoked_at = NOW(),
                revoked_by = $5
            WHERE user_id = $1
              AND role_id = $2
              AND (($3::text IS NULL AND scope_type IS NULL) OR scope_type = $3)
              AND (($4::uuid IS NULL AND scope_id IS NULL) OR scope_id = $4)
              AND revoked_at IS NULL
            ",
        )
        .bind(user_id)
        .bind(role_id)
        .bind(scope_type)
        .bind(scope_id)
        .bind(revoked_by)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get all roles for a user.
    pub async fn get_user_roles(&self, user_id: UserId) -> Result<Vec<RoleRow>, RepositoryError> {
        let roles = sqlx::query_as::<_, RoleRow>(
            r"
            SELECT r.* FROM roles r
            JOIN user_roles ur ON ur.role_id = r.id
            WHERE ur.user_id = $1
              AND ur.revoked_at IS NULL
              AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
            ORDER BY r.priority DESC, r.name
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(roles)
    }

    // =========================================
    // Scoped Role Methods
    // =========================================

    /// Assign a scoped role to a user by role name.
    ///
    /// This is a convenience method that looks up the role by name and assigns it
    /// with the specified scope. Used for team/league/tournament/match role assignment.
    ///
    /// If the user already has this role in this scope (and it's not revoked),
    /// returns the existing assignment.
    pub async fn assign_scoped_role(
        &self,
        user_id: Uuid,
        role_name: &str,
        scope_type: ScopeType,
        scope_id: Uuid,
        granted_by: Option<Uuid>,
    ) -> Result<UserRoleRow, RepositoryError> {
        // Find the role by name
        let role = self
            .find_by_name(role_name)
            .await?
            .ok_or_else(|| RepositoryError::not_found("roles", role_name))?;

        // Check if the user already has this role in this scope (not revoked)
        let existing = sqlx::query_as::<_, UserRoleRow>(
            r"
            SELECT * FROM user_roles
            WHERE user_id = $1
              AND role_id = $2
              AND scope_type = $3
              AND scope_id = $4
              AND revoked_at IS NULL
            ",
        )
        .bind(user_id)
        .bind(role.id)
        .bind(scope_type.as_str())
        .bind(scope_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(existing_role) = existing {
            return Ok(existing_role);
        }

        // Create the new assignment
        let user_role = sqlx::query_as::<_, UserRoleRow>(
            r"
            INSERT INTO user_roles (user_id, role_id, scope_type, scope_id, granted_by)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(role.id)
        .bind(scope_type.as_str())
        .bind(scope_id)
        .bind(granted_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "user role assignment"))?;

        Ok(user_role)
    }

    /// Revoke all roles for a user in a specific scope.
    ///
    /// This soft-deletes all role assignments for the user within the given scope.
    /// Used when removing a user from a team, league, etc.
    pub async fn revoke_scoped_roles(
        &self,
        user_id: Uuid,
        scope_type: ScopeType,
        scope_id: Uuid,
        revoked_by: Option<Uuid>,
    ) -> Result<u64, RepositoryError> {
        let result = sqlx::query(
            r"
            UPDATE user_roles SET
                revoked_at = NOW(),
                revoked_by = $4
            WHERE user_id = $1
              AND scope_type = $2
              AND scope_id = $3
              AND revoked_at IS NULL
            ",
        )
        .bind(user_id)
        .bind(scope_type.as_str())
        .bind(scope_id)
        .bind(revoked_by)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Count users with a specific role in a scope.
    ///
    /// Used for business rules like "team must have at least one captain".
    pub async fn count_scoped_role_holders(
        &self,
        role_name: &str,
        scope_type: ScopeType,
        scope_id: Uuid,
    ) -> Result<i64, RepositoryError> {
        let row = sqlx::query_as::<_, (i64,)>(
            r"
            SELECT COUNT(*) FROM user_roles ur
            JOIN roles r ON r.id = ur.role_id
            WHERE r.name = $1
              AND ur.scope_type = $2
              AND ur.scope_id = $3
              AND ur.revoked_at IS NULL
              AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
            ",
        )
        .bind(role_name)
        .bind(scope_type.as_str())
        .bind(scope_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }
}

/// Repository for permission operations.
#[derive(Clone)]
pub struct PermissionRepository {
    pool: DbPool,
}

impl PermissionRepository {
    /// Create a new permission repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a permission by ID.
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionRow>, RepositoryError> {
        let permission = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM permissions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(permission)
    }

    /// Find a permission by name.
    pub async fn find_by_name(&self, name: &str) -> Result<Option<PermissionRow>, RepositoryError> {
        let permission = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM permissions WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(permission)
    }

    /// Find a permission by ID or name.
    pub async fn find_by_id_or_name(
        &self,
        id_or_name: &str,
    ) -> Result<Option<PermissionRow>, RepositoryError> {
        let permission = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM permissions WHERE id::text = $1 OR name = $1",
        )
        .bind(id_or_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(permission)
    }

    /// List all permissions.
    pub async fn list(&self) -> Result<Vec<PermissionRow>, RepositoryError> {
        let permissions = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM permissions ORDER BY category, name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(permissions)
    }

    /// Get all permissions for a user (through their roles).
    pub async fn get_user_permissions(
        &self,
        user_id: UserId,
    ) -> Result<Vec<PermissionRow>, RepositoryError> {
        let permissions = sqlx::query_as::<_, PermissionRow>(
            r"
            SELECT DISTINCT p.* FROM permissions p
            JOIN role_permissions rp ON rp.permission_id = p.id
            JOIN user_roles ur ON ur.role_id = rp.role_id
            WHERE ur.user_id = $1
              AND ur.revoked_at IS NULL
              AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
            ORDER BY p.category, p.name
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(permissions)
    }

    /// Check if a user has a specific permission (global check, ignores scopes).
    pub async fn user_has_permission(
        &self,
        user_id: UserId,
        permission_name: &str,
    ) -> Result<bool, RepositoryError> {
        let row = sqlx::query(
            r"
            SELECT 1 FROM permissions p
            JOIN role_permissions rp ON rp.permission_id = p.id
            JOIN user_roles ur ON ur.role_id = rp.role_id
            WHERE ur.user_id = $1
              AND p.name = $2
              AND ur.revoked_at IS NULL
              AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
            ",
        )
        .bind(user_id.as_uuid())
        .bind(permission_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    // =========================================
    // Scoped Permission Methods
    // =========================================

    /// Check if a user has a permission within a specific scope.
    ///
    /// Permission is granted if:
    /// 1. User has a global role (`scope_type` IS NULL) that grants the permission, OR
    /// 2. User has a scoped role matching the requested scope that grants the permission
    ///
    /// If no scope is provided, only global roles are checked.
    pub async fn user_has_scoped_permission(
        &self,
        user_id: UserId,
        permission_name: &str,
        scope: Option<&PermissionScope>,
    ) -> Result<bool, RepositoryError> {
        let (scope_type_str, scope_id) = match scope {
            Some(s) => (Some(s.scope_type.as_str().to_string()), Some(s.scope_id)),
            None => (None, None),
        };

        let row = sqlx::query(
            r"
            SELECT 1 FROM permissions p
            JOIN role_permissions rp ON rp.permission_id = p.id
            JOIN user_roles ur ON ur.role_id = rp.role_id
            WHERE ur.user_id = $1
              AND p.name = $2
              AND ur.revoked_at IS NULL
              AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
              AND (
                  -- Global role (no scope) applies everywhere
                  ur.scope_type IS NULL
                  -- Or scope matches exactly (when scope is provided)
                  OR ($3::text IS NOT NULL AND ur.scope_type = $3 AND ur.scope_id = $4)
              )
            LIMIT 1
            ",
        )
        .bind(user_id.as_uuid())
        .bind(permission_name)
        .bind(&scope_type_str)
        .bind(scope_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    /// Get all permissions for a user within a specific scope (or global).
    ///
    /// Returns permissions from:
    /// 1. Global roles (`scope_type` IS NULL)
    /// 2. Scoped roles matching the requested scope (if provided)
    pub async fn get_user_permissions_in_scope(
        &self,
        user_id: UserId,
        scope: Option<&PermissionScope>,
    ) -> Result<Vec<PermissionRow>, RepositoryError> {
        let (scope_type_str, scope_id) = match scope {
            Some(s) => (Some(s.scope_type.as_str().to_string()), Some(s.scope_id)),
            None => (None, None),
        };

        let permissions = sqlx::query_as::<_, PermissionRow>(
            r"
            SELECT DISTINCT p.* FROM permissions p
            JOIN role_permissions rp ON rp.permission_id = p.id
            JOIN user_roles ur ON ur.role_id = rp.role_id
            WHERE ur.user_id = $1
              AND ur.revoked_at IS NULL
              AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
              AND (
                  ur.scope_type IS NULL
                  OR ($2::text IS NOT NULL AND ur.scope_type = $2 AND ur.scope_id = $3)
              )
            ORDER BY p.category, p.name
            ",
        )
        .bind(user_id.as_uuid())
        .bind(&scope_type_str)
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(permissions)
    }
}

/// Repository for ban operations.
#[derive(Clone)]
pub struct BanRepository {
    pool: DbPool,
}

impl BanRepository {
    /// Create a new ban repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Create a new ban.
    pub async fn create(&self, new_ban: NewBan) -> Result<BanRow, RepositoryError> {
        let ban = sqlx::query_as::<_, BanRow>(
            r"
            INSERT INTO bans (user_id, ban_type, reason, scope_type, scope_id, issued_by, starts_at, ends_at)
            VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, NOW()), $8)
            RETURNING *
            ",
        )
        .bind(new_ban.user_id)
        .bind(&new_ban.ban_type)
        .bind(&new_ban.reason)
        .bind(&new_ban.scope_type)
        .bind(new_ban.scope_id)
        .bind(new_ban.issued_by)
        .bind(new_ban.starts_at)
        .bind(new_ban.ends_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, "ban"))?;

        Ok(ban)
    }

    /// Lift active bans for a user.
    pub async fn lift(
        &self,
        user_id: Uuid,
        lifted_by: Option<Uuid>,
        lift_reason: Option<&str>,
    ) -> Result<Vec<BanRow>, RepositoryError> {
        let bans = sqlx::query_as::<_, BanRow>(
            r"
            UPDATE bans SET
                lifted_at = NOW(),
                lifted_by = $2,
                lift_reason = $3,
                updated_at = NOW()
            WHERE user_id = $1
              AND lifted_at IS NULL
              AND (ends_at IS NULL OR ends_at > NOW())
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(lifted_by)
        .bind(lift_reason)
        .fetch_all(&self.pool)
        .await?;

        Ok(bans)
    }

    /// Get active bans for a user.
    pub async fn get_active(&self, user_id: Uuid) -> Result<Vec<BanRow>, RepositoryError> {
        let bans = sqlx::query_as::<_, BanRow>(
            r"
            SELECT * FROM bans
            WHERE user_id = $1
              AND lifted_at IS NULL
              AND (ends_at IS NULL OR ends_at > NOW())
            ORDER BY starts_at DESC
            ",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(bans)
    }

    /// Check if a user has an active platform ban.
    pub async fn is_banned(&self, user_id: Uuid) -> Result<bool, RepositoryError> {
        let row = sqlx::query(
            r"
            SELECT 1 FROM bans
            WHERE user_id = $1
              AND ban_type = 'platform'
              AND lifted_at IS NULL
              AND (ends_at IS NULL OR ends_at > NOW())
            ",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use portal_test::database::TestDb;

    // Helper to create a test user
    async fn create_test_user(pool: &DbPool, suffix: &str) -> Uuid {
        let user = sqlx::query_as::<_, (Uuid,)>(
            r#"
            INSERT INTO users (username, email, password_hash)
            VALUES ($1, $2, 'hash')
            RETURNING id
            "#,
        )
        .bind(format!("rbacuser{}", suffix))
        .bind(format!("rbac{}@example.com", suffix))
        .fetch_one(pool)
        .await
        .unwrap();
        user.0
    }

    // Helper to create a test permission
    async fn create_test_permission(pool: &DbPool, name: &str) -> Uuid {
        let perm = sqlx::query_as::<_, (Uuid,)>(
            r#"
            INSERT INTO permissions (name, display_name, category)
            VALUES ($1, $2, 'test')
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(format!("{} Permission", name))
        .fetch_one(pool)
        .await
        .unwrap();
        perm.0
    }

    // ===========================================
    // RoleRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_create_role() {
        let db = TestDb::new().await;
        let repo = RoleRepository::new(db.pool.clone());

        let new_role = NewRole {
            name: "test_role".to_string(),
            display_name: "Test Role".to_string(),
            description: Some("A test role".to_string()),
            category: "custom".to_string(),
            priority: 10,
            color: Some("#FF0000".to_string()),
        };

        let role = repo.create(new_role).await.unwrap();
        assert_eq!(role.name, "test_role");
        assert_eq!(role.display_name, "Test Role");
        assert!(!role.is_system);
        assert!(!role.is_default);
    }

    #[tokio::test]
    async fn test_find_role_by_name() {
        let db = TestDb::new().await;
        let repo = RoleRepository::new(db.pool.clone());

        let new_role = NewRole {
            name: "findable_role".to_string(),
            display_name: "Findable Role".to_string(),
            description: None,
            category: "custom".to_string(),
            priority: 5,
            color: None,
        };
        repo.create(new_role).await.unwrap();

        let found = repo.find_by_name("findable_role").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().display_name, "Findable Role");

        let not_found = repo.find_by_name("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_list_roles() {
        let db = TestDb::new().await;
        let repo = RoleRepository::new(db.pool.clone());

        for i in 1..=3 {
            let new_role = NewRole {
                name: format!("list_role_{}", i),
                display_name: format!("List Role {}", i),
                description: None,
                category: "custom".to_string(),
                priority: i,
                color: None,
            };
            repo.create(new_role).await.unwrap();
        }

        let roles = repo.list().await.unwrap();
        assert!(roles.len() >= 3);
    }

    #[tokio::test]
    async fn test_add_remove_permission() {
        let db = TestDb::new().await;
        let repo = RoleRepository::new(db.pool.clone());

        // Create role
        let new_role = NewRole {
            name: "perm_role".to_string(),
            display_name: "Permission Role".to_string(),
            description: None,
            category: "custom".to_string(),
            priority: 5,
            color: None,
        };
        let role = repo.create(new_role).await.unwrap();

        // Create permission
        let perm_id = create_test_permission(&db.pool, "test_perm").await;

        // Add permission to role
        repo.add_permission(role.id, perm_id).await.unwrap();

        // Verify permission is added
        let perms = repo.get_permissions(role.id).await.unwrap();
        assert_eq!(perms.len(), 1);
        assert_eq!(perms[0].name, "test_perm");

        // Remove permission
        let removed = repo.remove_permission(role.id, perm_id).await.unwrap();
        assert!(removed);

        // Verify permission is removed
        let perms = repo.get_permissions(role.id).await.unwrap();
        assert!(perms.is_empty());
    }

    #[tokio::test]
    async fn test_assign_role_to_user() {
        let db = TestDb::new().await;
        let repo = RoleRepository::new(db.pool.clone());

        // Create user and role
        let user_id = create_test_user(&db.pool, "assign").await;
        let new_role = NewRole {
            name: "assign_role".to_string(),
            display_name: "Assign Role".to_string(),
            description: None,
            category: "custom".to_string(),
            priority: 5,
            color: None,
        };
        let role = repo.create(new_role).await.unwrap();

        // Assign role
        let assignment = NewUserRole {
            user_id,
            role_id: role.id,
            scope_type: None,
            scope_id: None,
            granted_by: None,
            expires_at: None,
        };
        let user_role = repo.assign_to_user(assignment).await.unwrap();
        assert_eq!(user_role.user_id, user_id);
        assert_eq!(user_role.role_id, role.id);
        assert!(user_role.revoked_at.is_none());
    }

    #[tokio::test]
    async fn test_revoke_role() {
        let db = TestDb::new().await;
        let repo = RoleRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "revoke").await;
        let new_role = NewRole {
            name: "revoke_role".to_string(),
            display_name: "Revoke Role".to_string(),
            description: None,
            category: "custom".to_string(),
            priority: 5,
            color: None,
        };
        let role = repo.create(new_role).await.unwrap();

        // Assign role
        let assignment = NewUserRole {
            user_id,
            role_id: role.id,
            scope_type: None,
            scope_id: None,
            granted_by: None,
            expires_at: None,
        };
        repo.assign_to_user(assignment).await.unwrap();

        // Revoke role
        let revoked = repo
            .revoke_from_user(user_id, role.id, None, None, None)
            .await
            .unwrap();
        assert!(revoked);

        // Verify role is no longer in user's roles
        let roles = repo.get_user_roles(UserId::from(user_id)).await.unwrap();
        assert!(roles.iter().all(|r| r.id != role.id));
    }

    #[tokio::test]
    async fn test_get_user_roles() {
        let db = TestDb::new().await;
        let repo = RoleRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "getroles").await;

        for i in 1..=2 {
            let new_role = NewRole {
                name: format!("user_role_{}", i),
                display_name: format!("User Role {}", i),
                description: None,
                category: "custom".to_string(),
                priority: i,
                color: None,
            };
            let role = repo.create(new_role).await.unwrap();

            let assignment = NewUserRole {
                user_id,
                role_id: role.id,
                scope_type: None,
                scope_id: None,
                granted_by: None,
                expires_at: None,
            };
            repo.assign_to_user(assignment).await.unwrap();
        }

        let roles = repo.get_user_roles(UserId::from(user_id)).await.unwrap();
        assert_eq!(roles.len(), 2);
    }

    // ===========================================
    // PermissionRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_get_user_permissions() {
        let db = TestDb::new().await;
        let role_repo = RoleRepository::new(db.pool.clone());
        let perm_repo = PermissionRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "getperms").await;

        // Create role with permissions
        let new_role = NewRole {
            name: "perm_test_role".to_string(),
            display_name: "Permission Test Role".to_string(),
            description: None,
            category: "custom".to_string(),
            priority: 5,
            color: None,
        };
        let role = role_repo.create(new_role).await.unwrap();

        let perm_id = create_test_permission(&db.pool, "user_perm").await;
        role_repo.add_permission(role.id, perm_id).await.unwrap();

        // Assign role to user
        let assignment = NewUserRole {
            user_id,
            role_id: role.id,
            scope_type: None,
            scope_id: None,
            granted_by: None,
            expires_at: None,
        };
        role_repo.assign_to_user(assignment).await.unwrap();

        // Get user permissions
        let perms = perm_repo.get_user_permissions(UserId::from(user_id)).await.unwrap();
        assert!(!perms.is_empty());
        assert!(perms.iter().any(|p| p.name == "user_perm"));
    }

    #[tokio::test]
    async fn test_user_has_permission() {
        let db = TestDb::new().await;
        let role_repo = RoleRepository::new(db.pool.clone());
        let perm_repo = PermissionRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "hasperm").await;

        // Create role with permission
        let new_role = NewRole {
            name: "has_perm_role".to_string(),
            display_name: "Has Perm Role".to_string(),
            description: None,
            category: "custom".to_string(),
            priority: 5,
            color: None,
        };
        let role = role_repo.create(new_role).await.unwrap();

        let perm_id = create_test_permission(&db.pool, "check_perm").await;
        role_repo.add_permission(role.id, perm_id).await.unwrap();

        // User doesn't have permission yet
        assert!(!perm_repo
            .user_has_permission(UserId::from(user_id), "check_perm")
            .await
            .unwrap());

        // Assign role
        let assignment = NewUserRole {
            user_id,
            role_id: role.id,
            scope_type: None,
            scope_id: None,
            granted_by: None,
            expires_at: None,
        };
        role_repo.assign_to_user(assignment).await.unwrap();

        // Now user has permission
        assert!(perm_repo
            .user_has_permission(UserId::from(user_id), "check_perm")
            .await
            .unwrap());
    }

    // ===========================================
    // BanRepository Tests
    // ===========================================

    #[tokio::test]
    async fn test_create_ban() {
        let db = TestDb::new().await;
        let repo = BanRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "banned").await;

        let new_ban = NewBan {
            user_id,
            ban_type: "platform".to_string(),
            reason: "Test ban".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: None,
            ends_at: None, // Permanent ban
        };

        let ban = repo.create(new_ban).await.unwrap();
        assert_eq!(ban.user_id, user_id);
        assert_eq!(ban.ban_type, "platform");
        assert!(ban.lifted_at.is_none());
    }

    #[tokio::test]
    async fn test_get_active_bans() {
        let db = TestDb::new().await;
        let repo = BanRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "activebans").await;

        // Create an active ban
        let new_ban = NewBan {
            user_id,
            ban_type: "platform".to_string(),
            reason: "Active ban".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: None,
            ends_at: None,
        };
        repo.create(new_ban).await.unwrap();

        let bans = repo.get_active(user_id).await.unwrap();
        assert_eq!(bans.len(), 1);
    }

    #[tokio::test]
    async fn test_is_banned() {
        let db = TestDb::new().await;
        let repo = BanRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "isbanned").await;

        // User is not banned initially
        assert!(!repo.is_banned(user_id).await.unwrap());

        // Create ban
        let new_ban = NewBan {
            user_id,
            ban_type: "platform".to_string(),
            reason: "Ban test".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: None,
            ends_at: None,
        };
        repo.create(new_ban).await.unwrap();

        // User is now banned
        assert!(repo.is_banned(user_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_lift_ban() {
        let db = TestDb::new().await;
        let repo = BanRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "liftban").await;

        // Create ban
        let new_ban = NewBan {
            user_id,
            ban_type: "platform".to_string(),
            reason: "Lift test".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: None,
            ends_at: None,
        };
        repo.create(new_ban).await.unwrap();

        assert!(repo.is_banned(user_id).await.unwrap());

        // Lift ban
        let lifted = repo.lift(user_id, None, Some("Pardoned")).await.unwrap();
        assert_eq!(lifted.len(), 1);
        assert_eq!(lifted[0].lift_reason, Some("Pardoned".to_string()));

        // User is no longer banned
        assert!(!repo.is_banned(user_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_expired_ban_not_active() {
        let db = TestDb::new().await;
        let repo = BanRepository::new(db.pool.clone());

        let user_id = create_test_user(&db.pool, "expired").await;

        // Create expired ban (ends_at in the past, starts_at even further back)
        let new_ban = NewBan {
            user_id,
            ban_type: "platform".to_string(),
            reason: "Expired ban".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: Some(Utc::now() - Duration::hours(2)),
            ends_at: Some(Utc::now() - Duration::hours(1)),
        };
        repo.create(new_ban).await.unwrap();

        // Expired ban should not make user banned
        assert!(!repo.is_banned(user_id).await.unwrap());

        // Active bans should not include expired ones
        let active = repo.get_active(user_id).await.unwrap();
        assert!(active.is_empty());
    }
}
