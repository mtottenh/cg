//! Repository error types.

use portal_core::DomainError;
use thiserror::Error;

/// Errors that can occur during repository operations.
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// Entity not found.
    #[error("{entity_type} not found: {id}")]
    NotFound {
        entity_type: &'static str,
        id: String,
    },

    /// Unique constraint violation.
    #[error("duplicate {field}: {value}")]
    Duplicate { field: String, value: String },

    /// Foreign key constraint violation.
    #[error("referenced {entity_type} not found: {id}")]
    ForeignKeyViolation { entity_type: String, id: String },

    /// Check constraint violation.
    #[error("constraint violation: {message}")]
    ConstraintViolation { message: String },

    /// Database connection error.
    #[error("database connection error: {0}")]
    Connection(String),

    /// Database query error.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Serialization error (for JSONB fields).
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl RepositoryError {
    /// Create a not found error.
    #[must_use]
    pub fn not_found(entity_type: &'static str, id: impl ToString) -> Self {
        Self::NotFound {
            entity_type,
            id: id.to_string(),
        }
    }

    /// Create a duplicate error.
    #[must_use]
    pub fn duplicate(field: impl Into<String>, value: impl ToString) -> Self {
        Self::Duplicate {
            field: field.into(),
            value: value.to_string(),
        }
    }

    /// Check if this is a not found error.
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Check if this is a duplicate error.
    #[must_use]
    pub const fn is_duplicate(&self) -> bool {
        matches!(self, Self::Duplicate { .. })
    }

    /// Try to extract constraint info from a `SQLx` error.
    pub fn from_sqlx_error(err: sqlx::Error, context: &str) -> Self {
        match &err {
            sqlx::Error::Database(db_err) => {
                // Check for unique constraint violation
                if let Some(constraint) = db_err.constraint() {
                    let constraint_owned = constraint.to_string();
                    if constraint.contains("unique") || db_err.code().is_some_and(|c| c == "23505")
                    {
                        return Self::Duplicate {
                            field: constraint_owned,
                            value: context.to_string(),
                        };
                    }
                    // Check for foreign key violation
                    if db_err.code().is_some_and(|c| c == "23503") {
                        return Self::ForeignKeyViolation {
                            entity_type: constraint_owned,
                            id: context.to_string(),
                        };
                    }
                    // Check for check constraint violation
                    if db_err.code().is_some_and(|c| c == "23514") {
                        return Self::ConstraintViolation {
                            message: format!("{constraint_owned}: {context}"),
                        };
                    }
                }
                Self::Database(err)
            }
            _ => Self::Database(err),
        }
    }
}

// Convert repository errors to domain errors for use in services
impl From<RepositoryError> for DomainError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound { entity_type, id } => match entity_type {
                "User" => Self::UserNotFound(id),
                "Player" => Self::PlayerNotFound(id),
                "Team" => Self::TeamNotFound(id),
                "Game" => Self::GameNotFound(id),
                "Match" => Self::MatchNotFound(id),
                "Tournament" => Self::TournamentNotFound(id),
                "League" => Self::LeagueNotFound(id),
                "Lobby" => Self::LobbyNotFound(id),
                _ => Self::Internal(format!("{entity_type} not found: {id}")),
            },
            RepositoryError::Duplicate { field, value } => {
                Self::Conflict(format!("{field} already exists: {value}"))
            }
            RepositoryError::ForeignKeyViolation { entity_type, id } => {
                Self::Internal(format!("referenced {entity_type} not found: {id}"))
            }
            RepositoryError::ConstraintViolation { message } => {
                Self::InvalidState(message)
            }
            RepositoryError::Connection(msg) => Self::Internal(msg),
            RepositoryError::Database(err) => Self::Internal(err.to_string()),
            RepositoryError::Serialization(err) => Self::Internal(err.to_string()),
        }
    }
}
