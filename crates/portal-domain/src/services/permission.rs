//! Permission service with authorization logic.

use crate::repositories::PermissionRepository;
use portal_core::{DomainError, UserId};
use std::sync::Arc;
use tracing::instrument;

/// Well-known admin permission.
/// Users with this permission are considered platform administrators.
/// We use `users.view_all` which is granted to `platform_admin`, `super_admin`, and moderator roles.
const ADMIN_PERMISSION: &str = "users.view_all";

/// Service for authorization and permission checks.
pub struct PermissionService<PR>
where
    PR: PermissionRepository,
{
    permission_repo: Arc<PR>,
}

impl<PR> PermissionService<PR>
where
    PR: PermissionRepository,
{
    /// Create a new permission service.
    pub const fn new(permission_repo: Arc<PR>) -> Self {
        Self { permission_repo }
    }

    /// Check if a user has admin privileges.
    ///
    /// Admin status is determined by having the `admin.users.view` permission,
    /// which is granted to users with the `admin` role.
    #[instrument(skip(self))]
    pub async fn is_admin(&self, user_id: UserId) -> Result<bool, DomainError> {
        self.permission_repo
            .user_has_permission(user_id, ADMIN_PERMISSION)
            .await
    }

    /// Check if a user has a specific permission.
    #[instrument(skip(self))]
    pub async fn has_permission(
        &self,
        user_id: UserId,
        permission: &str,
    ) -> Result<bool, DomainError> {
        self.permission_repo
            .user_has_permission(user_id, permission)
            .await
    }

    /// Get all permission names for a user.
    #[instrument(skip(self))]
    pub async fn get_permissions(&self, user_id: UserId) -> Result<Vec<String>, DomainError> {
        self.permission_repo.get_user_permission_names(user_id).await
    }
}

impl<PR> Clone for PermissionService<PR>
where
    PR: PermissionRepository,
{
    fn clone(&self) -> Self {
        Self {
            permission_repo: Arc::clone(&self.permission_repo),
        }
    }
}
