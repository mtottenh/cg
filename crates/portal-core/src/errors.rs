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
    /// The requested user was not found.
    #[error("user not found: {0}")]
    UserNotFound(String),

    /// The requested player was not found.
    #[error("player not found: {0}")]
    PlayerNotFound(String),

    /// The requested team was not found.
    #[error("team not found: {0}")]
    TeamNotFound(String),

    /// The requested game was not found.
    #[error("game not found: {0}")]
    GameNotFound(String),

    /// The requested match was not found.
    #[error("match not found: {0}")]
    MatchNotFound(String),

    /// The requested tournament was not found.
    #[error("tournament not found: {0}")]
    TournamentNotFound(String),

    /// The requested league was not found.
    #[error("league not found: {0}")]
    LeagueNotFound(String),

    /// The requested lobby was not found.
    #[error("lobby not found: {0}")]
    LobbyNotFound(String),

    /// The requested league season was not found.
    #[error("league season not found: {0}")]
    LeagueSeasonNotFound(String),

    /// The requested league team was not found.
    #[error("league team not found: {0}")]
    LeagueTeamNotFound(String),

    /// The requested league team invitation was not found.
    #[error("league team invitation not found: {0}")]
    LeagueTeamInvitationNotFound(String),

    /// The requested ban record was not found.
    #[error("ban not found: {0}")]
    BanNotFound(String),

    /// The requested tournament stage was not found.
    #[error("tournament stage not found: {0}")]
    TournamentStageNotFound(String),

    /// The requested tournament bracket was not found.
    #[error("tournament bracket not found: {0}")]
    TournamentBracketNotFound(String),

    /// The requested tournament match was not found.
    #[error("tournament match not found: {0}")]
    TournamentMatchNotFound(String),

    /// The requested tournament registration was not found.
    #[error("tournament registration not found: {0}")]
    TournamentRegistrationNotFound(String),

    /// The requested dispute was not found.
    #[error("dispute not found: {0}")]
    DisputeNotFound(String),

    /// The requested forfeit record was not found.
    #[error("forfeit record not found: {0}")]
    ForfeitRecordNotFound(String),

    /// The requested evidence was not found.
    #[error("evidence not found: {0}")]
    EvidenceNotFound(String),

    /// The requested result claim was not found.
    #[error("result claim not found: {0}")]
    ResultClaimNotFound(String),

    /// The requested veto session was not found.
    #[error("veto session not found: {0}")]
    VetoSessionNotFound(String),

    /// The requested demo was not found.
    #[error("demo not found: {0}")]
    DemoNotFound(String),

    /// The requested demo-match link was not found.
    #[error("demo-match link not found: {0}")]
    DemoMatchLinkNotFound(String),

    /// Demo is not linked to the specified match.
    #[error("demo link {0} is not linked to match {1}")]
    DemoNotLinkedToMatch(String, String),

    /// The requested result review was not found.
    #[error("result review not found: {0}")]
    ResultReviewNotFound(String),

    /// Invalid review state for the requested operation.
    #[error("invalid review state '{0}': {1}")]
    InvalidReviewState(String, String),

    /// The review has already been acknowledged by the specified captain.
    #[error("review already acknowledged by captain {0}")]
    ReviewAlreadyAcknowledged(i32),

    /// Tournament is not open for registration.
    #[error("tournament is not open for registration")]
    TournamentNotOpen,

    /// Tournament registration has closed.
    #[error("tournament registration has closed")]
    TournamentRegistrationClosed,

    /// Tournament has already started and cannot be modified.
    #[error("tournament has already started")]
    TournamentAlreadyStarted,

    /// Tournament is at maximum capacity.
    #[error("tournament is at maximum capacity")]
    TournamentFull,

    /// Participant is already registered for this tournament.
    #[error("already registered for this tournament")]
    AlreadyRegisteredForTournament,

    /// Participant is not registered for this tournament.
    #[error("not registered for this tournament")]
    NotRegisteredForTournament,

    /// Tournament registration is still pending approval.
    #[error("tournament registration is pending approval")]
    TournamentRegistrationPending,

    /// Participant has not checked in for the tournament.
    #[error("participant has not checked in")]
    NotCheckedIn,

    /// Match is not ready to start.
    #[error("match is not ready")]
    MatchNotReady,

    /// Match has already been completed.
    #[error("match has already been completed")]
    MatchAlreadyCompleted,

    /// The submitted match result is invalid.
    #[error("invalid match result: {0}")]
    InvalidMatchResult(String),

    /// Bracket generation failed.
    #[error("bracket generation failed: {0}")]
    BracketGenerationFailed(String),

    /// Not enough participants to start the tournament.
    #[error("insufficient participants for tournament")]
    InsufficientParticipants,

    /// Invalid tournament state transition.
    #[error("invalid tournament state transition: {from} -> {to}")]
    InvalidTournamentTransition {
        /// The current state.
        from: String,
        /// The attempted target state.
        to: String,
    },

    /// The user is not authorized to perform this action.
    #[error("not authorized: {0}")]
    NotAuthorized(String),

    /// The user is forbidden from accessing this resource.
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// The authentication token is invalid or missing.
    #[error("invalid or missing token")]
    InvalidToken,

    /// The authentication token has expired.
    #[error("token has expired")]
    TokenExpired,

    /// The provided credentials are invalid.
    #[error("invalid credentials")]
    InvalidCredentials,

    /// The player is already a member of this team.
    #[error("player is already a member of this team")]
    AlreadyTeamMember,

    /// The player is not a member of this team.
    #[error("player is not a member of this team")]
    NotTeamMember,

    /// Cannot remove the team founder from the team.
    #[error("cannot remove the team founder")]
    CannotRemoveFounder,

    /// Cannot demote the team founder to a lower role.
    #[error("cannot demote the team founder")]
    CannotDemoteFounder,

    /// The team must have at least one captain.
    #[error("team must have at least one captain")]
    TeamMustHaveCaptain,

    /// The team invitation has expired.
    #[error("team invitation has expired")]
    InvitationExpired,

    /// The team invitation is not found or has already been used.
    #[error("team invitation not found or already used")]
    InvitationInvalid,

    /// The player already has a pending invitation to this team.
    #[error("player already has a pending invitation to this team")]
    InvitationAlreadyExists,

    /// The team has reached its maximum member count.
    #[error("team has reached maximum member count")]
    TeamFull,

    /// The player is already in a matchmaking queue.
    #[error("player is already in queue")]
    AlreadyInQueue,

    /// The player is not in any matchmaking queue.
    #[error("player is not in queue")]
    NotInQueue,

    /// The match has already started.
    #[error("match has already started")]
    MatchAlreadyStarted,

    /// The match has already ended.
    #[error("match has already ended")]
    MatchAlreadyEnded,

    /// The lobby is full.
    #[error("lobby is full")]
    LobbyFull,

    /// The player is already in a lobby.
    #[error("player is already in a lobby")]
    AlreadyInLobby,

    /// The player is not in the lobby.
    #[error("player is not in lobby")]
    NotInLobby,

    /// Invalid lobby state transition.
    #[error("invalid lobby state transition: {from} -> {to}")]
    InvalidLobbyTransition {
        /// The current state of the lobby.
        from: String,
        /// The attempted target state.
        to: String,
    },

    /// Tournament or league registration is closed.
    #[error("tournament registration is closed")]
    RegistrationClosed,

    /// The team is already registered for this tournament.
    #[error("team is already registered for this tournament")]
    AlreadyRegistered,

    /// The team does not meet tournament requirements.
    #[error("team does not meet tournament requirements")]
    RequirementsNotMet(String),

    /// The user is not a member of this league.
    #[error("not a league member")]
    NotLeagueMember,

    /// League membership is invite-only.
    #[error("league membership is invite-only")]
    LeagueInviteOnly,

    /// Rating calculation failed.
    #[error("rating calculation failed: {0}")]
    RatingCalculationFailed(String),

    /// A saga (distributed transaction) failed.
    #[error("saga failed: {0}")]
    SagaFailed(String),

    /// A step in a saga failed.
    #[error("saga step failed: {step} - {reason}")]
    SagaStepFailed {
        /// The name of the failed step.
        step: String,
        /// The reason for the failure.
        reason: String,
    },

    /// A saga is paused waiting for external resolution (e.g., review).
    #[error("saga paused: {0}")]
    SagaPaused(String),

    /// The result was rejected by a review.
    #[error("result rejected by review: {0}")]
    ResultRejectedByReview(String),

    /// A conflict occurred (e.g., duplicate resource).
    #[error("conflict: {0}")]
    Conflict(String),

    /// The entity is in an invalid state for the requested operation.
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// Validation failed.
    #[error("validation failed")]
    Validation(#[from] ValidationError),

    /// An internal error occurred.
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
            "tournament stage" => Self::TournamentStageNotFound(id),
            "tournament bracket" => Self::TournamentBracketNotFound(id),
            "tournament match" => Self::TournamentMatchNotFound(id),
            "tournament registration" => Self::TournamentRegistrationNotFound(id),
            "dispute" => Self::DisputeNotFound(id),
            "forfeit record" => Self::ForfeitRecordNotFound(id),
            "evidence" => Self::EvidenceNotFound(id),
            "result claim" => Self::ResultClaimNotFound(id),
            "veto session" => Self::VetoSessionNotFound(id),
            "league" => Self::LeagueNotFound(id),
            "lobby" => Self::LobbyNotFound(id),
            "league season" => Self::LeagueSeasonNotFound(id),
            "league team" => Self::LeagueTeamNotFound(id),
            "league team invitation" => Self::LeagueTeamInvitationNotFound(id),
            "ban" => Self::BanNotFound(id),
            "demo" => Self::DemoNotFound(id),
            "demo match link" => Self::DemoMatchLinkNotFound(id),
            "result review" => Self::ResultReviewNotFound(id),
            _ => Self::Internal(format!("{entity_type} not found: {id}")),
        }
    }

    /// Check if this is a "not found" type error.
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::UserNotFound(_)
                | Self::PlayerNotFound(_)
                | Self::TeamNotFound(_)
                | Self::GameNotFound(_)
                | Self::MatchNotFound(_)
                | Self::TournamentNotFound(_)
                | Self::TournamentStageNotFound(_)
                | Self::TournamentBracketNotFound(_)
                | Self::TournamentMatchNotFound(_)
                | Self::TournamentRegistrationNotFound(_)
                | Self::DisputeNotFound(_)
                | Self::ForfeitRecordNotFound(_)
                | Self::EvidenceNotFound(_)
                | Self::ResultClaimNotFound(_)
                | Self::VetoSessionNotFound(_)
                | Self::LeagueNotFound(_)
                | Self::LobbyNotFound(_)
                | Self::LeagueSeasonNotFound(_)
                | Self::LeagueTeamNotFound(_)
                | Self::LeagueTeamInvitationNotFound(_)
                | Self::BanNotFound(_)
                | Self::DemoNotFound(_)
                | Self::DemoMatchLinkNotFound(_)
                | Self::ResultReviewNotFound(_)
        )
    }

    /// Check if this is an authorization error.
    #[must_use]
    pub const fn is_auth_error(&self) -> bool {
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
