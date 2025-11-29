# Auth & RBAC Vertical Slice Design
## Multi-Game Competitive Gaming Portal

**Version:** 1.0
**Status:** Draft for Engineering Review
**Last Updated:** November 2024

---

## Table of Contents

1. [Overview](#1-overview)
2. [Authentication Model](#2-authentication-model)
3. [Authorization Model (RBAC)](#3-authorization-model-rbac)
4. [Core Layer (`portal-core`)](#4-core-layer-portal-core)
5. [Database Layer (`portal-db`)](#5-database-layer-portal-db)
6. [Domain Layer (`portal-domain`)](#6-domain-layer-portal-domain)
7. [API Layer (`portal-api`)](#7-api-layer-portal-api)
8. [Security Considerations](#8-security-considerations)
9. [Implementation Checklist](#9-implementation-checklist)

---

## 1. Overview

### 1.1 Purpose

This document provides a comprehensive design specification for the **Authentication & RBAC** vertical slice. It covers user authentication (registration, login, sessions, 2FA), authorization (roles, permissions, scopes), and security infrastructure (bans, audit logging).

### 1.2 Scope

The Auth & RBAC domain encompasses:

- **User Authentication**: Registration, login, password management, sessions
- **OAuth Integration**: Steam, Discord, Twitch, Google providers
- **Two-Factor Authentication**: TOTP-based 2FA with backup codes
- **Session Management**: Access tokens (JWT), refresh tokens, device tracking
- **Role-Based Access Control**: Roles, permissions, scoped assignments
- **Ban System**: Platform-wide and scoped bans with appeals
- **Audit Logging**: Security-relevant event tracking

### 1.3 Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Token Format | JWT (access) + Opaque (refresh) | Stateless validation + secure rotation |
| Access Token Lifetime | 15 minutes | Balance security and UX |
| Refresh Token Lifetime | 7 days | Reasonable session duration |
| Password Hashing | Argon2id | OWASP recommendation, memory-hard |
| 2FA Method | TOTP (RFC 6238) | Widely supported, offline-capable |
| Permission Model | Hierarchical RBAC + Scopes | Flexible, supports team/league contexts |

### 1.4 Dependencies

- `portal-core`: ID types, error types, JWT claims structure
- `portal-db`: Entity definitions, repository implementations
- `portal-domain`: Service traits, authentication/authorization logic
- `portal-api`: HTTP handlers, middleware, DTOs
- **External**: argon2 (hashing), jsonwebtoken (JWT), totp-rs (2FA)

---

## 2. Authentication Model

### 2.1 Authentication Flow Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        AUTHENTICATION FLOWS                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────┐                                                           │
│  │   Register   │──► Create User ──► Send Verification Email                │
│  └──────────────┘         │                                                 │
│                           ▼                                                 │
│  ┌──────────────┐    ┌─────────┐    ┌─────────────┐                        │
│  │    Login     │──► │Validate │──► │ Check 2FA   │                        │
│  │ (password)   │    │Password │    │  Enabled?   │                        │
│  └──────────────┘    └─────────┘    └──────┬──────┘                        │
│                                            │                                │
│                           ┌────────────────┼────────────────┐              │
│                           ▼                                 ▼              │
│                    ┌─────────────┐                 ┌─────────────┐         │
│                    │  No 2FA     │                 │  2FA Token  │         │
│                    │Issue Tokens │                 │  Required   │         │
│                    └──────┬──────┘                 └──────┬──────┘         │
│                           │                               │                │
│                           ▼                               ▼                │
│                    ┌─────────────┐                 ┌─────────────┐         │
│                    │   Return    │                 │ Verify TOTP │         │
│                    │Access+Refresh                 │   Code      │         │
│                    └─────────────┘                 └──────┬──────┘         │
│                                                          │                │
│                                                          ▼                │
│                                                   ┌─────────────┐         │
│                                                   │Issue Tokens │         │
│                                                   └─────────────┘         │
│                                                                            │
│  ┌──────────────┐                                                          │
│  │OAuth Login   │──► Redirect to Provider ──► Callback ──► Link/Create    │
│  │(Steam/Discord)    ──► Issue Tokens                                     │
│  └──────────────┘                                                          │
│                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 User Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `username` | VARCHAR(32) | Unique username (3-32 alphanumeric) |
| `email` | VARCHAR(255) | Unique email address |
| `email_verified` | BOOLEAN | Email verification status |
| `email_verified_at` | TIMESTAMPTZ | When verified |
| `password_hash` | VARCHAR(255) | Argon2id hash (NULL for OAuth-only) |
| `password_changed_at` | TIMESTAMPTZ | Last password change |
| `two_factor_enabled` | BOOLEAN | 2FA status |
| `two_factor_secret` | VARCHAR(255) | Encrypted TOTP secret |
| `two_factor_backup_codes` | JSONB | Encrypted backup codes |
| `status` | VARCHAR(20) | Account status |
| `status_reason` | TEXT | Reason for status |
| `locale` | VARCHAR(10) | User locale |
| `timezone` | VARCHAR(64) | User timezone |
| `created_at` | TIMESTAMPTZ | Account creation |
| `last_login_at` | TIMESTAMPTZ | Last successful login |

**Status Values**: `active`, `inactive`, `suspended`, `banned`, `pending_verification`

### 2.3 OAuth Connection Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `user_id` | UUID | FK to users |
| `provider` | VARCHAR(32) | OAuth provider name |
| `provider_user_id` | VARCHAR(255) | ID from provider |
| `provider_username` | VARCHAR(255) | Username from provider |
| `provider_email` | VARCHAR(255) | Email from provider |
| `provider_avatar_url` | VARCHAR(512) | Avatar from provider |
| `access_token` | TEXT | Encrypted access token |
| `refresh_token` | TEXT | Encrypted refresh token |
| `token_expires_at` | TIMESTAMPTZ | Token expiration |
| `provider_data` | JSONB | Provider-specific data |
| `last_used_at` | TIMESTAMPTZ | Last OAuth use |

**Supported Providers**: `steam`, `discord`, `twitch`, `google`, `battlenet`

### 2.4 Session & Token Entities

#### Refresh Tokens

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `user_id` | UUID | FK to users |
| `token_hash` | VARCHAR(64) | SHA-256 of token |
| `token_family` | UUID | For rotation detection |
| `device_id` | VARCHAR(64) | Device fingerprint |
| `device_name` | VARCHAR(128) | Device description |
| `device_type` | VARCHAR(32) | web, mobile, desktop |
| `ip_address` | INET | Request IP |
| `issued_at` | TIMESTAMPTZ | Token issued |
| `expires_at` | TIMESTAMPTZ | Token expires |
| `last_used_at` | TIMESTAMPTZ | Last refresh |
| `revoked_at` | TIMESTAMPTZ | If revoked |
| `replaced_by` | UUID | New token after rotation |

#### User Sessions

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `user_id` | UUID | FK to users |
| `refresh_token_id` | UUID | FK to refresh_tokens |
| `device_fingerprint` | VARCHAR(64) | Browser fingerprint |
| `device_name` | VARCHAR(128) | Device description |
| `browser` | VARCHAR(64) | Browser name |
| `os` | VARCHAR(64) | Operating system |
| `ip_address` | INET | Current IP |
| `ip_country` | CHAR(2) | GeoIP country |
| `last_active_at` | TIMESTAMPTZ | Last activity |
| `is_current` | BOOLEAN | Current session flag |
| `terminated_at` | TIMESTAMPTZ | If terminated |

### 2.5 JWT Claims Structure

```rust
/// Access token claims.
#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
    /// JWT ID (for revocation)
    pub jti: String,
    /// Token type
    pub typ: String,  // "access"
    /// User's current roles (for fast checks)
    pub roles: Vec<String>,
    /// Session ID
    pub sid: String,
}

/// Refresh token claims (minimal, actual data in DB).
#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshTokenClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Issued at timestamp
    pub iat: i64,
    /// Expiration timestamp
    pub exp: i64,
    /// JWT ID (for revocation lookup)
    pub jti: String,
    /// Token type
    pub typ: String,  // "refresh"
    /// Token family (for rotation detection)
    pub fam: String,
}
```

---

## 3. Authorization Model (RBAC)

### 3.1 RBAC Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           RBAC MODEL                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────┐     M:N      ┌──────────┐     M:N      ┌─────────────┐       │
│  │   User   │─────────────►│   Role   │─────────────►│ Permission  │       │
│  └──────────┘              └──────────┘              └─────────────┘       │
│       │                         │                                           │
│       │                         │ Hierarchy                                 │
│       │                         ▼                                           │
│       │                    ┌──────────┐                                    │
│       │                    │  Parent  │                                    │
│       │                    │   Role   │                                    │
│       │                    └──────────┘                                    │
│       │                                                                     │
│       │ Scoped Assignment                                                  │
│       ▼                                                                     │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │ user_roles                                                          │   │
│  │ ├── role_id: UUID                                                   │   │
│  │ ├── scope_type: team | tournament | league | game (optional)        │   │
│  │ └── scope_id: UUID (optional)                                       │   │
│  │                                                                      │   │
│  │ Example Assignments:                                                 │   │
│  │ • User A has "player" role (global, no scope)                       │   │
│  │ • User A has "team_captain" role for Team X (scope: team, team_x_id)│   │
│  │ • User B has "league_admin" role for League Y (scope: league, y_id) │   │
│  └────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Built-in Roles

| Role | Priority | Category | Description |
|------|----------|----------|-------------|
| `platform_admin` | 1000 | system | Full platform access |
| `moderator` | 500 | moderator | Community moderation |
| `verified_player` | 100 | player | Verified identity |
| `player` | 10 | player | Default role (auto-assigned) |
| `team_captain` | 200 | team | Team management (scoped) |
| `team_officer` | 150 | team | Team officer (scoped) |
| `league_admin` | 300 | tournament | League management (scoped) |
| `tournament_admin` | 250 | tournament | Tournament management (scoped) |

### 3.3 Permission Categories

| Category | Permissions |
|----------|-------------|
| **auth** | `auth.manage_sessions` |
| **player** | `player.profile.read`, `player.profile.update`, `player.profile.update.any` |
| **team** | `team.create`, `team.manage`, `team.manage.any`, `team.delete`, `team.members.manage`, `team.members.invite` |
| **match** | `match.view`, `match.create`, `match.admin` |
| **tournament** | `tournament.view`, `tournament.create`, `tournament.manage`, `tournament.admin` |
| **queue** | `queue.join` |
| **lobby** | `lobby.create`, `lobby.join`, `lobby.admin` |
| **admin** | `admin.users.view`, `admin.users.manage`, `admin.bans.manage`, `admin.games.manage`, `admin.system.configure` |

### 3.4 Permission Resolution

```rust
/// Check if user has permission, considering scopes.
pub async fn has_permission(
    &self,
    user_id: UserId,
    permission: &str,
    scope: Option<PermissionScope>,
) -> Result<bool, RbacError> {
    // 1. Get user's roles (including scoped roles)
    let roles = self.get_user_roles(user_id).await?;

    // 2. For each role, check if it grants the permission
    for role_assignment in roles {
        // Check if role has the permission
        if !self.role_has_permission(&role_assignment.role, permission).await? {
            continue;
        }

        // If permission doesn't require scope, global role is sufficient
        if scope.is_none() {
            return Ok(true);
        }

        // If role is global (no scope), it applies everywhere
        if role_assignment.scope.is_none() {
            return Ok(true);
        }

        // Check if role scope matches requested scope
        if role_assignment.scope == scope {
            return Ok(true);
        }
    }

    Ok(false)
}
```

### 3.5 Ban Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `user_id` | UUID | Banned user |
| `banned_by` | UUID | Admin who issued |
| `ban_type` | VARCHAR(32) | Type of ban |
| `scope_type` | VARCHAR(32) | Optional scope type |
| `scope_id` | UUID | Optional scope ID |
| `reason` | TEXT | Public reason |
| `internal_notes` | TEXT | Admin notes |
| `evidence_urls` | JSONB | Evidence links |
| `starts_at` | TIMESTAMPTZ | Ban start |
| `ends_at` | TIMESTAMPTZ | Ban end (NULL = permanent) |
| `appeal_status` | VARCHAR(32) | Appeal status |
| `appeal_text` | TEXT | Appeal message |
| `lifted_at` | TIMESTAMPTZ | If ban was lifted |
| `lifted_by` | UUID | Who lifted |

**Ban Types**: `platform`, `game`, `tournament`, `league`, `matchmaking`, `chat`
**Appeal Status**: `pending`, `under_review`, `approved`, `denied`

---

## 4. Core Layer (`portal-core`)

### 4.1 ID Types

```rust
// src/ids.rs

define_id!(UserId, "usr");
define_id!(RoleId, "rol");
define_id!(PermissionId, "prm");
define_id!(RefreshTokenId, "rtk");
define_id!(SessionId, "ses");
define_id!(BanId, "ban");
define_id!(OAuthConnectionId, "oac");
```

### 4.2 Auth Types

```rust
// src/types/auth.rs

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// User account status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Active,
    Inactive,
    Suspended,
    Banned,
    PendingVerification,
}

impl UserStatus {
    /// Check if user can log in.
    #[must_use]
    pub fn can_login(&self) -> bool {
        matches!(self, Self::Active | Self::PendingVerification)
    }

    /// Check if user can perform actions.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl fmt::Display for UserStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Suspended => write!(f, "suspended"),
            Self::Banned => write!(f, "banned"),
            Self::PendingVerification => write!(f, "pending_verification"),
        }
    }
}

impl FromStr for UserStatus {
    type Err = crate::CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "suspended" => Ok(Self::Suspended),
            "banned" => Ok(Self::Banned),
            "pending_verification" => Ok(Self::PendingVerification),
            _ => Err(crate::CoreError::InvalidEnumValue {
                enum_name: "UserStatus",
                value: s.to_string(),
            }),
        }
    }
}

/// OAuth provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthProvider {
    Steam,
    Discord,
    Twitch,
    Google,
    BattleNet,
}

impl fmt::Display for OAuthProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Steam => write!(f, "steam"),
            Self::Discord => write!(f, "discord"),
            Self::Twitch => write!(f, "twitch"),
            Self::Google => write!(f, "google"),
            Self::BattleNet => write!(f, "battlenet"),
        }
    }
}

/// Device type for sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Web,
    MobileIos,
    MobileAndroid,
    Desktop,
    Api,
    Unknown,
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Web => write!(f, "web"),
            Self::MobileIos => write!(f, "mobile_ios"),
            Self::MobileAndroid => write!(f, "mobile_android"),
            Self::Desktop => write!(f, "desktop"),
            Self::Api => write!(f, "api"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}
```

### 4.3 RBAC Types

```rust
// src/types/rbac.rs

use serde::{Deserialize, Serialize};
use std::fmt;

/// Role category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleCategory {
    System,
    Admin,
    Moderator,
    Tournament,
    Team,
    Player,
    Custom,
}

impl fmt::Display for RoleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::Admin => write!(f, "admin"),
            Self::Moderator => write!(f, "moderator"),
            Self::Tournament => write!(f, "tournament"),
            Self::Team => write!(f, "team"),
            Self::Player => write!(f, "player"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// Permission category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionCategory {
    Auth,
    Player,
    Team,
    Match,
    Tournament,
    League,
    Game,
    Admin,
    Moderation,
    System,
}

/// Scope type for scoped permissions/roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeType {
    Game,
    Team,
    Tournament,
    League,
    Season,
}

impl fmt::Display for ScopeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game => write!(f, "game"),
            Self::Team => write!(f, "team"),
            Self::Tournament => write!(f, "tournament"),
            Self::League => write!(f, "league"),
            Self::Season => write!(f, "season"),
        }
    }
}

/// Permission scope for authorization checks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PermissionScope {
    pub scope_type: ScopeType,
    pub scope_id: uuid::Uuid,
}

impl PermissionScope {
    pub fn team(team_id: uuid::Uuid) -> Self {
        Self {
            scope_type: ScopeType::Team,
            scope_id: team_id,
        }
    }

    pub fn tournament(tournament_id: uuid::Uuid) -> Self {
        Self {
            scope_type: ScopeType::Tournament,
            scope_id: tournament_id,
        }
    }

    pub fn league(league_id: uuid::Uuid) -> Self {
        Self {
            scope_type: ScopeType::League,
            scope_id: league_id,
        }
    }
}
```

### 4.4 Ban Types

```rust
// src/types/ban.rs

use serde::{Deserialize, Serialize};
use std::fmt;

/// Ban type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BanType {
    /// Full platform ban
    Platform,
    /// Banned from specific game
    Game,
    /// Banned from specific tournament
    Tournament,
    /// Banned from specific league
    League,
    /// Banned from matchmaking
    Matchmaking,
    /// Banned from chat
    Chat,
}

impl fmt::Display for BanType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Platform => write!(f, "platform"),
            Self::Game => write!(f, "game"),
            Self::Tournament => write!(f, "tournament"),
            Self::League => write!(f, "league"),
            Self::Matchmaking => write!(f, "matchmaking"),
            Self::Chat => write!(f, "chat"),
        }
    }
}

/// Appeal status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppealStatus {
    Pending,
    UnderReview,
    Approved,
    Denied,
}

impl fmt::Display for AppealStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::UnderReview => write!(f, "under_review"),
            Self::Approved => write!(f, "approved"),
            Self::Denied => write!(f, "denied"),
        }
    }
}
```

### 4.5 Validation Types

```rust
// src/types/auth.rs (continued)

use crate::CoreError;

/// Validated email address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Email(String);

impl Email {
    pub fn new(email: impl Into<String>) -> Result<Self, CoreError> {
        let email = email.into().trim().to_lowercase();

        // Basic email validation
        if !email.contains('@') || !email.contains('.') {
            return Err(CoreError::ValidationError {
                field: "email".to_string(),
                message: "Invalid email format".to_string(),
            });
        }

        if email.len() > 255 {
            return Err(CoreError::ValidationError {
                field: "email".to_string(),
                message: "Email too long".to_string(),
            });
        }

        Ok(Self(email))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Email {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Email> for String {
    fn from(email: Email) -> Self {
        email.0
    }
}

/// Validated username.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Username(String);

impl Username {
    pub fn new(username: impl Into<String>) -> Result<Self, CoreError> {
        let username = username.into();

        if username.len() < 3 || username.len() > 32 {
            return Err(CoreError::ValidationError {
                field: "username".to_string(),
                message: "Username must be 3-32 characters".to_string(),
            });
        }

        let valid = username.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '_' || c == '-'
        });

        if !valid {
            return Err(CoreError::ValidationError {
                field: "username".to_string(),
                message: "Username can only contain letters, numbers, underscores, and hyphens".to_string(),
            });
        }

        Ok(Self(username))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Password for validation (not stored).
#[derive(Debug, Clone)]
pub struct Password(String);

impl Password {
    /// Minimum password length.
    pub const MIN_LENGTH: usize = 8;

    pub fn new(password: impl Into<String>) -> Result<Self, CoreError> {
        let password = password.into();

        if password.len() < Self::MIN_LENGTH {
            return Err(CoreError::ValidationError {
                field: "password".to_string(),
                message: format!("Password must be at least {} characters", Self::MIN_LENGTH),
            });
        }

        // Require at least one uppercase, one lowercase, one digit
        let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
        let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
        let has_digit = password.chars().any(|c| c.is_ascii_digit());

        if !has_upper || !has_lower || !has_digit {
            return Err(CoreError::ValidationError {
                field: "password".to_string(),
                message: "Password must contain uppercase, lowercase, and digit".to_string(),
            });
        }

        Ok(Self(password))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

### 4.6 Error Types

```rust
// src/error.rs

/// Authentication errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Account not found")]
    AccountNotFound,

    #[error("Account is {status}")]
    AccountNotActive { status: UserStatus },

    #[error("Email already registered")]
    EmailTaken,

    #[error("Username already taken")]
    UsernameTaken,

    #[error("Email not verified")]
    EmailNotVerified,

    #[error("Invalid or expired token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("2FA required")]
    TwoFactorRequired { challenge_token: String },

    #[error("Invalid 2FA code")]
    InvalidTwoFactorCode,

    #[error("2FA not enabled")]
    TwoFactorNotEnabled,

    #[error("2FA already enabled")]
    TwoFactorAlreadyEnabled,

    #[error("Session not found")]
    SessionNotFound,

    #[error("Session expired")]
    SessionExpired,

    #[error("OAuth error: {0}")]
    OAuthError(String),

    #[error("Password reset token expired")]
    ResetTokenExpired,
}

/// Authorization errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RbacError {
    #[error("Permission denied: {permission}")]
    PermissionDenied { permission: String },

    #[error("Role not found: {0}")]
    RoleNotFound(String),

    #[error("Permission not found: {0}")]
    PermissionNotFound(String),

    #[error("Cannot modify system role")]
    SystemRoleModification,

    #[error("User is banned")]
    UserBanned,

    #[error("Invalid scope for role")]
    InvalidScope,
}

/// Ban errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BanError {
    #[error("Ban not found")]
    NotFound,

    #[error("Cannot ban platform admin")]
    CannotBanAdmin,

    #[error("Ban already expired")]
    AlreadyExpired,

    #[error("Ban already lifted")]
    AlreadyLifted,

    #[error("Appeal already submitted")]
    AppealExists,

    #[error("Cannot appeal permanent ban")]
    CannotAppealPermanent,
}
```

---

## 5. Database Layer (`portal-db`)

### 5.1 User Entity

```rust
// src/entities/user.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `users` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub password_hash: Option<String>,
    pub password_changed_at: Option<DateTime<Utc>>,
    pub two_factor_enabled: bool,
    pub two_factor_secret: Option<String>,
    pub two_factor_backup_codes: Option<serde_json::Value>,
    pub status: String,
    pub status_reason: Option<String>,
    pub status_changed_at: Option<DateTime<Utc>>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// Data for creating a new user.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub status: String,
}

/// Data for updating a user.
#[derive(Debug, Clone, Default)]
pub struct UpdateUser {
    pub username: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub password_hash: Option<String>,
    pub two_factor_enabled: Option<bool>,
    pub two_factor_secret: Option<String>,
    pub two_factor_backup_codes: Option<serde_json::Value>,
    pub status: Option<String>,
    pub status_reason: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
}
```

### 5.2 Role & Permission Entities

```rust
// src/entities/role.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RoleRow {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub category: String,
    pub priority: i32,
    pub parent_role_id: Option<Uuid>,
    pub is_system: bool,
    pub is_default: bool,
    pub is_assignable: bool,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PermissionRow {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub category: String,
    pub resource_type: Option<String>,
    pub is_dangerous: bool,
    pub requires_2fa: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserRoleRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub scope_type: Option<String>,
    pub scope_id: Option<Uuid>,
    pub granted_by: Option<Uuid>,
    pub granted_at: DateTime<Utc>,
    pub reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<Uuid>,
    pub revoke_reason: Option<String>,
}

/// Data for assigning a role to a user.
#[derive(Debug, Clone)]
pub struct NewUserRole {
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub scope_type: Option<String>,
    pub scope_id: Option<Uuid>,
    pub granted_by: Option<Uuid>,
    pub reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}
```

### 5.3 Session & Token Entities

```rust
// src/entities/session.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RefreshTokenRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub token_family: Uuid,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub device_type: Option<String>,
    pub user_agent: Option<String>,
    pub ip_address: Option<IpAddr>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
    pub replaced_by: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserSessionRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub refresh_token_id: Option<Uuid>,
    pub session_token_hash: String,
    pub device_fingerprint: Option<String>,
    pub device_name: Option<String>,
    pub device_type: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub ip_address: Option<IpAddr>,
    pub ip_country: Option<String>,
    pub ip_city: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub is_current: bool,
    pub terminated_at: Option<DateTime<Utc>>,
    pub terminated_reason: Option<String>,
}

/// Data for creating a new refresh token.
#[derive(Debug, Clone)]
pub struct NewRefreshToken {
    pub user_id: Uuid,
    pub token_hash: String,
    pub token_family: Uuid,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub device_type: Option<String>,
    pub user_agent: Option<String>,
    pub ip_address: Option<IpAddr>,
    pub expires_at: DateTime<Utc>,
}
```

### 5.4 User Repository

```rust
// src/repositories/user_repository.rs

use crate::entities::user::{NewUser, UpdateUser, UserRow};
use crate::error::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<UserRow>, DbError> {
        sqlx::query_as!(UserRow, "SELECT * FROM users WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::from)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<UserRow>, DbError> {
        sqlx::query_as!(UserRow, "SELECT * FROM users WHERE email = lower($1)", email)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::from)
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<UserRow>, DbError> {
        sqlx::query_as!(
            UserRow,
            "SELECT * FROM users WHERE lower(username) = lower($1)",
            username
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }

    pub async fn create(&self, new_user: NewUser) -> Result<UserRow, DbError> {
        sqlx::query_as!(
            UserRow,
            r#"
            INSERT INTO users (username, email, password_hash, status)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
            new_user.username,
            new_user.email,
            new_user.password_hash,
            new_user.status
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    pub async fn update(&self, id: Uuid, update: UpdateUser) -> Result<UserRow, DbError> {
        sqlx::query_as!(
            UserRow,
            r#"
            UPDATE users SET
                username = COALESCE($2, username),
                email = COALESCE($3, email),
                email_verified = COALESCE($4, email_verified),
                password_hash = COALESCE($5, password_hash),
                password_changed_at = CASE WHEN $5 IS NOT NULL THEN NOW() ELSE password_changed_at END,
                two_factor_enabled = COALESCE($6, two_factor_enabled),
                two_factor_secret = COALESCE($7, two_factor_secret),
                two_factor_backup_codes = COALESCE($8, two_factor_backup_codes),
                status = COALESCE($9, status),
                status_reason = COALESCE($10, status_reason),
                status_changed_at = CASE WHEN $9 IS NOT NULL THEN NOW() ELSE status_changed_at END,
                locale = COALESCE($11, locale),
                timezone = COALESCE($12, timezone),
                last_login_at = COALESCE($13, last_login_at),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
            id,
            update.username,
            update.email,
            update.email_verified,
            update.password_hash,
            update.two_factor_enabled,
            update.two_factor_secret,
            update.two_factor_backup_codes,
            update.status,
            update.status_reason,
            update.locale,
            update.timezone,
            update.last_login_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    pub async fn set_email_verified(&self, id: Uuid) -> Result<(), DbError> {
        sqlx::query!(
            r#"
            UPDATE users SET
                email_verified = TRUE,
                email_verified_at = NOW(),
                status = CASE WHEN status = 'pending_verification' THEN 'active' ELSE status END,
                updated_at = NOW()
            WHERE id = $1
            "#,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;
        Ok(())
    }

    pub async fn update_last_login(&self, id: Uuid) -> Result<(), DbError> {
        sqlx::query!(
            "UPDATE users SET last_login_at = NOW(), updated_at = NOW() WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;
        Ok(())
    }
}
```

### 5.5 RBAC Repository

```rust
// src/repositories/rbac_repository.rs

use crate::entities::role::{NewUserRole, PermissionRow, RoleRow, UserRoleRow};
use crate::error::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub struct RbacRepository {
    pool: PgPool,
}

impl RbacRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get all roles for a user (including scoped).
    pub async fn get_user_roles(&self, user_id: Uuid) -> Result<Vec<UserRoleRow>, DbError> {
        sqlx::query_as!(
            UserRoleRow,
            r#"
            SELECT * FROM user_roles
            WHERE user_id = $1
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Get permissions for a role.
    pub async fn get_role_permissions(&self, role_id: Uuid) -> Result<Vec<PermissionRow>, DbError> {
        sqlx::query_as!(
            PermissionRow,
            r#"
            SELECT p.* FROM permissions p
            JOIN role_permissions rp ON rp.permission_id = p.id
            WHERE rp.role_id = $1
            "#,
            role_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Check if user has permission (with optional scope).
    pub async fn user_has_permission(
        &self,
        user_id: Uuid,
        permission_name: &str,
        scope_type: Option<&str>,
        scope_id: Option<Uuid>,
    ) -> Result<bool, DbError> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM user_roles ur
                JOIN role_permissions rp ON rp.role_id = ur.role_id
                JOIN permissions p ON p.id = rp.permission_id
                WHERE ur.user_id = $1
                  AND p.name = $2
                  AND ur.revoked_at IS NULL
                  AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
                  AND (
                      -- Global role (no scope) applies everywhere
                      ur.scope_type IS NULL
                      -- Or scope matches
                      OR (ur.scope_type = $3 AND ur.scope_id = $4)
                  )
            ) as "exists!"
            "#,
            user_id,
            permission_name,
            scope_type,
            scope_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(result)
    }

    /// Assign role to user.
    pub async fn assign_role(&self, assignment: NewUserRole) -> Result<UserRoleRow, DbError> {
        sqlx::query_as!(
            UserRoleRow,
            r#"
            INSERT INTO user_roles (user_id, role_id, scope_type, scope_id, granted_by, reason, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
            assignment.user_id,
            assignment.role_id,
            assignment.scope_type,
            assignment.scope_id,
            assignment.granted_by,
            assignment.reason,
            assignment.expires_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Revoke role from user.
    pub async fn revoke_role(
        &self,
        user_role_id: Uuid,
        revoked_by: Uuid,
        reason: Option<&str>,
    ) -> Result<(), DbError> {
        sqlx::query!(
            r#"
            UPDATE user_roles SET
                revoked_at = NOW(),
                revoked_by = $2,
                revoke_reason = $3
            WHERE id = $1 AND revoked_at IS NULL
            "#,
            user_role_id,
            revoked_by,
            reason
        )
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;
        Ok(())
    }

    /// Get role by name.
    pub async fn get_role_by_name(&self, name: &str) -> Result<Option<RoleRow>, DbError> {
        sqlx::query_as!(RoleRow, "SELECT * FROM roles WHERE name = $1", name)
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::from)
    }

    /// Get default role (assigned to new users).
    pub async fn get_default_role(&self) -> Result<Option<RoleRow>, DbError> {
        sqlx::query_as!(RoleRow, "SELECT * FROM roles WHERE is_default = TRUE LIMIT 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(DbError::from)
    }
}
```

---

## 6. Domain Layer (`portal-domain`)

### 6.1 Auth Service

```rust
// src/services/auth.rs

use crate::entities::User;
use crate::repositories::{UserRepository, RefreshTokenRepository, SessionRepository};
use portal_core::{UserId, AuthError, Email, Username, Password};
use std::sync::Arc;
use tracing::instrument;

pub struct AuthService<UR, RTR, SR>
where
    UR: UserRepository,
    RTR: RefreshTokenRepository,
    SR: SessionRepository,
{
    user_repo: Arc<UR>,
    refresh_token_repo: Arc<RTR>,
    session_repo: Arc<SR>,
    password_hasher: Arc<dyn PasswordHasher>,
    jwt_service: Arc<JwtService>,
    totp_service: Arc<TotpService>,
}

impl<UR, RTR, SR> AuthService<UR, RTR, SR>
where
    UR: UserRepository,
    RTR: RefreshTokenRepository,
    SR: SessionRepository,
{
    /// Register a new user with email/password.
    #[instrument(skip(self, password))]
    pub async fn register(
        &self,
        username: Username,
        email: Email,
        password: Password,
    ) -> Result<User, AuthError> {
        // Check if email is taken
        if self.user_repo.find_by_email(email.as_str()).await?.is_some() {
            return Err(AuthError::EmailTaken);
        }

        // Check if username is taken
        if self.user_repo.find_by_username(username.as_str()).await?.is_some() {
            return Err(AuthError::UsernameTaken);
        }

        // Hash password
        let password_hash = self.password_hasher.hash(password.as_str()).await?;

        // Create user
        let user = self.user_repo.create(CreateUser {
            username: username.into(),
            email: email.into(),
            password_hash: Some(password_hash),
            status: "pending_verification".to_string(),
        }).await?;

        // Assign default role
        if let Some(default_role) = self.rbac_repo.get_default_role().await? {
            self.rbac_repo.assign_role(NewUserRole {
                user_id: user.id.into(),
                role_id: default_role.id,
                scope_type: None,
                scope_id: None,
                granted_by: None,
                reason: Some("Auto-assigned on registration".to_string()),
                expires_at: None,
            }).await?;
        }

        // TODO: Send verification email

        Ok(user)
    }

    /// Login with email/password.
    #[instrument(skip(self, password))]
    pub async fn login(
        &self,
        email: &str,
        password: &str,
        device_info: DeviceInfo,
    ) -> Result<AuthTokens, AuthError> {
        // Find user
        let user = self.user_repo
            .find_by_email(email)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        // Check account status
        let status: portal_core::UserStatus = user.status.parse()?;
        if !status.can_login() {
            return Err(AuthError::AccountNotActive { status });
        }

        // Verify password
        let password_hash = user.password_hash
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;

        if !self.password_hasher.verify(password, password_hash).await? {
            return Err(AuthError::InvalidCredentials);
        }

        // Check if 2FA is enabled
        if user.two_factor_enabled {
            let challenge_token = self.jwt_service.create_2fa_challenge(user.id)?;
            return Err(AuthError::TwoFactorRequired { challenge_token });
        }

        // Issue tokens
        self.issue_tokens(user.id.into(), device_info).await
    }

    /// Complete 2FA verification.
    #[instrument(skip(self, code))]
    pub async fn verify_2fa(
        &self,
        challenge_token: &str,
        code: &str,
        device_info: DeviceInfo,
    ) -> Result<AuthTokens, AuthError> {
        // Validate challenge token
        let claims = self.jwt_service.validate_2fa_challenge(challenge_token)?;
        let user_id: UserId = claims.sub.parse()?;

        // Get user
        let user = self.user_repo
            .find_by_id(user_id.into())
            .await?
            .ok_or(AuthError::AccountNotFound)?;

        // Verify TOTP code
        let secret = user.two_factor_secret
            .as_ref()
            .ok_or(AuthError::TwoFactorNotEnabled)?;

        if !self.totp_service.verify(secret, code)? {
            // Check backup codes
            if !self.verify_backup_code(&user, code).await? {
                return Err(AuthError::InvalidTwoFactorCode);
            }
        }

        // Issue tokens
        self.issue_tokens(user_id, device_info).await
    }

    /// Refresh access token.
    #[instrument(skip(self))]
    pub async fn refresh_tokens(
        &self,
        refresh_token: &str,
    ) -> Result<AuthTokens, AuthError> {
        // Validate and decode refresh token
        let claims = self.jwt_service.validate_refresh_token(refresh_token)?;
        let token_id: uuid::Uuid = claims.jti.parse()?;

        // Find token in database
        let token_row = self.refresh_token_repo
            .find_by_id(token_id)
            .await?
            .ok_or(AuthError::InvalidToken)?;

        // Check if revoked
        if token_row.revoked_at.is_some() {
            // Possible token theft - revoke entire family
            self.refresh_token_repo.revoke_family(token_row.token_family).await?;
            return Err(AuthError::InvalidToken);
        }

        // Check expiration
        if token_row.expires_at < chrono::Utc::now() {
            return Err(AuthError::TokenExpired);
        }

        // Rotate refresh token (issue new one, mark old as replaced)
        let new_tokens = self.issue_tokens_with_family(
            token_row.user_id.into(),
            token_row.token_family,
            DeviceInfo::from_row(&token_row),
        ).await?;

        // Mark old token as replaced
        self.refresh_token_repo.mark_replaced(token_id, new_tokens.refresh_token_id).await?;

        Ok(new_tokens)
    }

    /// Logout (revoke refresh token).
    #[instrument(skip(self))]
    pub async fn logout(&self, refresh_token: &str) -> Result<(), AuthError> {
        let claims = self.jwt_service.validate_refresh_token(refresh_token)?;
        let token_id: uuid::Uuid = claims.jti.parse()?;

        self.refresh_token_repo.revoke(token_id, "logout").await?;
        Ok(())
    }

    /// Logout from all devices.
    #[instrument(skip(self))]
    pub async fn logout_all(&self, user_id: UserId) -> Result<(), AuthError> {
        self.refresh_token_repo.revoke_all_for_user(user_id.into()).await?;
        Ok(())
    }

    /// Enable 2FA.
    #[instrument(skip(self))]
    pub async fn enable_2fa(&self, user_id: UserId) -> Result<TwoFactorSetup, AuthError> {
        let user = self.user_repo
            .find_by_id(user_id.into())
            .await?
            .ok_or(AuthError::AccountNotFound)?;

        if user.two_factor_enabled {
            return Err(AuthError::TwoFactorAlreadyEnabled);
        }

        // Generate secret
        let secret = self.totp_service.generate_secret();
        let provisioning_uri = self.totp_service.provisioning_uri(&user.email, &secret);

        // Store secret (not yet enabled)
        self.user_repo.update(user_id.into(), UpdateUser {
            two_factor_secret: Some(secret.clone()),
            ..Default::default()
        }).await?;

        Ok(TwoFactorSetup {
            secret,
            provisioning_uri,
            backup_codes: Vec::new(), // Generated on confirmation
        })
    }

    /// Confirm 2FA setup with verification code.
    #[instrument(skip(self, code))]
    pub async fn confirm_2fa(
        &self,
        user_id: UserId,
        code: &str,
    ) -> Result<Vec<String>, AuthError> {
        let user = self.user_repo
            .find_by_id(user_id.into())
            .await?
            .ok_or(AuthError::AccountNotFound)?;

        let secret = user.two_factor_secret
            .as_ref()
            .ok_or(AuthError::TwoFactorNotEnabled)?;

        // Verify code
        if !self.totp_service.verify(secret, code)? {
            return Err(AuthError::InvalidTwoFactorCode);
        }

        // Generate backup codes
        let backup_codes = self.generate_backup_codes();
        let hashed_codes: Vec<String> = backup_codes.iter()
            .map(|c| self.password_hasher.hash_sync(c))
            .collect();

        // Enable 2FA
        self.user_repo.update(user_id.into(), UpdateUser {
            two_factor_enabled: Some(true),
            two_factor_backup_codes: Some(serde_json::to_value(&hashed_codes)?),
            ..Default::default()
        }).await?;

        Ok(backup_codes)
    }

    /// Disable 2FA.
    #[instrument(skip(self, password))]
    pub async fn disable_2fa(
        &self,
        user_id: UserId,
        password: &str,
    ) -> Result<(), AuthError> {
        let user = self.user_repo
            .find_by_id(user_id.into())
            .await?
            .ok_or(AuthError::AccountNotFound)?;

        // Verify password
        let password_hash = user.password_hash
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;

        if !self.password_hasher.verify(password, password_hash).await? {
            return Err(AuthError::InvalidCredentials);
        }

        // Disable 2FA
        self.user_repo.update(user_id.into(), UpdateUser {
            two_factor_enabled: Some(false),
            two_factor_secret: None,
            two_factor_backup_codes: None,
            ..Default::default()
        }).await?;

        Ok(())
    }

    // Helper: Issue tokens
    async fn issue_tokens(
        &self,
        user_id: UserId,
        device_info: DeviceInfo,
    ) -> Result<AuthTokens, AuthError> {
        let family = uuid::Uuid::new_v4();
        self.issue_tokens_with_family(user_id, family, device_info).await
    }

    async fn issue_tokens_with_family(
        &self,
        user_id: UserId,
        family: uuid::Uuid,
        device_info: DeviceInfo,
    ) -> Result<AuthTokens, AuthError> {
        // Get user roles for access token
        let roles = self.rbac_repo.get_user_roles(user_id.into()).await?;
        let role_names: Vec<String> = roles.iter().map(|r| r.role_name.clone()).collect();

        // Create access token
        let access_token = self.jwt_service.create_access_token(user_id, &role_names)?;

        // Create refresh token
        let refresh_token_id = uuid::Uuid::new_v4();
        let refresh_token = self.jwt_service.create_refresh_token(user_id, refresh_token_id, family)?;

        // Store refresh token in database
        self.refresh_token_repo.create(NewRefreshToken {
            id: refresh_token_id,
            user_id: user_id.into(),
            token_hash: sha256_hash(&refresh_token),
            token_family: family,
            device_id: device_info.device_id,
            device_name: device_info.device_name,
            device_type: device_info.device_type,
            user_agent: device_info.user_agent,
            ip_address: device_info.ip_address,
            expires_at: chrono::Utc::now() + chrono::Duration::days(7),
        }).await?;

        // Update last login
        self.user_repo.update_last_login(user_id.into()).await?;

        Ok(AuthTokens {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: 900, // 15 minutes
        })
    }
}
```

### 6.2 RBAC Service

```rust
// src/services/rbac.rs

use crate::repositories::RbacRepository;
use portal_core::{UserId, RoleId, PermissionScope, RbacError};
use std::sync::Arc;
use tracing::instrument;

pub struct RbacService<RR>
where
    RR: RbacRepository,
{
    rbac_repo: Arc<RR>,
    permission_cache: Arc<PermissionCache>,
}

impl<RR> RbacService<RR>
where
    RR: RbacRepository,
{
    /// Check if user has permission.
    #[instrument(skip(self))]
    pub async fn has_permission(
        &self,
        user_id: UserId,
        permission: &str,
        scope: Option<PermissionScope>,
    ) -> Result<bool, RbacError> {
        // Check cache first
        let cache_key = format!("{}:{}:{:?}", user_id, permission, scope);
        if let Some(cached) = self.permission_cache.get(&cache_key).await {
            return Ok(cached);
        }

        // Query database
        let has_perm = self.rbac_repo.user_has_permission(
            user_id.into(),
            permission,
            scope.as_ref().map(|s| s.scope_type.to_string()).as_deref(),
            scope.as_ref().map(|s| s.scope_id),
        ).await?;

        // Cache result
        self.permission_cache.set(&cache_key, has_perm, Duration::from_secs(60)).await;

        Ok(has_perm)
    }

    /// Require permission or return error.
    #[instrument(skip(self))]
    pub async fn require_permission(
        &self,
        user_id: UserId,
        permission: &str,
        scope: Option<PermissionScope>,
    ) -> Result<(), RbacError> {
        if !self.has_permission(user_id, permission, scope).await? {
            return Err(RbacError::PermissionDenied {
                permission: permission.to_string(),
            });
        }
        Ok(())
    }

    /// Assign role to user.
    #[instrument(skip(self))]
    pub async fn assign_role(
        &self,
        user_id: UserId,
        role_name: &str,
        scope: Option<PermissionScope>,
        granted_by: UserId,
        reason: Option<String>,
    ) -> Result<(), RbacError> {
        let role = self.rbac_repo
            .get_role_by_name(role_name)
            .await?
            .ok_or_else(|| RbacError::RoleNotFound(role_name.to_string()))?;

        // System roles require admin permission
        if role.is_system {
            self.require_permission(granted_by, "admin.users.manage", None).await?;
        }

        self.rbac_repo.assign_role(NewUserRole {
            user_id: user_id.into(),
            role_id: role.id,
            scope_type: scope.as_ref().map(|s| s.scope_type.to_string()),
            scope_id: scope.as_ref().map(|s| s.scope_id),
            granted_by: Some(granted_by.into()),
            reason,
            expires_at: None,
        }).await?;

        // Invalidate cache
        self.permission_cache.invalidate_user(user_id).await;

        Ok(())
    }

    /// Revoke role from user.
    #[instrument(skip(self))]
    pub async fn revoke_role(
        &self,
        user_role_id: uuid::Uuid,
        revoked_by: UserId,
        reason: Option<String>,
    ) -> Result<(), RbacError> {
        // Get the assignment to check for system role
        let assignment = self.rbac_repo.get_user_role(user_role_id).await?;

        // Cannot revoke system roles without admin
        let role = self.rbac_repo.get_role_by_id(assignment.role_id).await?;
        if role.is_system {
            self.require_permission(revoked_by, "admin.users.manage", None).await?;
        }

        self.rbac_repo.revoke_role(user_role_id, revoked_by.into(), reason.as_deref()).await?;

        // Invalidate cache
        self.permission_cache.invalidate_user(assignment.user_id.into()).await;

        Ok(())
    }

    /// Get all roles for a user.
    #[instrument(skip(self))]
    pub async fn get_user_roles(&self, user_id: UserId) -> Result<Vec<UserRole>, RbacError> {
        let rows = self.rbac_repo.get_user_roles(user_id.into()).await?;

        let mut roles = Vec::with_capacity(rows.len());
        for row in rows {
            let role = self.rbac_repo.get_role_by_id(row.role_id).await?;
            roles.push(UserRole {
                id: row.id.into(),
                role,
                scope_type: row.scope_type,
                scope_id: row.scope_id,
                granted_at: row.granted_at,
                expires_at: row.expires_at,
            });
        }

        Ok(roles)
    }
}
```

### 6.3 Ban Service

```rust
// src/services/ban.rs

use crate::repositories::{BanRepository, UserRepository};
use portal_core::{UserId, BanId, BanType, BanError, PermissionScope};
use std::sync::Arc;
use tracing::instrument;

pub struct BanService<BR, UR>
where
    BR: BanRepository,
    UR: UserRepository,
{
    ban_repo: Arc<BR>,
    user_repo: Arc<UR>,
    saga_executor: Arc<SagaExecutor>,
}

impl<BR, UR> BanService<BR, UR>
where
    BR: BanRepository,
    UR: UserRepository,
{
    /// Create a ban.
    #[instrument(skip(self))]
    pub async fn create_ban(
        &self,
        user_id: UserId,
        banned_by: UserId,
        ban_type: BanType,
        scope: Option<PermissionScope>,
        reason: String,
        duration: Option<chrono::Duration>,
    ) -> Result<Ban, BanError> {
        // Check if user is platform admin (cannot ban admins)
        if self.is_platform_admin(user_id).await? {
            return Err(BanError::CannotBanAdmin);
        }

        // Calculate end time
        let ends_at = duration.map(|d| chrono::Utc::now() + d);

        // Create ban record
        let ban = self.ban_repo.create(NewBan {
            user_id: user_id.into(),
            banned_by: banned_by.into(),
            ban_type: ban_type.to_string(),
            scope_type: scope.as_ref().map(|s| s.scope_type.to_string()),
            scope_id: scope.as_ref().map(|s| s.scope_id),
            reason,
            ends_at,
        }).await?;

        // Execute ban saga for side effects
        if ban_type == BanType::Platform {
            self.saga_executor.execute(BanPlayerSaga {
                user_id,
                ban_id: ban.id.into(),
            }).await?;
        }

        Ok(ban)
    }

    /// Check if user is banned.
    #[instrument(skip(self))]
    pub async fn is_banned(
        &self,
        user_id: UserId,
        ban_type: Option<BanType>,
        scope: Option<PermissionScope>,
    ) -> Result<Option<Ban>, BanError> {
        self.ban_repo.get_active_ban(
            user_id.into(),
            ban_type.map(|t| t.to_string()).as_deref(),
            scope.as_ref().map(|s| s.scope_type.to_string()).as_deref(),
            scope.as_ref().map(|s| s.scope_id),
        ).await
    }

    /// Lift a ban.
    #[instrument(skip(self))]
    pub async fn lift_ban(
        &self,
        ban_id: BanId,
        lifted_by: UserId,
        reason: String,
    ) -> Result<(), BanError> {
        let ban = self.ban_repo
            .find_by_id(ban_id.into())
            .await?
            .ok_or(BanError::NotFound)?;

        if ban.lifted_at.is_some() {
            return Err(BanError::AlreadyLifted);
        }

        self.ban_repo.lift(ban_id.into(), lifted_by.into(), &reason).await?;

        // If platform ban, update user status
        if ban.ban_type == "platform" {
            self.user_repo.update(ban.user_id.into(), UpdateUser {
                status: Some("active".to_string()),
                status_reason: Some(format!("Ban lifted: {}", reason)),
                ..Default::default()
            }).await?;
        }

        Ok(())
    }

    /// Submit appeal.
    #[instrument(skip(self))]
    pub async fn submit_appeal(
        &self,
        ban_id: BanId,
        appeal_text: String,
    ) -> Result<(), BanError> {
        let ban = self.ban_repo
            .find_by_id(ban_id.into())
            .await?
            .ok_or(BanError::NotFound)?;

        if ban.appeal_status.is_some() {
            return Err(BanError::AppealExists);
        }

        // Cannot appeal permanent bans (policy decision)
        if ban.ends_at.is_none() {
            return Err(BanError::CannotAppealPermanent);
        }

        self.ban_repo.submit_appeal(ban_id.into(), &appeal_text).await?;

        Ok(())
    }
}
```

---

## 7. API Layer (`portal-api`)

### 7.1 Auth DTOs

```rust
// src/dto/auth.rs

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub accept_terms: bool,
    pub captcha_token: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthTokensResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Verify2faRequest {
    pub challenge_token: String,
    pub code: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TwoFactorSetupResponse {
    pub secret: String,
    pub provisioning_uri: String,
    pub qr_code_data_url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Confirm2faRequest {
    pub code: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BackupCodesResponse {
    pub backup_codes: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct Disable2faRequest {
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordResetConfirmRequest {
    pub token: String,
    pub new_password: String,
}
```

### 7.2 Auth Handlers

```rust
// src/handlers/auth.rs

use crate::dto::auth::*;
use crate::extractors::{AuthenticatedUser, ValidatedJson, DeviceInfo};
use crate::state::AppState;
use crate::error::ApiError;
use axum::{extract::State, Json};

/// Register a new user.
#[utoipa::path(
    post,
    path = "/v1/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User created", body = AuthTokensResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Email/username taken"),
    ),
    tag = "Authentication"
)]
pub async fn register(
    State(state): State<AppState>,
    device_info: DeviceInfo,
    ValidatedJson(req): ValidatedJson<RegisterRequest>,
) -> Result<Json<AuthTokensResponse>, ApiError> {
    // Validate terms acceptance
    if !req.accept_terms {
        return Err(ApiError::bad_request("Must accept terms of service"));
    }

    let username = portal_core::Username::new(req.username)?;
    let email = portal_core::Email::new(req.email)?;
    let password = portal_core::Password::new(req.password)?;

    let user = state.auth_service
        .register(username, email, password)
        .await?;

    // Auto-login after registration
    let tokens = state.auth_service
        .issue_tokens(user.id, device_info.into())
        .await?;

    Ok(Json(AuthTokensResponse::from(tokens)))
}

/// Login with email/password.
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = AuthTokensResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 403, description = "2FA required"),
    ),
    tag = "Authentication"
)]
pub async fn login(
    State(state): State<AppState>,
    device_info: DeviceInfo,
    ValidatedJson(req): ValidatedJson<LoginRequest>,
) -> Result<Json<AuthTokensResponse>, ApiError> {
    match state.auth_service.login(&req.email, &req.password, device_info.into()).await {
        Ok(tokens) => Ok(Json(AuthTokensResponse::from(tokens))),
        Err(AuthError::TwoFactorRequired { challenge_token }) => {
            Err(ApiError::two_factor_required(challenge_token))
        }
        Err(e) => Err(e.into()),
    }
}

/// Verify 2FA code.
#[utoipa::path(
    post,
    path = "/v1/auth/2fa/verify",
    request_body = Verify2faRequest,
    responses(
        (status = 200, description = "2FA verified", body = AuthTokensResponse),
        (status = 401, description = "Invalid code"),
    ),
    tag = "Authentication"
)]
pub async fn verify_2fa(
    State(state): State<AppState>,
    device_info: DeviceInfo,
    ValidatedJson(req): ValidatedJson<Verify2faRequest>,
) -> Result<Json<AuthTokensResponse>, ApiError> {
    let tokens = state.auth_service
        .verify_2fa(&req.challenge_token, &req.code, device_info.into())
        .await?;

    Ok(Json(AuthTokensResponse::from(tokens)))
}

/// Refresh access token.
#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, description = "Tokens refreshed", body = AuthTokensResponse),
        (status = 401, description = "Invalid/expired token"),
    ),
    tag = "Authentication"
)]
pub async fn refresh_token(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<RefreshTokenRequest>,
) -> Result<Json<AuthTokensResponse>, ApiError> {
    let tokens = state.auth_service
        .refresh_tokens(&req.refresh_token)
        .await?;

    Ok(Json(AuthTokensResponse::from(tokens)))
}

/// Logout (revoke refresh token).
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    request_body = RefreshTokenRequest,
    responses(
        (status = 204, description = "Logged out"),
    ),
    security(("bearer_auth" = [])),
    tag = "Authentication"
)]
pub async fn logout(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<RefreshTokenRequest>,
) -> Result<(), ApiError> {
    state.auth_service.logout(&req.refresh_token).await?;
    Ok(())
}

/// Enable 2FA.
#[utoipa::path(
    post,
    path = "/v1/auth/2fa/enroll",
    responses(
        (status = 200, description = "2FA setup initiated", body = TwoFactorSetupResponse),
        (status = 409, description = "2FA already enabled"),
    ),
    security(("bearer_auth" = [])),
    tag = "Authentication"
)]
pub async fn enable_2fa(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<TwoFactorSetupResponse>, ApiError> {
    let setup = state.auth_service.enable_2fa(auth.user_id).await?;

    Ok(Json(TwoFactorSetupResponse {
        secret: setup.secret,
        provisioning_uri: setup.provisioning_uri,
        qr_code_data_url: generate_qr_code(&setup.provisioning_uri)?,
    }))
}

/// Confirm 2FA setup.
#[utoipa::path(
    post,
    path = "/v1/auth/2fa/confirm",
    request_body = Confirm2faRequest,
    responses(
        (status = 200, description = "2FA enabled", body = BackupCodesResponse),
        (status = 401, description = "Invalid code"),
    ),
    security(("bearer_auth" = [])),
    tag = "Authentication"
)]
pub async fn confirm_2fa(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<Confirm2faRequest>,
) -> Result<Json<BackupCodesResponse>, ApiError> {
    let backup_codes = state.auth_service
        .confirm_2fa(auth.user_id, &req.code)
        .await?;

    Ok(Json(BackupCodesResponse { backup_codes }))
}

/// Disable 2FA.
#[utoipa::path(
    post,
    path = "/v1/auth/2fa/disable",
    request_body = Disable2faRequest,
    responses(
        (status = 204, description = "2FA disabled"),
        (status = 401, description = "Invalid password"),
    ),
    security(("bearer_auth" = [])),
    tag = "Authentication"
)]
pub async fn disable_2fa(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<Disable2faRequest>,
) -> Result<(), ApiError> {
    state.auth_service.disable_2fa(auth.user_id, &req.password).await?;
    Ok(())
}
```

### 7.3 Auth Routes

```rust
// src/routes/auth.rs

use crate::handlers::auth;
use axum::{routing::post, Router};
use crate::state::AppState;

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        // Registration & Login
        .route("/v1/auth/register", post(auth::register))
        .route("/v1/auth/login", post(auth::login))
        .route("/v1/auth/refresh", post(auth::refresh_token))
        .route("/v1/auth/logout", post(auth::logout))

        // 2FA
        .route("/v1/auth/2fa/verify", post(auth::verify_2fa))
        .route("/v1/auth/2fa/enroll", post(auth::enable_2fa))
        .route("/v1/auth/2fa/confirm", post(auth::confirm_2fa))
        .route("/v1/auth/2fa/disable", post(auth::disable_2fa))

        // Password Reset
        .route("/v1/auth/password/reset-request", post(auth::request_password_reset))
        .route("/v1/auth/password/reset", post(auth::reset_password))

        // OAuth
        .route("/v1/auth/oauth/:provider", post(auth::oauth_redirect))
        .route("/v1/auth/oauth/:provider/callback", post(auth::oauth_callback))
}
```

### 7.4 RBAC Middleware

```rust
// src/middleware/rbac.rs

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;
use crate::error::ApiError;

