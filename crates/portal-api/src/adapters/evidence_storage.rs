//! Evidence storage adapters implementing `EvidenceS3Client`.
//!
//! - `LocalEvidenceStorage` — filesystem-backed for local development
//! - `S3EvidenceStorageAdapter` — wraps `portal_storage::S3Storage` for production
//! - `EvidenceStorageBackend` — enum that dispatches to either at runtime

use async_trait::async_trait;
use portal_core::DomainError;
use portal_domain::services::tournament::{EvidenceS3Client, ObjectPage, StoredObject};
use portal_storage::{S3Config, S3EvidenceClient, S3Storage};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

// =============================================================================
// LOCAL EVIDENCE STORAGE (dev)
// =============================================================================

/// Local filesystem storage adapter for evidence.
///
/// Returns HTTP URLs (via the configured `base_url`) so browsers can PUT/GET
/// files through the Axum server rather than needing `file://` paths.
#[derive(Debug, Clone)]
pub struct LocalEvidenceStorage {
    base_path: PathBuf,
    base_url: String,
}

impl LocalEvidenceStorage {
    /// Create a new local evidence storage.
    ///
    /// * `base_path` — filesystem root for uploads (e.g. `./uploads`)
    /// * `base_url` — HTTP URL prefix (e.g. `http://localhost:3000/uploads`)
    #[must_use]
    pub fn new(base_path: impl Into<PathBuf>, base_url: String) -> Self {
        Self {
            base_path: base_path.into(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Full filesystem path for a bucket/key pair.
    fn get_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.base_path.join(bucket).join(key)
    }

    /// Ensure parent directories exist.
    async fn ensure_dir(&self, path: &Path) -> Result<(), DomainError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| DomainError::Internal(format!("Failed to create directory: {e}")))?;
        }
        Ok(())
    }
}

#[async_trait]
impl EvidenceS3Client for LocalEvidenceStorage {
    async fn presign_put(
        &self,
        bucket: &str,
        key: &str,
        _content_type: &str,
        _ttl: Duration,
    ) -> Result<String, DomainError> {
        let path = self.get_path(bucket, key);
        self.ensure_dir(&path).await?;
        Ok(format!("{}/{}/{}", self.base_url, bucket, key))
    }

    async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        _ttl: Duration,
    ) -> Result<String, DomainError> {
        Ok(format!("{}/{}/{}", self.base_url, bucket, key))
    }

    /// Directory walk that emulates S3 prefix + delimiter semantics.
    ///
    /// Dev-only: the cursor is a plain key offset rather than an S3
    /// continuation token, which is enough for the browse UI and keeps the
    /// local backend dependency-free.
    async fn list_objects_page(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: Option<&str>,
        cursor: Option<&str>,
        max_keys: i32,
    ) -> Result<ObjectPage, DomainError> {
        let root = self.base_path.join(bucket);
        if !root.exists() {
            return Ok(ObjectPage::default());
        }

        let mut keys: Vec<(String, u64, Option<chrono::DateTime<chrono::Utc>>)> = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(mut entries) = fs::read_dir(&dir).await else {
                continue;
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&root) else {
                    continue;
                };
                let key = rel.to_string_lossy().replace('\\', "/");
                if !key.starts_with(prefix) {
                    continue;
                }
                let (size, modified) = match entry.metadata().await {
                    Ok(md) => (md.len(), md.modified().ok().map(Into::into)),
                    Err(_) => (0, None),
                };
                keys.push((key, size, modified));
            }
        }
        keys.sort_by(|a, b| a.0.cmp(&b.0));

        // Roll keys up to the first delimiter past the prefix, as S3 does.
        let mut objects = Vec::new();
        let mut common_prefixes: Vec<String> = Vec::new();
        for (key, size, modified) in keys {
            if let Some(delim) = delimiter
                && let Some(idx) = key[prefix.len()..].find(delim)
            {
                let folder = format!("{}{}", &key[..prefix.len() + idx], delim);
                if !common_prefixes.contains(&folder) {
                    common_prefixes.push(folder);
                }
                continue;
            }
            objects.push(StoredObject {
                key,
                size: i64::try_from(size).unwrap_or(i64::MAX),
                last_modified: modified,
            });
        }

        let start = cursor
            .and_then(|c| objects.iter().position(|o| o.key.as_str() == c))
            .unwrap_or(0);
        let limit = max_keys.clamp(1, 1000) as usize;
        let end = (start + limit).min(objects.len());
        let next_cursor = objects.get(end).map(|o| o.key.clone());
        let objects = objects[start..end].to_vec();

        Ok(ObjectPage {
            objects,
            common_prefixes,
            next_cursor,
        })
    }

    async fn object_exists(&self, bucket: &str, key: &str) -> Result<bool, DomainError> {
        let path = self.get_path(bucket, key);
        Ok(path.exists())
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), DomainError> {
        let path = self.get_path(bucket, key);
        if path.exists() {
            fs::remove_file(&path)
                .await
                .map_err(|e| DomainError::Internal(format!("Failed to delete file: {e}")))?;
        }
        Ok(())
    }
}

