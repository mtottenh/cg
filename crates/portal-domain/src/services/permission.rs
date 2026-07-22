//! Permission service with authorization logic.

use crate::repositories::PermissionRepository;
use portal_core::{DomainError, UserId};
use std::sync::Arc;
use tracing::instrument;

/// Well-known permission used by [`PermissionService::is_admin`].
///
/// `users.view_all` is a READ permission granted to `super_admin`,
/// `platform_admin` AND `moderator`. It is only suitable for gating
/// admin-surface READ endpoints; mutations must check an explicit manage
/// permission (e.g. `admin.users.manage`, `admin.bans.manage`) instead.
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

    /// Check if a user has admin READ privileges.
    ///
    /// "Admin" here means "holds the `users.view_all` permission", which is
    /// granted to `super_admin`, `platform_admin` and `moderator`. Because
    /// moderators hold it, this check is only appropriate for READ surfaces
    /// (listing users, viewing bans, ...). Mutating admin endpoints must
    /// require an explicit manage permission (`admin.users.manage`,
    /// `admin.bans.manage`, ...) via the API-layer `PermissionChecker`.
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
        self.permission_repo
            .get_user_permission_names(user_id)
            .await
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
