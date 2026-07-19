//! System settings repository adapter.

use crate::DbPool;
use async_trait::async_trait;
use portal_core::DomainError;
use portal_domain::repositories::system_settings::SystemSettingsRepository;

/// Postgres implementation of [`SystemSettingsRepository`].
#[derive(Clone)]
pub struct PgSystemSettingsRepository {
    pool: DbPool,
}

impl PgSystemSettingsRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SystemSettingsRepository for PgSystemSettingsRepository {
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let row: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT value FROM system_settings WHERE key = $1")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::internal(format!("Failed to read setting: {e}")))?;
        Ok(row.map(|(v,)| v))
    }

    async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO system_settings (key, value, updated_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to write setting: {e}")))?;
        Ok(())
    }
}
