//! Pluggable storage backends for the Gaming Portal.
//!
//! This crate provides a trait-based abstraction for file storage operations,
//! with implementations for local filesystem and S3-compatible storage.
//!
//! # Features
//!
//! - `local` (default): Local filesystem storage backend
//! - `s3`: AWS S3 / S3-compatible storage backend
//!
//! # Example
//!
//! ```rust,ignore
//! use portal_storage::{StorageBackend, LocalStorage, StoreRequest};
//! use bytes::Bytes;
//!
//! let storage = LocalStorage::new("/tmp/uploads", "http://localhost:3000/uploads");
//! let request = StoreRequest {
//!     data: Bytes::from("hello world"),
//!     filename: "test.txt".to_string(),
//!     content_type: "text/plain".to_string(),
//!     prefix: "files".to_string(),
//!     owner_id: Some("user-123".to_string()),
//!     keep_filename: false,
//! };
//!
//! let result = storage.store(request).await?;
//! println!("File stored at: {}", result.url);
//! ```

pub mod error;
pub mod image;

#[cfg(feature = "local")]
mod local;

#[cfg(feature = "s3")]
mod s3;

pub use error::StorageError;

#[cfg(feature = "local")]
pub use local::LocalStorage;

#[cfg(feature = "s3")]
pub use s3::{
    ObjectInfo, ObjectMetadata, ObjectPage, S3Config, S3Credentials, S3EvidenceClient, S3Storage,
};

use async_trait::async_trait;
use bytes::Bytes;

/// Request to store a file.
#[derive(Debug, Clone)]
pub struct StoreRequest {
    /// File content as bytes.
    pub data: Bytes,
    /// Original filename.
    pub filename: String,
    /// MIME content type.
    pub content_type: String,
    /// Storage path prefix (e.g., "teams/logos").
    pub prefix: String,
    /// Optional owner ID for path organization.
    pub owner_id: Option<String>,
    /// Name the object after `filename` instead of a fresh UUID.
    ///
    /// For files whose name other services key on: demo-stats names its
    /// parse output after the object's basename and the scanner looks it up
    /// by the cataloged file name, so the two must be the same string. The
    /// caller must pass a sanitised basename; a re-upload under the same
    /// name overwrites, which makes replays idempotent.
    pub keep_filename: bool,
}

/// The storage key for a request: `prefix[/owner]/name`.
fn object_key(request: &StoreRequest) -> String {
    let name = if request.keep_filename {
        // Defence in depth: never let a path through, whatever the caller did.
        let base = request
            .filename
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default();
        if base.is_empty() || base == "." || base == ".." {
            format!("{}.bin", uuid::Uuid::now_v7())
        } else {
            base.to_string()
        }
    } else {
        let extension = request.filename.rsplit('.').next().unwrap_or("bin");
        format!("{}.{}", uuid::Uuid::now_v7(), extension)
    };
    match &request.owner_id {
        Some(owner) => format!("{}/{}/{}", request.prefix, owner, name),
        None => format!("{}/{}", request.prefix, name),
    }
}

/// Result of a successful store operation.
#[derive(Debug, Clone)]
pub struct StoredFile {
    /// Public URL to access the file.
    pub url: String,
    /// Storage key (path within the backend).
    pub key: String,
    /// File size in bytes.
    pub size: u64,
    /// MIME content type.
    pub content_type: String,
}

/// Trait for pluggable storage backends.
///
/// Implementations must be thread-safe and can be shared across tasks.
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    /// Store a file and return metadata.
    ///
    /// The implementation should generate a unique key based on the request
    /// and store the file in the appropriate location.
    async fn store(&self, request: StoreRequest) -> Result<StoredFile, StorageError>;

    /// Delete a file by its storage key.
    ///
    /// Returns `Ok(())` if the file was deleted or didn't exist.
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Check if a file exists.
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// Read a stored file's bytes by key.
    async fn read(&self, key: &str) -> Result<Bytes, StorageError>;

    /// Get the public URL for a storage key.
    fn public_url(&self, key: &str) -> String;
}

#[cfg(test)]
mod object_key_tests {
    use super::*;

    fn request(filename: &str, owner: Option<&str>, keep_filename: bool) -> StoreRequest {
        StoreRequest {
            data: Bytes::new(),
            filename: filename.to_string(),
            content_type: "application/octet-stream".to_string(),
            prefix: "matchzy/7".to_string(),
            owner_id: owner.map(String::from),
            keep_filename,
        }
    }

    #[test]
    fn keeps_the_file_name_when_asked() {
        assert_eq!(
            object_key(&request("a_b.dem", Some("srv"), true)),
            "matchzy/7/srv/a_b.dem"
        );
    }

    #[test]
    fn never_keeps_a_path() {
        assert_eq!(
            object_key(&request("../../x.dem", None, true)),
            "matchzy/7/x.dem"
        );
        let key = object_key(&request("dir/..", None, true));
        assert!(
            key.starts_with("matchzy/7/") && key.rsplit('.').next() == Some("bin"),
            "{key}"
        );
    }

    #[test]
    fn otherwise_uses_a_fresh_uuid_with_the_extension() {
        let key = object_key(&request("logo.png", Some("p1"), false));
        assert!(
            key.starts_with("matchzy/7/p1/") && key.rsplit('.').next() == Some("png"),
            "{key}"
        );
        assert_ne!(key, object_key(&request("logo.png", Some("p1"), false)));
    }
}