// =============================================================================
// S3 EVIDENCE STORAGE ADAPTER (production)
// =============================================================================

/// Production S3 adapter — wraps `portal_storage::S3Storage`,
/// implements the domain's `EvidenceS3Client` trait.
#[derive(Debug, Clone)]
pub struct S3EvidenceStorageAdapter {
    s3: S3Storage,
}

impl S3EvidenceStorageAdapter {
    /// Create a new S3 evidence storage adapter.
    pub async fn new(config: S3Config) -> Self {
        Self {
            s3: S3Storage::new(config).await,
        }
    }

    /// Create from an existing AWS SDK config (used by tests with explicit credentials).
    ///
    /// Set `force_path_style` to `true` for MinIO/LocalStack compatibility.
    pub fn from_sdk_config(
        sdk_config: &aws_config::SdkConfig,
        bucket: impl Into<String>,
        public_url: impl Into<String>,
        force_path_style: bool,
    ) -> Self {
        let client = if force_path_style {
            aws_sdk_s3::Client::from_conf(
                aws_sdk_s3::config::Builder::from(sdk_config)
                    .force_path_style(true)
                    .build(),
            )
        } else {
            aws_sdk_s3::Client::new(sdk_config)
        };
        Self {
            s3: S3Storage::from_client(client, bucket, public_url),
        }
    }
}

#[async_trait]
impl EvidenceS3Client for S3EvidenceStorageAdapter {
    async fn presign_put(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<String, DomainError> {
        crate::observability::track_s3(
            "evidence",
            "presign-put",
            self.s3.presign_put(bucket, key, content_type, ttl),
        )
        .await
        .map_err(|e| DomainError::Internal(format!("S3 presign_put failed: {e}")))
    }

    async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<String, DomainError> {
        crate::observability::track_s3(
            "evidence",
            "presign-get",
            self.s3.presign_get(bucket, key, ttl),
        )
        .await
        .map_err(|e| DomainError::Internal(format!("S3 presign_get failed: {e}")))
    }

    async fn list_objects_page(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: Option<&str>,
        cursor: Option<&str>,
        max_keys: i32,
    ) -> Result<ObjectPage, DomainError> {
        let page = crate::observability::track_s3(
            "demos",
            "list",
            self.s3
                .list_objects_page(bucket, prefix, delimiter, cursor, max_keys),
        )
        .await
        .map_err(|e| DomainError::Internal(format!("S3 list_objects_page failed: {e}")))?;

        Ok(ObjectPage {
            objects: page
                .objects
                .into_iter()
                .map(|o| StoredObject {
                    key: o.key,
                    size: o.size,
                    last_modified: o.last_modified,
                })
                .collect(),
            common_prefixes: page.common_prefixes,
            next_cursor: page.next_continuation_token,
        })
    }

    async fn object_exists(&self, bucket: &str, key: &str) -> Result<bool, DomainError> {
        crate::observability::track_s3("evidence", "head", self.s3.object_exists(bucket, key))
            .await
            .map_err(|e| DomainError::Internal(format!("S3 object_exists failed: {e}")))
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), DomainError> {
        crate::observability::track_s3("evidence", "delete", self.s3.delete_object(bucket, key))
            .await
            .map_err(|e| DomainError::Internal(format!("S3 delete_object failed: {e}")))
    }
}

// =============================================================================
// EVIDENCE STORAGE BACKEND ENUM (runtime switching)
// =============================================================================

/// Runtime-switchable evidence storage backend.
///
/// Constructed based on env vars (`EVIDENCE_STORAGE=local|s3`).
#[derive(Debug, Clone)]
pub enum EvidenceStorageBackend {
    Local(LocalEvidenceStorage),
    S3(S3EvidenceStorageAdapter),
}

#[async_trait]
impl EvidenceS3Client for EvidenceStorageBackend {
    async fn presign_put(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<String, DomainError> {
        match self {
            Self::Local(local) => local.presign_put(bucket, key, content_type, ttl).await,
            Self::S3(s3) => s3.presign_put(bucket, key, content_type, ttl).await,
        }
    }

    async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<String, DomainError> {
        match self {
            Self::Local(local) => local.presign_get(bucket, key, ttl).await,
            Self::S3(s3) => s3.presign_get(bucket, key, ttl).await,
        }
    }

    async fn list_objects_page(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: Option<&str>,
        cursor: Option<&str>,
        max_keys: i32,
    ) -> Result<ObjectPage, DomainError> {
        match self {
            Self::Local(local) => {
                local
                    .list_objects_page(bucket, prefix, delimiter, cursor, max_keys)
                    .await
            }
            Self::S3(s3) => {
                s3.list_objects_page(bucket, prefix, delimiter, cursor, max_keys)
                    .await
            }
        }
    }

    async fn object_exists(&self, bucket: &str, key: &str) -> Result<bool, DomainError> {
        match self {
            Self::Local(local) => local.object_exists(bucket, key).await,
            Self::S3(s3) => s3.object_exists(bucket, key).await,
        }
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), DomainError> {
        match self {
            Self::Local(local) => local.delete_object(bucket, key).await,
            Self::S3(s3) => s3.delete_object(bucket, key).await,
        }
    }
}
