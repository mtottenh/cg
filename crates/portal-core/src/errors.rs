//! Error types for the Gaming Portal.
//!
//! Errors are organized in layers:
//! - [`ValidationError`]: Input validation failures (field-level)
//! - [`DomainError`]: Business rule violations
//! - API errors (in portal-api crate) convert these to RFC 7807 responses

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single validation error for a specific field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldError {
    /// The field name that failed validation.
    pub field: String,
    /// Human-readable error message.
    pub message: String,
    /// Machine-readable error code (e.g., "length", "format", "required").
    pub code: String,
}

impl FieldError {
    /// Create a new field error.
    #[must_use]
    pub fn new(field: impl Into<String>, message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            code: code.into(),
        }
    }

    /// Create a "required" field error.
    #[must_use]
    pub fn required(field: impl Into<String>) -> Self {
        let field = field.into();
        Self {
            message: format!("{field} is required"),
            field,
            code: "required".to_string(),
        }
    }

    /// Create a "length" field error.
    #[must_use]
    pub fn length(field: impl Into<String>, min: usize, max: usize) -> Self {
        let field = field.into();
        Self {
            message: format!("{field} must be between {min} and {max} characters"),
            field,
            code: "length".to_string(),
        }
    }

    /// Create a "format" field error.
    #[must_use]
    pub fn format(field: impl Into<String>, expected: impl Into<String>) -> Self {
        let field = field.into();
        let expected = expected.into();
        Self {
            message: format!("{field} must be {expected}"),
            field,
            code: "format".to_string(),
        }
    }
}

/// Collection of validation errors.
///
/// Accumulates multiple field errors for batch validation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationError {
    errors: Vec<FieldError>,
}

impl ValidationError {
    /// Create an empty validation error collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a validation error with a single field error.
    #[must_use]
    pub fn field(error: FieldError) -> Self {
        Self {
            errors: vec![error],
        }
    }

    /// Add a field error.
    pub fn add(&mut self, error: FieldError) {
        self.errors.push(error);
    }

    /// Add a field error and return self for chaining.
    #[must_use]
    pub fn with(mut self, error: FieldError) -> Self {
        self.errors.push(error);
        self
    }

    /// Check if there are any errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get all field errors.
    #[must_use]
    pub fn errors(&self) -> &[FieldError] {
        &self.errors
    }

    /// Convert to Result - Ok if no errors, Err(self) otherwise.
    pub fn into_result<T>(self, value: T) -> Result<T, Self> {
        if self.has_errors() {
            Err(self)
        } else {
            Ok(value)
        }
    }

    /// Get errors grouped by field name.
    #[must_use]
    pub fn by_field(&self) -> HashMap<&str, Vec<&FieldError>> {
        let mut map: HashMap<&str, Vec<&FieldError>> = HashMap::new();
        for error in &self.errors {
            map.entry(&error.field).or_default().push(error);
        }
        map
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.errors.is_empty() {
            write!(f, "validation error (no details)")
        } else if self.errors.len() == 1 {
            write!(f, "{}", self.errors[0].message)
        } else {
            write!(f, "{} validation errors", self.errors.len())
        }
    }
}

impl std::error::Error for ValidationError {}

/// Domain-level errors representing business rule violations.
///
/// These errors occur when operations fail due to business logic constraints,
/// not due to invalid input or infrastructure failures.
#[derive(Debug, Error)]
pub enum DomainError {
    // Entity not found errors
    #[error("user not found: {0}")]
    UserNotFound(String),

    #[error("player not found: {0}")]
    PlayerNotFound(String),

    #[error("team not found: {0}")]
    TeamNotFound(String),

    #[error("game not found: {0}")]
    GameNotFound(String),

    #[error("match not found: {0}")]
    MatchNotFound(String),

    #[error("tournament not found: {0}")]
    TournamentNotFound(String),

    #[error("league not found: {0}")]
    LeagueNotFound(String),

    #[error("lobby not found: {0}")]
    LobbyNotFound(String),

    #[error("league season not found: {0}")]
    LeagueSeasonNotFound(String),

    #[error("league team not found: {0}")]
    LeagueTeamNotFound(String),

    #[error("league team invitation not found: {0}")]
    LeagueTeamInvitationNotFound(String),

    // Authorization errors
    #[error("not authorized: {0}")]
    NotAuthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    // Authentication errors
    #[error("invalid or missing token")]
    InvalidToken,

    #[error("token has expired")]
    TokenExpired,

    #[error("invalid credentials")]
    InvalidCredentials,

    // Team-specific errors
    #[error("player is already a member of this team")]
    AlreadyTeamMember,

    #[error("player is not a member of this team")]
    NotTeamMember,

    #[error("cannot remove the team founder")]
    CannotRemoveFounder,

    #[error("cannot demote the team founder")]
    CannotDemoteFounder,

    #[error("team must have at least one captain")]
    TeamMustHaveCaptain,

    #[error("team invitation has expired")]
    InvitationExpired,

    #[error("team invitation not found or already used")]
    InvitationInvalid,

