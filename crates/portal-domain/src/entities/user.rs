//! User domain entity.

use chrono::{DateTime, Utc};
use portal_core::UserId;

/// User domain entity.
#[derive(Debug, Clone)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub status: UserStatus,
    pub locale: String,
    pub timezone: String,
    pub two_factor_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// User account status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserStatus {
    #[default]
    Active,
    Inactive,
    Suspended,
    Banned,
    PendingVerification,
}

impl std::fmt::Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Suspended => write!(f, "suspended"),
            Self::Banned => write!(f, "banned"),
            Self::PendingVerification => write!(f, "pending_verification"),
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

impl User {
    /// Check if the user can perform actions.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == UserStatus::Active
    }

    /// Check if the user is banned or suspended.
    #[must_use]
    pub const fn is_restricted(&self) -> bool {
        matches!(self.status, UserStatus::Banned | UserStatus::Suspended)
    }

    /// Check if the user needs to verify their email.
    #[must_use]
    pub fn needs_email_verification(&self) -> bool {
        !self.email_verified && self.status == UserStatus::PendingVerification
    }
}

/// User data needed for authentication.
/// This is separate from `User` to keep password hash out of the main entity.
#[derive(Debug, Clone)]
pub struct UserWithCredentials {
    /// User ID.
    pub id: UserId,
    /// Username.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Password hash (Argon2id PHC string).
    pub password_hash: Option<String>,
    /// Account status.
    pub status: UserStatus,
}
