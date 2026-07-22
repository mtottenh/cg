//! Local filesystem storage backend.

use crate::{StorageBackend, StorageError, StoreRequest, StoredFile};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::instrument;

/// Local filesystem storage backend.
///
/// Stores files in a directory on the local filesystem and serves them
/// via a base URL (typically through a static file server).
#[derive(Debug, Clone)]
pub struct LocalStorage {
    /// Base path for file storage.
    base_path: PathBuf,
    /// Base URL for public access.
    base_url: String,
}

impl LocalStorage {
    /// Create a new local storage backend.
    ///
    /// # Arguments
    ///
    /// * `base_path` - Directory path where files will be stored
    /// * `base_url` - Base URL for accessing stored files
    ///
    /// # Example
    ///
    /// ```rust
    /// use portal_storage::LocalStorage;
    ///
    /// let storage = LocalStorage::new("./uploads", "http://localhost:3000/uploads");
    /// ```
    #[must_use]
    pub fn new(base_path: impl Into<PathBuf>, base_url: impl Into<String>) -> Self {
        let mut base_url = base_url.into();
        // Remove trailing slash for consistent URL building
        if base_url.ends_with('/') {
            base_url.pop();
        }

        Self {
            base_path: base_path.into(),
            base_url,
        }
    }

    /// Ensure the storage directory exists.
    async fn ensure_base_path(&self) -> Result<(), StorageError> {
        fs::create_dir_all(&self.base_path).await?;
        Ok(())
    }

    /// Generate a unique storage key.
    fn generate_key(&self, request: &StoreRequest) -> String {
        let id = uuid::Uuid::now_v7();
        let extension = request.filename.rsplit('.').next().unwrap_or("bin");

        match &request.owner_id {
            Some(owner) => format!("{}/{}/{}.{}", request.prefix, owner, id, extension),
            None => format!("{}/{}.{}", request.prefix, id, extension),
        }
    }

    /// Get the full filesystem path for a key.
    fn full_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    #[instrument(skip(self, request), fields(prefix = %request.prefix, filename = %request.filename))]
    async fn store(&self, request: StoreRequest) -> Result<StoredFile, StorageError> {
        self.ensure_base_path().await?;

        let key = self.generate_key(&request);
        let path = self.full_path(&key);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write the file
        let mut file = fs::File::create(&path).await?;
        file.write_all(&request.data).await?;
        file.flush().await?;

        let size = request.data.len() as u64;
        let url = self.public_url(&key);

        tracing::debug!(key = %key, size = size, "File stored successfully");

        Ok(StoredFile {
            url,
            key,
            size,
            content_type: request.content_type,
        })
    }

    #[instrument(skip(self))]
    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.full_path(key);

        match fs::remove_file(&path).await {
            Ok(()) => {
                tracing::debug!(key = %key, "File deleted successfully");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!(key = %key, "File not found, nothing to delete");
                Ok(())
            }
            Err(e) => Err(StorageError::from(e)),
        }
    }

    #[instrument(skip(self))]
    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let path = self.full_path(key);
        Ok(path.exists())
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.base_url, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::path::Path;

    fn test_storage(dir: &Path) -> LocalStorage {
        LocalStorage::new(dir, "http://localhost:3000/uploads")
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = test_storage(temp_dir.path());

        let request = StoreRequest {
            data: Bytes::from("hello world"),
            filename: "test.txt".to_string(),
            content_type: "text/plain".to_string(),
            prefix: "files".to_string(),
            owner_id: None,
        };

        let result = storage.store(request).await.unwrap();

        assert!(
            result
                .url
                .starts_with("http://localhost:3000/uploads/files/")
        );
        assert!(result.key.starts_with("files/"));
        assert!(
            Path::new(&result.key)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
        );
        assert_eq!(result.size, 11);
        assert_eq!(result.content_type, "text/plain");

        // Verify file exists
        assert!(storage.exists(&result.key).await.unwrap());

        // Read the file and verify content
        let path = storage.full_path(&result.key);
        let content = fs::read_to_string(path).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_store_with_owner_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = test_storage(temp_dir.path());

        let request = StoreRequest {
            data: Bytes::from("test"),
            filename: "avatar.png".to_string(),
            content_type: "image/png".to_string(),
            prefix: "players/avatars".to_string(),
            owner_id: Some("player-123".to_string()),
        };

        let result = storage.store(request).await.unwrap();

        assert!(result.key.contains("players/avatars/player-123/"));
        assert!(
            Path::new(&result.key)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        );
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = test_storage(temp_dir.path());

        let request = StoreRequest {
            data: Bytes::from("to be deleted"),
            filename: "delete_me.txt".to_string(),
            content_type: "text/plain".to_string(),
            prefix: "temp".to_string(),
            owner_id: None,
        };

        let result = storage.store(request).await.unwrap();
        assert!(storage.exists(&result.key).await.unwrap());

        storage.delete(&result.key).await.unwrap();
        assert!(!storage.exists(&result.key).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = test_storage(temp_dir.path());

        // Deleting a non-existent file should not error
        let result = storage.delete("nonexistent/file.txt").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_public_url() {
        let storage = LocalStorage::new("/tmp/uploads", "http://example.com/files/");

        assert_eq!(
            storage.public_url("teams/logos/abc.png"),
            "http://example.com/files/teams/logos/abc.png"
        );
    }
}
