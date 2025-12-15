//! Error types for the plugin system.

use thiserror::Error;

/// Errors that can occur in the plugin system.
#[derive(Debug, Error)]
pub enum PluginError {
    /// Plugin with this ID already registered.
    #[error("Plugin '{0}' is already registered")]
    AlreadyRegistered(String),

    /// Plugin not found.
    #[error("Plugin '{0}' not found")]
    NotFound(String),

    /// Plugin failed to initialize.
    #[error("Plugin '{0}' failed to initialize: {1}")]
    InitializationFailed(String, String),

    /// Invalid plugin configuration.
    #[error("Invalid plugin configuration: {0}")]
    InvalidConfiguration(String),

    /// Feature not supported by this plugin.
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// Evidence parsing error.
    #[error("Evidence parsing error: {0}")]
    EvidenceParseError(String),

    /// Storage error (S3, etc.).
    #[error("Storage error: {0}")]
    StorageError(String),

    /// External service error (demo API, etc.).
    #[error("External service error: {0}")]
    ExternalService(String),

    /// Parse error (JSON, etc.).
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Errors that can occur during stats calculation.
#[derive(Debug, Error)]
pub enum StatsError {
    /// Missing required field in match data.
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid data format.
    #[error("Invalid data format: {0}")]
    InvalidFormat(String),

    /// Calculation error.
    #[error("Calculation error: {0}")]
    CalculationError(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Errors that can occur during rating calculation.
#[derive(Debug, Error)]
pub enum RatingError {
    /// Not enough participants.
    #[error("Not enough participants for rating calculation")]
    InsufficientParticipants,

    /// Invalid match result.
    #[error("Invalid match result: {0}")]
    InvalidResult(String),

    /// Rating calculation overflow.
    #[error("Rating calculation overflow")]
    Overflow,
}
