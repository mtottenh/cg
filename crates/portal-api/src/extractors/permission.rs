//! Permission checking extractors and utilities.

use crate::error::ApiError;
use crate::extractors::auth::AuthenticatedUser;
use crate::state::AppState;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use portal_core::{PermissionScope, ScopeType};
use portal_db::PermissionRepository;
use uuid::Uuid;

/// Wrapper for PermissionRepository that can be extracted from state.
#[derive(Clone)]
pub struct PermissionChecker(pub PermissionRepository);

impl FromRef<AppState> for PermissionChecker {
    fn from_ref(state: &AppState) -> Self {
        PermissionChecker(state.permission_repo.clone())
    }
}

impl<S> FromRequestParts<S> for PermissionChecker
where
    S: Send + Sync,
    PermissionChecker: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Ok(PermissionChecker::from_ref(state))
    }
}

impl PermissionChecker {
    /// Check if a user has a specific permission.
    pub async fn has_permission(&self, user: &AuthenticatedUser, permission: &str) -> bool {
        // Dev users have all permissions
        if AuthenticatedUser::is_dev_auth_enabled() && user.username == "devuser" {
            return true;
        }

        self.0
            .user_has_permission(user.user_id, permission)
            .await
            .unwrap_or(false)
    }

    /// Require a permission or return 403 Forbidden.
    pub async fn require_permission(
        &self,
        user: &AuthenticatedUser,
        permission: &str,
    ) -> Result<(), ApiError> {
        if self.has_permission(user, permission).await {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "Missing required permission: {}",
                permission
            )))
        }
    }

    /// Require any of the given permissions or return 403 Forbidden.
    pub async fn require_any_permission(
        &self,
        user: &AuthenticatedUser,
        permissions: &[&str],
    ) -> Result<(), ApiError> {
        // Dev users have all permissions
        if AuthenticatedUser::is_dev_auth_enabled() && user.username == "devuser" {
            return Ok(());
        }

        for permission in permissions {
            if self
                .0
                .user_has_permission(user.user_id, permission)
                .await
                .unwrap_or(false)
            {
                return Ok(());
            }
        }

        Err(ApiError::forbidden(format!(
            "Missing required permission: one of {:?}",
            permissions
        )))
    }

    /// Require all of the given permissions or return 403 Forbidden.
    pub async fn require_all_permissions(
        &self,
        user: &AuthenticatedUser,
        permissions: &[&str],
    ) -> Result<(), ApiError> {
        // Dev users have all permissions
        if AuthenticatedUser::is_dev_auth_enabled() && user.username == "devuser" {
            return Ok(());
        }

        for permission in permissions {
            if !self
                .0
                .user_has_permission(user.user_id, permission)
                .await
                .unwrap_or(false)
            {
                return Err(ApiError::forbidden(format!(
                    "Missing required permission: {}",
                    permission
                )));
            }
        }

        Ok(())
    }

    // =========================================================================
    // Scoped Permission Methods
    // =========================================================================

    /// Check if a user has a scoped permission (e.g., team.settings.manage for a specific team).
    ///
    /// This checks:
    /// 1. Whether the user has the permission in the specified scope, OR
    /// 2. Whether the user has a global admin override permission
    pub async fn has_scoped_permission(
        &self,
        user: &AuthenticatedUser,
        permission: &str,
        scope_type: ScopeType,
        scope_id: Uuid,
    ) -> bool {
        // Dev users have all permissions
        if AuthenticatedUser::is_dev_auth_enabled() && user.username == "devuser" {
            return true;
        }

        let scope = PermissionScope { scope_type, scope_id };
        self.0
            .user_has_scoped_permission(user.user_id, permission, Some(&scope))
            .await
            .unwrap_or(false)
    }

    /// Check if a user has global admin override for a scope type.
    ///
    /// Admin overrides:
    /// - Team scope: `admin.teams.manage_any`
    /// - League scope: `admin.leagues.manage_any`
    /// - Tournament scope: `admin.tournaments.manage_any`
    pub async fn has_admin_override(&self, user: &AuthenticatedUser, scope_type: ScopeType) -> bool {
        // Dev users have all permissions
        if AuthenticatedUser::is_dev_auth_enabled() && user.username == "devuser" {
            return true;
        }

        let admin_permission = match scope_type {
            ScopeType::Team => "admin.teams.manage_any",
            ScopeType::League => "admin.leagues.manage_any",
            ScopeType::Tournament => "admin.tournaments.manage_any",
            ScopeType::Match => "admin.tournaments.manage_any", // Matches fall under tournament admin
        };

        self.0
            .user_has_permission(user.user_id, admin_permission)
            .await
            .unwrap_or(false)
    }

    /// Require a scoped permission or admin override, or return 403 Forbidden.
    ///
    /// This is the primary method for checking team/league/tournament permissions.
    pub async fn require_scoped_permission(
        &self,
        user: &AuthenticatedUser,
        permission: &str,
        scope_type: ScopeType,
        scope_id: Uuid,
    ) -> Result<(), ApiError> {
        // Check scoped permission first
        if self.has_scoped_permission(user, permission, scope_type, scope_id).await {
            return Ok(());
        }

        // Check admin override
        if self.has_admin_override(user, scope_type).await {
            return Ok(());
        }

        Err(ApiError::forbidden(format!(
            "Missing required permission: {} for {:?} {}",
            permission, scope_type, scope_id
        )))
    }

    /// Convenience method for requiring team permissions.
    pub async fn require_team_permission(
        &self,
        user: &AuthenticatedUser,
        team_id: Uuid,
        permission: &str,
    ) -> Result<(), ApiError> {
        self.require_scoped_permission(user, permission, ScopeType::Team, team_id)
            .await
    }

    /// Convenience method for requiring league permissions.
    pub async fn require_league_permission(
        &self,
        user: &AuthenticatedUser,
        league_id: Uuid,
        permission: &str,
    ) -> Result<(), ApiError> {
        self.require_scoped_permission(user, permission, ScopeType::League, league_id)
            .await
    }

    /// Convenience method for requiring tournament permissions.
    pub async fn require_tournament_permission(
        &self,
        user: &AuthenticatedUser,
        tournament_id: Uuid,
        permission: &str,
    ) -> Result<(), ApiError> {
        self.require_scoped_permission(user, permission, ScopeType::Tournament, tournament_id)
            .await
    }
}

/// Macro to create a permission-requiring handler wrapper.
///
/// Usage:
/// ```ignore
/// require_permission!(create_team_handler, "teams.create");
/// ```
#[macro_export]
macro_rules! require_permission {
    ($handler:ident, $permission:expr) => {
        async fn $handler(
            State(state): State<$crate::state::AppState>,
            user: $crate::extractors::AuthenticatedUser,
            // ... other extractors
        ) -> Result<impl axum::response::IntoResponse, $crate::error::ApiError> {
            let perm_checker = $crate::extractors::PermissionChecker::from_ref(&state);
            perm_checker.require_permission(&user, $permission).await?;
            // ... rest of handler
        }
    };
}
