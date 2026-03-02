//! API key authentication extractor for service-to-service calls.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use portal_core::ApiKeyId;
use portal_domain::repositories::api_key::ApiKeyRepository;
use sha2::{Digest, Sha256};

/// Authenticated service extracted from `X-API-Key` header.
///
/// Used for bot and service endpoints under `/v1/internal/...`.
#[derive(Debug, Clone)]
pub struct AuthenticatedService {
    /// The API key's database ID.
    pub api_key_id: ApiKeyId,
    /// The service name (e.g., "cs2-poller", "cs2-enricher").
    pub service_name: String,
    /// Permissions granted to this key.
    pub permissions: Vec<String>,
}

impl AuthenticatedService {
    /// Check if this service has a required permission. Returns `ApiError::Forbidden` if not.
    pub fn require_permission(&self, perm: &str) -> Result<(), ApiError> {
        if self.permissions.iter().any(|p| p == perm) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "Service '{}' lacks required permission: {perm}",
                self.service_name
            )))
        }
    }
}

/// Hash a raw API key with SHA-256 (hex-encoded).
pub fn hash_api_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}

impl<S> FromRequestParts<S> for AuthenticatedService
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // Extract X-API-Key header
        let raw_key = parts
            .headers
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("Missing X-API-Key header"))?;

        // Hash the key and look it up
        let key_hash = hash_api_key(raw_key);
        let api_key = app_state
            .api_key_repo
            .find_by_hash(&key_hash)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .ok_or_else(|| ApiError::unauthorized("Invalid API key"))?;

        // Check validity
        if !api_key.is_valid() {
            return Err(ApiError::unauthorized("API key is inactive or expired"));
        }

        // Touch last_used_at (fire-and-forget)
        let repo = app_state.api_key_repo.clone();
        let key_id = api_key.id;
        tokio::spawn(async move {
            let _ = repo.touch(key_id).await;
        });

        Ok(Self {
            api_key_id: api_key.id,
            service_name: api_key.service_name,
            permissions: api_key.permissions,
        })
    }
}
