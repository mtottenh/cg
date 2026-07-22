//! API key repository trait.

use crate::entities::api_key::ApiKey;
use async_trait::async_trait;
use portal_core::{ApiKeyId, DomainError};

/// Repository trait for API key operations.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Find an API key by its hash.
    async fn find_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, DomainError>;

    /// Find an API key by ID.
    async fn find_by_id(&self, id: ApiKeyId) -> Result<Option<ApiKey>, DomainError>;

    /// Create a new API key.
    async fn create(&self, cmd: CreateApiKey) -> Result<ApiKey, DomainError>;

    /// Update the `last_used_at` timestamp.
    async fn touch(&self, id: ApiKeyId) -> Result<(), DomainError>;

    /// Deactivate an API key.
    async fn deactivate(&self, id: ApiKeyId) -> Result<(), DomainError>;
}

/// Data for creating a new API key.
#[derive(Debug, Clone)]
pub struct CreateApiKey {
    pub service_name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub permissions: Vec<String>,
}
