//! Refresh token domain entity.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A refresh token for silent access token renewal.
#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl RefreshToken {
    /// Check if this refresh token is currently valid (not expired and not revoked).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.revoked_at.is_none() && self.expires_at > Utc::now()
    }

    /// Check if the token has been revoked.
    #[must_use]
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    /// Check if the token has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }
}
