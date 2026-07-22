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
pub use s3::{ObjectInfo, ObjectMetadata, S3Config, S3EvidenceClient, S3Storage};

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

    /// Get the public URL for a storage key.
    fn public_url(&self, key: &str) -> String;
}
