//! Permission repository trait.

use async_trait::async_trait;
use portal_core::{DomainError, UserId};

/// Repository trait for permission operations.
#[async_trait]
pub trait PermissionRepository: Send + Sync {
    /// Check if a user has a specific permission (global check).
    async fn user_has_permission(
        &self,
        user_id: UserId,
        permission_name: &str,
    ) -> Result<bool, DomainError>;

    /// Get all permission names for a user.
    async fn get_user_permission_names(&self, user_id: UserId) -> Result<Vec<String>, DomainError>;
}
