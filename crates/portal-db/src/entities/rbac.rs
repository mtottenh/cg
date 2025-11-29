//! RBAC (Role-Based Access Control) database entities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `roles` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RoleRow {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub category: String,
    pub priority: i32,
    pub color: Option<String>,
    pub is_system: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new role.
#[derive(Debug, Clone)]
pub struct NewRole {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub category: String,
    pub priority: i32,
    pub color: Option<String>,
}

/// Database row for the `permissions` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PermissionRow {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub category: String,
    pub is_dangerous: bool,
    pub created_at: DateTime<Utc>,
}

/// Database row for the `user_roles` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserRoleRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub scope_type: Option<String>,
    pub scope_id: Option<Uuid>,
    pub granted_by: Option<Uuid>,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<Uuid>,
}

/// Data for assigning a role to a user.
#[derive(Debug, Clone)]
pub struct NewUserRole {
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub scope_type: Option<String>,
    pub scope_id: Option<Uuid>,
    pub granted_by: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Database row for the `bans` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BanRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub ban_type: String,
    pub reason: String,
    pub scope_type: Option<String>,
    pub scope_id: Option<Uuid>,
    pub issued_by: Option<Uuid>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub lifted_at: Option<DateTime<Utc>>,
    pub lifted_by: Option<Uuid>,
    pub lift_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for creating a new ban.
#[derive(Debug, Clone)]
pub struct NewBan {
    pub user_id: Uuid,
    pub ban_type: String,
    pub reason: String,
    pub scope_type: Option<String>,
    pub scope_id: Option<Uuid>,
    pub issued_by: Option<Uuid>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}