/// Middleware to require a specific permission.
pub fn require_permission(permission: &'static str) -> impl Fn(
    State<AppState>,
    AuthenticatedUser,
    Request,
    Next,
) -> impl Future<Output = Result<Response, ApiError>> + Clone {
    move |State(state), auth, request, next| {
        let permission = permission;
        async move {
            // Check permission
            if !state.rbac_service
                .has_permission(auth.user_id, permission, None)
                .await?
            {
                return Err(ApiError::forbidden(format!(
                    "Missing permission: {}",
                    permission
                )));
            }

            Ok(next.run(request).await)
        }
    }
}

/// Middleware to require permission with scope from path.
pub fn require_scoped_permission(
    permission: &'static str,
    scope_type: &'static str,
    scope_param: &'static str,
) -> impl Fn(
    State<AppState>,
    AuthenticatedUser,
    Request,
    Next,
) -> impl Future<Output = Result<Response, ApiError>> + Clone {
    move |State(state), auth, request, next| {
        let permission = permission;
        let scope_type = scope_type;
        let scope_param = scope_param;

        async move {
            // Extract scope ID from path
            let scope_id: uuid::Uuid = request
                .extensions()
                .get::<axum::extract::Path<std::collections::HashMap<String, String>>>()
                .and_then(|p| p.get(scope_param))
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| ApiError::bad_request("Missing scope parameter"))?;

            let scope = match scope_type {
                "team" => Some(PermissionScope::team(scope_id)),
                "tournament" => Some(PermissionScope::tournament(scope_id)),
                "league" => Some(PermissionScope::league(scope_id)),
                _ => None,
            };

            if !state.rbac_service
                .has_permission(auth.user_id, permission, scope)
                .await?
            {
                return Err(ApiError::forbidden(format!(
                    "Missing permission: {} for {:?}",
                    permission, scope_type
                )));
            }

            Ok(next.run(request).await)
        }
    }
}
```

---

## 8. Security Considerations

### 8.1 Password Security

- **Hashing**: Argon2id with recommended parameters
- **Minimum Requirements**: 8 chars, uppercase, lowercase, digit
- **Rate Limiting**: 5 failed attempts per 15 minutes
- **Breach Detection**: Check against HaveIBeenPwned (optional)

### 8.2 Token Security

- **Access Tokens**: Short-lived (15 min), stateless JWT
- **Refresh Tokens**: Long-lived (7 days), stored in DB, rotated on use
- **Token Rotation**: New refresh token on each use, old marked replaced
- **Family Tracking**: Detect token theft via reuse of replaced token

### 8.3 2FA Security

- **TOTP**: RFC 6238 compliant, 30-second windows
- **Backup Codes**: 10 single-use codes, hashed in DB
- **Secret Storage**: Encrypted at rest
- **Recovery**: Backup codes only, no SMS fallback

### 8.4 Session Security

- **Device Tracking**: Fingerprint, user agent, IP
- **Geo-IP**: Detect unusual locations
- **Multi-Session**: Users can view/revoke all sessions
- **Idle Timeout**: Sessions expire after 30 days inactivity

### 8.5 Rate Limiting

| Endpoint | Limit |
|----------|-------|
| `/auth/login` | 5/min per IP |
| `/auth/register` | 3/min per IP |
| `/auth/password/reset-request` | 3/hour per email |
| `/auth/2fa/verify` | 5/min per challenge |

### 8.6 Audit Logging

Events logged to `audit_logs` table:
- Login success/failure
- Password changes
- 2FA enable/disable
- Role assignments/revocations
- Ban create/lift/appeal
- Permission denials

---

## 9. Implementation Checklist

### 9.1 Core Layer
- [ ] Add `UserId`, `RoleId`, `PermissionId`, `RefreshTokenId`, `SessionId`, `BanId` to `ids.rs`
- [ ] Create `src/types/auth.rs` with auth enums and validation types
- [ ] Create `src/types/rbac.rs` with RBAC types
- [ ] Create `src/types/ban.rs` with ban types
- [ ] Add auth/rbac/ban errors to `src/error.rs`

### 9.2 Database Layer
- [ ] Create `src/entities/user.rs` (extend existing)
- [ ] Create `src/entities/role.rs`
- [ ] Create `src/entities/session.rs`
- [ ] Create `src/entities/ban.rs`
- [ ] Create `src/repositories/user_repository.rs`
- [ ] Create `src/repositories/rbac_repository.rs`
- [ ] Create `src/repositories/session_repository.rs`
- [ ] Create `src/repositories/ban_repository.rs`

### 9.3 Domain Layer
- [ ] Create `src/entities/user.rs` domain entity
- [ ] Create `src/services/auth.rs` service
- [ ] Create `src/services/rbac.rs` service
- [ ] Create `src/services/ban.rs` service
- [ ] Create `src/services/jwt.rs` JWT service
- [ ] Create `src/services/totp.rs` TOTP service
- [ ] Create `src/services/password.rs` password hasher

### 9.4 API Layer
- [ ] Create `src/dto/auth.rs` DTOs
- [ ] Create `src/dto/rbac.rs` DTOs
- [ ] Create `src/handlers/auth.rs` handlers
- [ ] Create `src/handlers/rbac.rs` handlers
- [ ] Create `src/routes/auth.rs` routes
- [ ] Create `src/middleware/auth.rs` JWT validation
- [ ] Create `src/middleware/rbac.rs` permission middleware
- [ ] Create `src/extractors/auth.rs` AuthenticatedUser extractor

### 9.5 Dependencies
- [ ] Add `argon2` for password hashing
- [ ] Add `jsonwebtoken` for JWT
- [ ] Add `totp-rs` for 2FA
- [ ] Add `tower-governor` for rate limiting

---

*End of Auth & RBAC Design Document*
