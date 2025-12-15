//! Role and permission request DTOs.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

/// Request body for creating a new role.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateRoleRequest {
    /// Unique machine-readable name for the role (lowercase alphanumeric with underscores).
    #[validate(length(min = 2, max = 50, message = "Name must be 2-50 characters"))]
    #[schema(example = "league_moderator")]
    pub name: String,

    /// Human-readable display name.
    #[validate(length(min = 2, max = 100, message = "Display name must be 2-100 characters"))]
    #[schema(example = "League Moderator")]
    pub display_name: String,

    /// Optional description of the role's purpose.
    #[validate(length(max = 500, message = "Description must be at most 500 characters"))]
    #[schema(example = "Can manage league settings and moderate players")]
    pub description: Option<String>,

    /// Category for grouping roles (e.g., "platform", "team", "league", "tournament").
    #[validate(length(min = 2, max = 50, message = "Category must be 2-50 characters"))]
    #[schema(example = "league")]
    pub category: String,

    /// Priority for role ordering (higher = more prominent).
    #[schema(example = 50)]
    pub priority: Option<i32>,

    /// Color for UI display (hex format, e.g., "#4CAF50").
    #[validate(length(max = 7, message = "Color must be at most 7 characters"))]
    #[schema(example = "#4CAF50")]
    pub color: Option<String>,
}

/// Request body for updating a role.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateRoleRequest {
    /// Human-readable display name.
    #[validate(length(min = 2, max = 100, message = "Display name must be 2-100 characters"))]
    #[schema(example = "League Administrator")]
    pub display_name: Option<String>,

    /// Description of the role's purpose.
    #[validate(length(max = 500, message = "Description must be at most 500 characters"))]
    #[schema(example = "Full administrative access to league management")]
    pub description: Option<String>,

    /// Priority for role ordering.
    #[schema(example = 60)]
    pub priority: Option<i32>,

    /// Color for UI display (hex format, e.g., "#2196F3").
    #[validate(length(max = 7, message = "Color must be at most 7 characters"))]
    #[schema(example = "#2196F3")]
    pub color: Option<String>,
}

/// Request body for assigning a role to a user.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AssignRoleRequest {
    /// The ID of the role to assign.
    #[schema(example = "01234567-89ab-cdef-0123-456789abcdef")]
    pub role_id: Uuid,

    /// Optional scope type for context-specific assignments (e.g., "team", "league", "tournament").
    /// If null, the role is assigned globally.
    #[validate(custom(function = "validate_scope_type"))]
    #[schema(example = "league")]
    pub scope_type: Option<String>,

    /// Optional scope ID for context-specific assignments.
    /// Required if scope_type is provided.
    #[schema(example = "01234567-89ab-cdef-0123-456789abcdef")]
    pub scope_id: Option<Uuid>,

    /// Optional expiration time for the role assignment.
    #[schema(example = "2025-12-31T23:59:59Z")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Request body for revoking a role from a user.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RevokeRoleRequest {
    /// The ID of the role to revoke.
    #[schema(example = "01234567-89ab-cdef-0123-456789abcdef")]
    pub role_id: Uuid,

    /// Optional scope type (must match the original assignment).
    #[validate(custom(function = "validate_scope_type"))]
    #[schema(example = "league")]
    pub scope_type: Option<String>,

    /// Optional scope ID (must match the original assignment).
    #[schema(example = "01234567-89ab-cdef-0123-456789abcdef")]
    pub scope_id: Option<Uuid>,
}

/// Request body for adding a permission to a role.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddPermissionToRoleRequest {
    /// The ID of the permission to add.
    #[schema(example = "01234567-89ab-cdef-0123-456789abcdef")]
    pub permission_id: Uuid,
}

/// Validate scope type.
fn validate_scope_type(scope_type: &str) -> Result<(), validator::ValidationError> {
    match scope_type {
        "team" | "league" | "tournament" | "match" => Ok(()),
        _ => Err(validator::ValidationError::new("invalid_scope_type")),
    }
}
