//! User builder for tests.

use chrono::{DateTime, Utc};
use fake::faker::internet::en::{FreeEmail, Username};
use fake::Fake;
use portal_core::UserId;
use portal_db::entities::UserRow;
use portal_db::DbPool;
use uuid::Uuid;

/// Builder for creating test users.
#[derive(Debug, Clone)]
pub struct UserBuilder {
    id: Option<Uuid>,
    username: Option<String>,
    email: Option<String>,
    email_verified: bool,
    password_hash: Option<String>,
    status: String,
    two_factor_enabled: bool,
}

impl Default for UserBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl UserBuilder {
    /// Create a new user builder with random defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            username: None,
            email: None,
            email_verified: true,
            password_hash: Some("$argon2id$v=19$m=16,t=2,p=1$dGVzdHNhbHQ$hash".to_string()),
            status: "active".to_string(),
            two_factor_enabled: false,
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the username.
    #[must_use]
    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// Set the email.
    #[must_use]
    pub fn email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Set email verification status.
    #[must_use]
    pub fn email_verified(mut self, verified: bool) -> Self {
        self.email_verified = verified;
        self
    }

    /// Set as unverified user.
    #[must_use]
    pub fn unverified(mut self) -> Self {
        self.email_verified = false;
        self.status = "pending_verification".to_string();
        self
    }

    /// Set as banned user.
    #[must_use]
    pub fn banned(mut self) -> Self {
        self.status = "banned".to_string();
        self
    }

    /// Set as suspended user.
    #[must_use]
    pub fn suspended(mut self) -> Self {
        self.status = "suspended".to_string();
        self
    }

    /// Enable 2FA.
    #[must_use]
    pub fn with_2fa(mut self) -> Self {
        self.two_factor_enabled = true;
        self
    }

    /// Build an in-memory user (not persisted).
    #[must_use]
    pub fn build(self) -> UserRow {
        let now = Utc::now();
        UserRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            username: self.username.unwrap_or_else(|| Username().fake()),
            email: self.email.unwrap_or_else(|| FreeEmail().fake()),
            email_verified: self.email_verified,
            email_verified_at: if self.email_verified { Some(now) } else { None },
            password_hash: self.password_hash,
            password_changed_at: None,
            two_factor_enabled: self.two_factor_enabled,
            two_factor_secret: None,
            two_factor_backup_codes: None,
            status: self.status,
            status_reason: None,
            status_changed_at: None,
            locale: Some("en-US".to_string()),
            timezone: Some("UTC".to_string()),
            created_at: now,
            updated_at: now,
            last_login_at: None,
        }
    }

    /// Build and persist the user to the database.
    pub async fn build_persisted(self, pool: &DbPool) -> UserRow {
        let user = self.build();

        sqlx::query_as::<_, UserRow>(
            r#"
            INSERT INTO users (id, username, email, email_verified, email_verified_at,
                password_hash, status, two_factor_enabled, locale, timezone)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(user.email_verified)
        .bind(user.email_verified_at)
        .bind(&user.password_hash)
        .bind(&user.status)
        .bind(user.two_factor_enabled)
        .bind(&user.locale)
        .bind(&user.timezone)
        .fetch_one(pool)
        .await
        .expect("Failed to create test user")
    }
}
