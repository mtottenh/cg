//! Role and permission management handlers (admin).

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    AddPermissionToRoleRequest, AssignRoleRequest, CreateRoleRequest, UpdateRoleRequest,
};
use crate::dto::responses::{
    PermissionResponse, RoleResponse, RoleWithPermissionsResponse, UserRoleAssignmentResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::RolesState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::UserId;
use portal_db::entities::{NewRole, NewUserRole};
use uuid::Uuid;
use validator::Validate;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Role name that is exempt from the assignment priority ceiling.
const SUPER_ADMIN_ROLE: &str = "super_admin";

/// Check if the user is an admin (holds `users.view_all`).
///
/// READ endpoints only: `users.view_all` is also granted to moderators.
/// Mutating endpoints gate on `admin.users.manage` via [`PermissionChecker`].
async fn require_admin(state: &RolesState, user_id: UserId) -> ApiResult<()> {
    let is_admin = state
        .permission_service
        .is_admin(user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }
    Ok(())
}

// ============== Role Management ==============

/// List all roles.
#[utoipa::path(
    get,
    path = "/v1/admin/roles",
    responses(
        (status = 200, description = "List of roles", body = DataResponse<Vec<RoleResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn list_roles(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<RoleResponse>>>> {
    let request_id = get_request_id(&headers);
    require_admin(&state, auth.user_id).await?;

    let roles = state
        .role_repo
        .list()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list roles: {e}")))?;

    let responses: Vec<RoleResponse> = roles.into_iter().map(RoleResponse::from).collect();
    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Create a new role.
#[utoipa::path(
    post,
    path = "/v1/admin/roles",
    request_body = CreateRoleRequest,
    responses(
        (status = 201, description = "Role created", body = DataResponse<RoleResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 409, description = "Role name already exists", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn create_role(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Json(body): Json<CreateRoleRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<RoleResponse>>)> {
    let request_id = get_request_id(&headers);
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::USERS_MANAGE)
        .await?;

    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let new_role = NewRole {
        name: body.name,
        display_name: body.display_name,
        description: body.description,
        category: body.category,
        priority: body.priority.unwrap_or(0),
        color: body.color,
    };

    let role = state
        .role_repo
        .create(new_role)
        .await
        .map_err(|e| ApiError::conflict(format!("Failed to create role: {e}")))?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(RoleResponse::from(role), request_id)),
    ))
}

/// Get a role by ID.
#[utoipa::path(
    get,
    path = "/v1/admin/roles/{role_id}",
    params(
        ("role_id" = String, Path, description = "Role ID"),
    ),
    responses(
        (status = 200, description = "Role details with permissions", body = DataResponse<RoleWithPermissionsResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Role not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn get_role(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(role_id): Path<String>,
) -> ApiResult<Json<DataResponse<RoleWithPermissionsResponse>>> {
    let request_id = get_request_id(&headers);
    require_admin(&state, auth.user_id).await?;

    let role_uuid: Uuid = role_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid role ID"))?;

    let role = state
        .role_repo
        .find_by_id(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch role: {e}")))?
        .ok_or_else(|| ApiError::not_found("Role not found"))?;

    let permissions = state
        .role_repo
        .get_permissions(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch permissions: {e}")))?;

    let response = RoleWithPermissionsResponse::new(role, permissions);
    Ok(Json(DataResponse::new(response, request_id)))
}

/// Update a role.
#[utoipa::path(
    patch,
    path = "/v1/admin/roles/{role_id}",
    params(
        ("role_id" = String, Path, description = "Role ID"),
    ),
    request_body = UpdateRoleRequest,
    responses(
        (status = 200, description = "Role updated", body = DataResponse<RoleResponse>),
        (status = 400, description = "Validation error or cannot update system role", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Role not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn update_role(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(role_id): Path<String>,
    Json(body): Json<UpdateRoleRequest>,
) -> ApiResult<Json<DataResponse<RoleResponse>>> {
    let request_id = get_request_id(&headers);
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::USERS_MANAGE)
        .await?;

    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let role_uuid: Uuid = role_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid role ID"))?;

    // Fetch existing role
    let existing = state
        .role_repo
        .find_by_id(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch role: {e}")))?
        .ok_or_else(|| ApiError::not_found("Role not found"))?;

    // Don't allow updating system roles (name field cannot be changed anyway)
    if existing.is_system {
        // Only allow updating display_name, description, priority, color for system roles
        // This is acceptable - we just can't delete or rename them
    }

    // Build update query using repository
    let updated = state
        .role_repo
        .update(
            role_uuid,
            body.display_name.as_deref(),
            body.description.as_deref(),
            body.priority,
            body.color.as_deref(),
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update role: {e}")))?
        .ok_or_else(|| ApiError::not_found("Role not found"))?;

    Ok(Json(DataResponse::new(
        RoleResponse::from(updated),
        request_id,
    )))
}

/// Delete a role.
#[utoipa::path(
    delete,
    path = "/v1/admin/roles/{role_id}",
    params(
        ("role_id" = String, Path, description = "Role ID"),
    ),
    responses(
        (status = 204, description = "Role deleted"),
        (status = 400, description = "Cannot delete system role", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Role not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn delete_role(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(role_id): Path<String>,
) -> ApiResult<StatusCode> {
    let _request_id = get_request_id(&headers);
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::USERS_MANAGE)
        .await?;

    let role_uuid: Uuid = role_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid role ID"))?;

    let deleted = state
        .role_repo
        .delete(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete role: {e}")))?;

    if !deleted {
        // Could be not found or is_system = true
        let role = state.role_repo.find_by_id(role_uuid).await.ok().flatten();
        if let Some(r) = role
            && r.is_system
        {
            return Err(ApiError::bad_request("Cannot delete system role"));
        }
        return Err(ApiError::not_found("Role not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ============== Role Permissions ==============

/// Add a permission to a role.
#[utoipa::path(
    post,
    path = "/v1/admin/roles/{role_id}/permissions",
    params(
        ("role_id" = String, Path, description = "Role ID"),
    ),
    request_body = AddPermissionToRoleRequest,
    responses(
        (status = 200, description = "Permission added", body = DataResponse<RoleWithPermissionsResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Role or permission not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn add_permission_to_role(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(role_id): Path<String>,
    Json(body): Json<AddPermissionToRoleRequest>,
) -> ApiResult<Json<DataResponse<RoleWithPermissionsResponse>>> {
    let request_id = get_request_id(&headers);
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::USERS_MANAGE)
        .await?;

    let role_uuid: Uuid = role_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid role ID"))?;

    // Verify role exists
    let role = state
        .role_repo
        .find_by_id(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch role: {e}")))?
        .ok_or_else(|| ApiError::not_found("Role not found"))?;

    // Verify permission exists
    state
        .permission_repo
        .find_by_id(body.permission_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch permission: {e}")))?
        .ok_or_else(|| ApiError::not_found("Permission not found"))?;

    // Add permission
    state
        .role_repo
        .add_permission(role_uuid, body.permission_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to add permission: {e}")))?;

    // Return updated role with permissions
    let permissions = state
        .role_repo
        .get_permissions(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch permissions: {e}")))?;

    let response = RoleWithPermissionsResponse::new(role, permissions);
    Ok(Json(DataResponse::new(response, request_id)))
}

/// Remove a permission from a role.
#[utoipa::path(
    delete,
    path = "/v1/admin/roles/{role_id}/permissions/{permission_id}",
    params(
        ("role_id" = String, Path, description = "Role ID"),
        ("permission_id" = String, Path, description = "Permission ID"),
    ),
    responses(
        (status = 200, description = "Permission removed", body = DataResponse<RoleWithPermissionsResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Role or permission not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn remove_permission_from_role(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((role_id, permission_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<RoleWithPermissionsResponse>>> {
    let request_id = get_request_id(&headers);
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::USERS_MANAGE)
        .await?;

    let role_uuid: Uuid = role_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid role ID"))?;
    let permission_uuid: Uuid = permission_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid permission ID"))?;

    // Verify role exists
    let role = state
        .role_repo
        .find_by_id(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch role: {e}")))?
        .ok_or_else(|| ApiError::not_found("Role not found"))?;

    // Remove permission
    state
        .role_repo
        .remove_permission(role_uuid, permission_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to remove permission: {e}")))?;

    // Return updated role with permissions
    let permissions = state
        .role_repo
        .get_permissions(role_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch permissions: {e}")))?;

    let response = RoleWithPermissionsResponse::new(role, permissions);
    Ok(Json(DataResponse::new(response, request_id)))
}

// ============== Permissions ==============

/// List all permissions.
#[utoipa::path(
    get,
    path = "/v1/admin/permissions",
    responses(
        (status = 200, description = "List of permissions", body = DataResponse<Vec<PermissionResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn list_permissions(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<PermissionResponse>>>> {
    let request_id = get_request_id(&headers);
    require_admin(&state, auth.user_id).await?;

    let permissions = state
        .permission_repo
        .list()
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list permissions: {e}")))?;

    let responses: Vec<PermissionResponse> = permissions
        .into_iter()
        .map(PermissionResponse::from)
        .collect();
    Ok(Json(DataResponse::new(responses, request_id)))
}

// ============== User Role Assignments ==============

/// Get all roles assigned to a user.
#[utoipa::path(
    get,
    path = "/v1/admin/users/{user_id}/roles",
    params(
        ("user_id" = String, Path, description = "User ID"),
    ),
    responses(
        (status = 200, description = "User's role assignments", body = DataResponse<Vec<UserRoleAssignmentResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn get_user_roles(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<UserRoleAssignmentResponse>>>> {
    let request_id = get_request_id(&headers);
    require_admin(&state, auth.user_id).await?;

    let user_uuid: Uuid = user_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid user ID"))?;

    // Get user role assignments using repository
    let assignments = state
        .role_repo
        .get_user_role_assignments(user_uuid)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch user roles: {e}")))?;

    // Fetch the role for each assignment
    let mut responses: Vec<UserRoleAssignmentResponse> = Vec::new();
    for assignment in assignments {
        if let Ok(Some(role)) = state.role_repo.find_by_id(assignment.role_id).await {
            responses.push(UserRoleAssignmentResponse::new(assignment, role));
        }
    }
    // Sort by role priority
    responses.sort_by(|a, b| b.role.priority.cmp(&a.role.priority));

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Assign a role to a user.
#[utoipa::path(
    post,
    path = "/v1/admin/users/{user_id}/roles",
    params(
        ("user_id" = String, Path, description = "User ID"),
    ),
    request_body = AssignRoleRequest,
    responses(
        (status = 201, description = "Role assigned", body = DataResponse<UserRoleAssignmentResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Role not found", body = ApiError),
        (status = 409, description = "User already has this role", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn assign_role_to_user(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(body): Json<AssignRoleRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<UserRoleAssignmentResponse>>)> {
    let request_id = get_request_id(&headers);
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::USERS_MANAGE)
        .await?;

    body.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    // Validate scope consistency
    if body.scope_type.is_some() != body.scope_id.is_some() {
        return Err(ApiError::bad_request(
            "scope_type and scope_id must both be provided or both be null",
        ));
    }

    let user_uuid: Uuid = user_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid user ID"))?;

    // Verify role exists
    let role = state
        .role_repo
        .find_by_id(body.role_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch role: {e}")))?
        .ok_or_else(|| ApiError::not_found("Role not found"))?;

    // Priority ceiling: a caller may only grant roles strictly below their own
    // highest-priority role. Seeded priorities (0014/0016): super_admin=1000,
    // platform_admin=900, moderator=500, user=100 — so a platform_admin can
    // grant moderator/user but not platform_admin or super_admin. Because the
    // `>=` comparison would also block super_admin from granting super_admin
    // (1000 >= 1000), callers holding the super_admin role are exempted
    // explicitly. The dev bypass account (test-utils builds only) is treated
    // like super_admin, matching its blanket-permission behaviour.
    if !PermissionChecker::is_bypass(&auth) {
        let caller_roles = state
            .role_repo
            .get_user_roles(auth.user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to fetch caller roles: {e}")))?;

        let is_super_admin = caller_roles.iter().any(|r| r.name == SUPER_ADMIN_ROLE);
        if !is_super_admin {
            let caller_max_priority = caller_roles
                .iter()
                .map(|r| r.priority)
                .max()
                .unwrap_or(i32::MIN);
            if role.priority >= caller_max_priority {
                return Err(ApiError::forbidden(
                    "Cannot assign a role with priority equal to or above your own highest role",
                ));
            }
        }
    }

    let new_assignment = NewUserRole {
        user_id: user_uuid,
        role_id: body.role_id,
        scope_type: body.scope_type,
        scope_id: body.scope_id,
        granted_by: Some(auth.user_id.as_uuid()),
        expires_at: body.expires_at,
    };

    let assignment = state
        .role_repo
        .assign_to_user(new_assignment)
        .await
        .map_err(|e| ApiError::conflict(format!("Failed to assign role: {e}")))?;

    let response = UserRoleAssignmentResponse::new(assignment, role);
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(response, request_id)),
    ))
}

/// Revoke a role from a user.
#[utoipa::path(
    delete,
    path = "/v1/admin/users/{user_id}/roles/{role_id}",
    params(
        ("user_id" = String, Path, description = "User ID"),
        ("role_id" = String, Path, description = "Role ID"),
        ("scope_type" = Option<String>, Query, description = "Scope type to match (optional)"),
        ("scope_id" = Option<String>, Query, description = "Scope ID to match (optional)"),
    ),
    responses(
        (status = 204, description = "Role revoked"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not an admin", body = ApiError),
        (status = 404, description = "Role assignment not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin_rbac"
)]
pub async fn revoke_role_from_user(
    State(state): State<RolesState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((user_id, role_id)): Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<RevokeRoleQuery>,
) -> ApiResult<StatusCode> {
    let _request_id = get_request_id(&headers);
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::USERS_MANAGE)
        .await?;

    let user_uuid: Uuid = user_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid user ID"))?;
    let role_uuid: Uuid = role_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid role ID"))?;

    let scope_id = query.scope_id.and_then(|s| s.parse().ok());

    let revoked = state
        .role_repo
        .revoke_from_user(
            user_uuid,
            role_uuid,
            query.scope_type.as_deref(),
            scope_id,
            Some(auth.user_id.as_uuid()),
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to revoke role: {e}")))?;

    if !revoked {
        return Err(ApiError::not_found("Role assignment not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Query parameters for revoking a role.
#[derive(Debug, serde::Deserialize)]
pub struct RevokeRoleQuery {
    pub scope_type: Option<String>,
    pub scope_id: Option<String>,
}
