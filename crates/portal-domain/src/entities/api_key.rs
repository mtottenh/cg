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
    /// Construct an `ApiKey` from its fields plus separately-resolved permissions.
    #[must_use]
    pub fn with_permissions(
        id: ApiKeyId,
        service_name: String,
        key_hash: String,
        key_prefix: String,
        is_active: bool,
        expires_at: Option<DateTime<Utc>>,
        last_used_at: Option<DateTime<Utc>>,
        created_by: Option<UserId>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        permissions: Vec<String>,
    ) -> Self {
        Self {
            id,
            service_name,
            key_hash,
            key_prefix,
            permissions,
            is_active,
            expires_at,
            last_used_at,
            created_by,
            created_at,
            updated_at,
        }
    }

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
