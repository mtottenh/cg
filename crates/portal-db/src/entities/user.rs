//! User database entity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `users` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserRow {
    pub id: Uuid,

    // Identity
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,

    // Authentication
    pub password_hash: Option<String>,
    pub password_changed_at: Option<DateTime<Utc>>,
    /// Authentication provider: 'local' (password) or 'steam' (OpenID).
    pub auth_provider: String,

    // Two-Factor Authentication
    pub two_factor_enabled: bool,
    pub two_factor_secret: Option<String>,
    pub two_factor_backup_codes: Option<serde_json::Value>,

    // Account Status
    pub status: String,
    pub status_reason: Option<String>,
    pub status_changed_at: Option<DateTime<Utc>>,

    // Metadata
    pub locale: Option<String>,
    pub timezone: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// User status values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserStatus {
    Active,
    Inactive,
    Suspended,
    Banned,
    PendingVerification,
}

impl UserStatus {
    /// Convert to database string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Suspended => "suspended",
            Self::Banned => "banned",
            Self::PendingVerification => "pending_verification",
        }
    }
}

impl std::str::FromStr for UserStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "suspended" => Ok(Self::Suspended),
            "banned" => Ok(Self::Banned),
            "pending_verification" => Ok(Self::PendingVerification),
            _ => Err(format!("invalid user status: {s}")),
        }
    }
}

/// Data for inserting a new user.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
}

/// Data for updating an existing user.
#[derive(Debug, Clone, Default)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub password_hash: Option<String>,
    pub status: Option<String>,
    pub status_reason: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub two_factor_enabled: Option<bool>,
}
