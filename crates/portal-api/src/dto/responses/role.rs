//! Role and permission response DTOs.

use portal_db::entities::{PermissionRow, RoleRow, UserRoleRow};
use serde::Serialize;
use utoipa::ToSchema;

/// Response DTO for a role.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleResponse {
    /// Unique role identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Machine-readable name.
    #[schema(example = "league_moderator")]
    pub name: String,

    /// Human-readable display name.
    #[schema(example = "League Moderator")]
    pub display_name: String,

    /// Description of the role's purpose.
    #[schema(example = "Can manage league settings and moderate players")]
    pub description: Option<String>,

    /// Category for grouping roles.
    #[schema(example = "league")]
    pub category: String,

    /// Priority for role ordering (higher = more prominent).
    #[schema(example = 50)]
    pub priority: i32,

    /// Color for UI display (hex format).
    #[schema(example = "#4CAF50")]
    pub color: Option<String>,

    /// Whether this is a system-defined role (cannot be deleted).
    #[schema(example = false)]
    pub is_system: bool,

    /// Whether this role is assigned by default to new users.
    #[schema(example = false)]
    pub is_default: bool,

    /// When the role was created.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,

    /// When the role was last updated.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub updated_at: String,
}

impl From<RoleRow> for RoleResponse {
    fn from(row: RoleRow) -> Self {
        Self {
            id: row.id.to_string(),
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            category: row.category,
            priority: row.priority,
            color: row.color,
            is_system: row.is_system,
            is_default: row.is_default,
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
        }
    }
}

/// Response DTO for a permission.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PermissionResponse {
    /// Unique permission identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// Machine-readable name (e.g., "team.roster.manage").
    #[schema(example = "team.roster.manage")]
    pub name: String,

    /// Human-readable display name.
    #[schema(example = "Manage Team Roster")]
    pub display_name: String,

    /// Description of what this permission allows.
    #[schema(example = "Allows adding and removing team members")]
    pub description: Option<String>,

    /// Category for grouping permissions.
    #[schema(example = "team")]
    pub category: String,

    /// Whether this permission grants dangerous capabilities.
    #[schema(example = false)]
    pub is_dangerous: bool,

    /// When the permission was created.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,
}

impl From<PermissionRow> for PermissionResponse {
    fn from(row: PermissionRow) -> Self {
        Self {
            id: row.id.to_string(),
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            category: row.category,
            is_dangerous: row.is_dangerous,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

/// Response DTO for a user's role assignment.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserRoleAssignmentResponse {
    /// Unique assignment identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// The assigned role.
    pub role: RoleResponse,

    /// Scope type for context-specific assignments (e.g., "team", "league").
    #[schema(example = "league")]
    pub scope_type: Option<String>,

    /// Scope ID for context-specific assignments.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440001")]
    pub scope_id: Option<String>,

    /// Who granted this role assignment.
    pub granted_by: Option<String>,

    /// When the role was assigned.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub granted_at: String,

    /// When the role assignment expires (null for permanent).
    pub expires_at: Option<String>,
}

impl UserRoleAssignmentResponse {
    /// Create a response from a user role row and its associated role.
    pub fn new(assignment: UserRoleRow, role: RoleRow) -> Self {
        Self {
            id: assignment.id.to_string(),
            role: RoleResponse::from(role),
            scope_type: assignment.scope_type,
            scope_id: assignment.scope_id.map(|id| id.to_string()),
            granted_by: assignment.granted_by.map(|id| id.to_string()),
            granted_at: assignment.granted_at.to_rfc3339(),
            expires_at: assignment.expires_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// Response DTO for a role with its permissions.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleWithPermissionsResponse {
    /// The role.
    #[serde(flatten)]
    pub role: RoleResponse,

    /// Permissions assigned to this role.
    pub permissions: Vec<PermissionResponse>,
}

impl RoleWithPermissionsResponse {
    /// Create a response from a role row and its permissions.
    pub fn new(role: RoleRow, permissions: Vec<PermissionRow>) -> Self {
        Self {
            role: RoleResponse::from(role),
            permissions: permissions
                .into_iter()
                .map(PermissionResponse::from)
                .collect(),
        }
    }
}
