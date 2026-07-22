//! System settings service.
//!
//! Typed accessors over the JSONB `system_settings` store. Well-known keys
//! live here as constants so call sites never pass raw strings.

use std::sync::Arc;

use portal_core::DomainError;
use tracing::instrument;

use crate::repositories::system_settings::SystemSettingsRepository;

/// Kill-switch for the demo→match auto-link pass at stats ingestion and the
/// admin backfill endpoint. Defaults to enabled when unset.
pub const DEMO_AUTO_LINK_ENABLED: &str = "demo_auto_link_enabled";

/// Service for reading and writing system settings.
#[derive(Clone)]
pub struct SystemSettingsService<R>
where
    R: SystemSettingsRepository,
{
    repo: Arc<R>,
}

impl<R> SystemSettingsService<R>
where
    R: SystemSettingsRepository,
{
    /// Create a new system settings service.
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    /// Get a boolean setting, falling back to `default` when the key is
    /// unset or not a JSON boolean.
    #[instrument(skip(self))]
    pub async fn get_bool(&self, key: &str, default: bool) -> Result<bool, DomainError> {
        Ok(self
            .repo
            .get(key)
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(default))
    }

    /// Set a boolean setting.
    #[instrument(skip(self))]
    pub async fn set_bool(&self, key: &str, value: bool) -> Result<(), DomainError> {
        self.repo.set(key, serde_json::Value::Bool(value)).await
    }
}
