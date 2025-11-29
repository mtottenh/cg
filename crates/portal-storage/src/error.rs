//! Storage error types.

use thiserror::Error;

/// Errors that can occur during storage operations.
#[derive(Error, Debug)]
pub enum StorageError {
    /// File not found.
    #[error("File not found: {key}")]
    NotFound {
        /// The key that was not found.
        key: String,
    },

    /// I/O error during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// S3-specific error.
    #[cfg(feature = "s3")]
    #[error("S3 error: {message}")]
    S3 {
        /// Error message from S3.
        message: String,
    },

    /// Invalid storage configuration.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// File too large.
    #[error("File too large: {size} bytes exceeds limit of {limit} bytes")]
    FileTooLarge {
        /// Actual file size.
        size: u64,
        /// Maximum allowed size.
        limit: u64,
    },
}

impl StorageError {
    /// Create a new configuration error.
    #[must_use]
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration(message.into())
    }

    /// Create a new not found error.
    #[must_use]
    pub fn not_found(key: impl Into<String>) -> Self {
        Self::NotFound { key: key.into() }
    }
}
