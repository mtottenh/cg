//! Permission repository adapter implementing domain trait.

use async_trait::async_trait;
use portal_core::{DomainError, UserId};
use portal_domain::repositories::PermissionRepository as PermissionRepositoryTrait;

use crate::repositories::PermissionRepository;

/// `PostgreSQL` implementation of the `PermissionRepository` trait.
pub struct PgPermissionRepository {
    inner: PermissionRepository,
}

impl PgPermissionRepository {
    /// Create a new `PostgreSQL` permission repository.
    pub const fn new(pool: crate::DbPool) -> Self {
        Self {
            inner: PermissionRepository::new(pool),
        }
    }
}

#[async_trait]
impl PermissionRepositoryTrait for PgPermissionRepository {
    async fn user_has_permission(
        &self,
        user_id: UserId,
        permission_name: &str,
    ) -> Result<bool, DomainError> {
        self.inner
            .user_has_permission(user_id, permission_name)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    async fn get_user_permission_names(&self, user_id: UserId) -> Result<Vec<String>, DomainError> {
        let permissions = self
            .inner
            .get_user_permissions(user_id)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(permissions.into_iter().map(|p| p.name).collect())
    }
}
