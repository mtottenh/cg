//! API key domain entity.

use chrono::{DateTime, Utc};
use portal_core::{ApiKeyId, UserId};

/// An API key for service-to-service authentication.
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub id: ApiKeyId,
    pub service_name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub permissions: Vec<String>,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_by: Option<UserId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApiKey {
    /// Check if the key is currently valid (active and not expired).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.is_active
            && self
                .expires_at
                .map_or(true, |exp| exp > Utc::now())
    }

    /// Check if this key has a specific permission.
    #[must_use]
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }
}
