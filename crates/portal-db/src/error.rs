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

// Convert repository errors to domain errors for use in services.
//
// Two principles:
//
//   1. **Every known NotFound variant maps to its typed DomainError**.
//      The previous version handled only 8 entity types and silently
//      collapsed the rest to DomainError::Internal — turning legitimate
//      404s into 500s and leaking the constructed message into the
//      response body.
//
//   2. **Raw database errors never reach the response body**. Database,
//      Connection, and Serialization errors carry source-level detail
//      (table names, constraint names, sometimes row data) that must
//      not be exposed to API clients. We log the source via tracing
//      and return an opaque message.
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
                "TournamentStage" => Self::TournamentStageNotFound(id),
                "TournamentBracket" => Self::TournamentBracketNotFound(id),
                "TournamentMatch" => Self::TournamentMatchNotFound(id),
                "TournamentRegistration" => Self::TournamentRegistrationNotFound(id),
                "League" | "LeagueMember" | "LeagueInvitation" => Self::LeagueNotFound(id),
                "LeagueSeason" => Self::LeagueSeasonNotFound(id),
                "LeagueTeam" => Self::LeagueTeamNotFound(id),
                "LeagueTeamInvitation" => Self::LeagueTeamInvitationNotFound(id),
                "Lobby" => Self::LobbyNotFound(id),
                "Ban" => Self::BanNotFound(id),
                "Dispute" => Self::DisputeNotFound(id),
                "ForfeitRecord" => Self::ForfeitRecordNotFound(id),
                "Evidence" => Self::EvidenceNotFound(id),
                "ResultClaim" => Self::ResultClaimNotFound(id),
                "VetoSession" => Self::VetoSessionNotFound(id),
                "Demo" => Self::DemoNotFound(id),
                "DemoMatchLink" => Self::DemoMatchLinkNotFound(id),
                "ResultReview" => Self::ResultReviewNotFound(id),
                other => {
                    // Programmer error: an adapter returned a NotFound for an
                    // entity type we don't know how to surface. Log loudly so
                    // it can be added; do not leak the constructed string.
                    tracing::error!(
                        entity_type = %other,
                        id = %id,
                        "RepositoryError::NotFound for unknown entity type — add a match arm in portal-db/src/error.rs"
                    );
                    Self::Internal("entity not found".into())
                }
            },
            RepositoryError::Duplicate { field, value } => {
                // Field name + value are user-facing (e.g. "username" / "alice")
                // and so are intentionally preserved.
                Self::Conflict(format!("{field} already exists: {value}"))
            }
            RepositoryError::ForeignKeyViolation { entity_type, id } => {
                // A referenced row is missing — a 409 Conflict, not a 500.
                tracing::warn!(
                    entity_type = %entity_type,
                    id = %id,
                    "foreign key violation"
                );
                Self::Conflict("referenced entity does not exist".into())
            }
            RepositoryError::ConstraintViolation { message } => {
                tracing::warn!(constraint = %message, "check constraint violation");
                Self::InvalidState("constraint violation".into())
            }
            RepositoryError::Connection(msg) => {
                tracing::error!(error = %msg, "database connection error");
                Self::Internal("database unavailable".into())
            }
            RepositoryError::Database(err) => {
                tracing::error!(error = %err, "database error");
                Self::Internal("database error".into())
            }
            RepositoryError::Serialization(err) => {
                tracing::error!(error = %err, "serialization error");
                Self::Internal("serialization error".into())
            }
        }
    }
}
