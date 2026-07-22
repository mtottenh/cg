//! System settings repository trait.
//!
//! Small JSONB key-value store for runtime-togglable platform settings
//! (`system_settings` table). Keys are well-known constants owned by the
//! service layer ([`crate::services::system_settings`]).

use async_trait::async_trait;
use portal_core::DomainError;

/// Repository for the `system_settings` key-value store.
#[async_trait]
pub trait SystemSettingsRepository: Send + Sync {
    /// Get a setting value by key. `None` if the key has never been set.
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, DomainError>;

    /// Upsert a setting value.
    async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), DomainError>;
}