    #[error("player already has a pending invitation to this team")]
    InvitationAlreadyExists,

    #[error("team has reached maximum member count")]
    TeamFull,

    // Match/Queue errors
    #[error("player is already in queue")]
    AlreadyInQueue,

    #[error("player is not in queue")]
    NotInQueue,

    #[error("match has already started")]
    MatchAlreadyStarted,

    #[error("match has already ended")]
    MatchAlreadyEnded,

    // Lobby errors
    #[error("lobby is full")]
    LobbyFull,

    #[error("player is already in a lobby")]
    AlreadyInLobby,

    #[error("player is not in lobby")]
    NotInLobby,

    #[error("invalid lobby state transition: {from} -> {to}")]
    InvalidLobbyTransition { from: String, to: String },

    // Tournament/League errors
    #[error("tournament registration is closed")]
    RegistrationClosed,

    #[error("team is already registered for this tournament")]
    AlreadyRegistered,

    #[error("team does not meet tournament requirements")]
    RequirementsNotMet(String),

    #[error("not a league member")]
    NotLeagueMember,

    #[error("league membership is invite-only")]
    LeagueInviteOnly,

    // Rating errors
    #[error("rating calculation failed: {0}")]
    RatingCalculationFailed(String),

    // Saga errors
    #[error("saga failed: {0}")]
    SagaFailed(String),

    #[error("saga step failed: {step} - {reason}")]
    SagaStepFailed { step: String, reason: String },

    // Generic errors
    #[error("conflict: {0}")]
    Conflict(String),

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("validation failed")]
    Validation(#[from] ValidationError),

    #[error("internal error: {0}")]
    Internal(String),
}

impl DomainError {
    /// Create a not authorized error with a custom message.
    #[must_use]
    pub fn not_authorized(msg: impl Into<String>) -> Self {
        Self::NotAuthorized(msg.into())
    }

    /// Create a forbidden error with a custom message.
    #[must_use]
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    /// Create a conflict error with a custom message.
    #[must_use]
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    /// Create an internal error with a custom message.
    #[must_use]
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    /// Create a not found error for any entity type.
    ///
    /// This is a generic helper that uses the appropriate specific variant
    /// based on the entity type name.
    #[must_use]
    pub fn not_found(entity_type: &str, id: impl Into<String>) -> Self {
        let id = id.into();
        match entity_type {
            "user" => Self::UserNotFound(id),
            "player" => Self::PlayerNotFound(id),
            "team" => Self::TeamNotFound(id),
            "game" => Self::GameNotFound(id),
            "match" => Self::MatchNotFound(id),
            "tournament" => Self::TournamentNotFound(id),
            "league" => Self::LeagueNotFound(id),
            "lobby" => Self::LobbyNotFound(id),
            "league season" => Self::LeagueSeasonNotFound(id),
            "league team" => Self::LeagueTeamNotFound(id),
            "league team invitation" => Self::LeagueTeamInvitationNotFound(id),
            _ => Self::Internal(format!("{entity_type} not found: {id}")),
        }
    }

    /// Check if this is a "not found" type error.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::UserNotFound(_)
                | Self::PlayerNotFound(_)
                | Self::TeamNotFound(_)
                | Self::GameNotFound(_)
                | Self::MatchNotFound(_)
                | Self::TournamentNotFound(_)
                | Self::LeagueNotFound(_)
                | Self::LobbyNotFound(_)
                | Self::LeagueSeasonNotFound(_)
                | Self::LeagueTeamNotFound(_)
                | Self::LeagueTeamInvitationNotFound(_)
        )
    }

    /// Check if this is an authorization error.
    #[must_use]
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::NotAuthorized(_) | Self::Forbidden(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_error_constructors() {
        let required = FieldError::required("name");
        assert_eq!(required.code, "required");
        assert!(required.message.contains("name"));

        let length = FieldError::length("tag", 2, 5);
        assert_eq!(length.code, "length");
        assert!(length.message.contains("2"));
        assert!(length.message.contains("5"));
    }

    #[test]
    fn test_validation_error_accumulation() {
        let mut errors = ValidationError::new();
        assert!(!errors.has_errors());

        errors.add(FieldError::required("name"));
        errors.add(FieldError::length("tag", 2, 5));
        assert!(errors.has_errors());
        assert_eq!(errors.errors().len(), 2);
    }

    #[test]
    fn test_validation_error_into_result() {
        let empty = ValidationError::new();
        assert!(empty.into_result(42).is_ok());

        let with_error = ValidationError::field(FieldError::required("x"));
        assert!(with_error.into_result(42).is_err());
    }

    #[test]
    fn test_domain_error_categorization() {
        assert!(DomainError::UserNotFound("123".into()).is_not_found());
        assert!(DomainError::TeamNotFound("456".into()).is_not_found());
        assert!(!DomainError::AlreadyTeamMember.is_not_found());

        assert!(DomainError::NotAuthorized("test".into()).is_auth_error());
        assert!(DomainError::Forbidden("test".into()).is_auth_error());
        assert!(!DomainError::AlreadyInQueue.is_auth_error());
    }
}
