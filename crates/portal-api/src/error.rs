//! API error types following RFC 7807.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use portal_core::{DomainError, ValidationError};
use serde::Serialize;
use utoipa::ToSchema;

/// API error response following RFC 7807 Problem Details.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiError {
    /// A URI reference identifying the problem type.
    #[serde(rename = "type")]
    #[schema(example = "https://api.gaming-portal.com/problems/validation-error")]
    pub error_type: String,

    /// A short, human-readable summary.
    #[schema(example = "Validation Failed")]
    pub title: String,

    /// The HTTP status code.
    #[schema(example = 400)]
    pub status: u16,

    /// A detailed explanation of the error.
    #[schema(example = "One or more fields failed validation")]
    pub detail: String,

    /// URI of the request that caused the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,

    /// Field-level validation errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FieldErrorDto>>,
}

/// A single field validation error.
#[derive(Debug, Serialize, ToSchema)]
pub struct FieldErrorDto {
    /// The field name.
    #[schema(example = "name")]
    pub field: String,

    /// Human-readable error message.
    #[schema(example = "Name must be between 3 and 64 characters")]
    pub message: String,

    /// Machine-readable error code.
    #[schema(example = "length")]
    pub code: String,
}

impl ApiError {
    /// Create a new API error.
    pub fn new(status: StatusCode, title: impl Into<String>, detail: impl Into<String>) -> Self {
        let title_str = title.into();
        Self {
            error_type: format!(
                "https://api.gaming-portal.com/problems/{}",
                title_str.to_lowercase().replace(' ', "-")
            ),
            title: title_str,
            status: status.as_u16(),
            detail: detail.into(),
            instance: None,
            errors: None,
        }
    }

    /// Create a not found error.
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not Found", detail)
    }

    /// Create an unauthorized error.
    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Unauthorized", detail)
    }

    /// Create a forbidden error.
    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "Forbidden", detail)
    }

    /// Create a bad request error.
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "Bad Request", detail)
    }

    /// Create a conflict error.
    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "Conflict", detail)
    }

    /// Create an internal server error.
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error",
            detail,
        )
    }

    /// Create a not implemented error.
    pub fn not_implemented(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_IMPLEMENTED, "Not Implemented", detail)
    }

    /// Create a validation error with field errors.
    pub fn validation(errors: Vec<FieldErrorDto>) -> Self {
        Self {
            error_type: "https://api.gaming-portal.com/problems/validation-error".to_string(),
            title: "Validation Failed".to_string(),
            status: 400,
            detail: "One or more fields failed validation".to_string(),
            instance: None,
            errors: Some(errors),
        }
    }

    /// Set the instance URI.
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(self)).into_response();
        // RFC 7807 mandates application/problem+json for problem details.
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            // Not found errors
            DomainError::UserNotFound(id) => Self::not_found(format!("User not found: {id}")),
            DomainError::PlayerNotFound(id) => Self::not_found(format!("Player not found: {id}")),
            DomainError::TeamNotFound(id) => Self::not_found(format!("Team not found: {id}")),
            DomainError::GameNotFound(id) => Self::not_found(format!("Game not found: {id}")),
            DomainError::MatchNotFound(id) => Self::not_found(format!("Match not found: {id}")),
            DomainError::TournamentNotFound(id) => {
                Self::not_found(format!("Tournament not found: {id}"))
            }
            DomainError::LeagueNotFound(id) => Self::not_found(format!("League not found: {id}")),
            DomainError::LeagueSeasonNotFound(id) => {
                Self::not_found(format!("League season not found: {id}"))
            }
            DomainError::LeagueTeamNotFound(id) => {
                Self::not_found(format!("League team not found: {id}"))
            }
            DomainError::LeagueTeamInvitationNotFound(id) => {
                Self::not_found(format!("League team invitation not found: {id}"))
            }
            DomainError::LobbyNotFound(id) => Self::not_found(format!("Lobby not found: {id}")),
            DomainError::BanNotFound(id) => Self::not_found(format!("Ban not found: {id}")),
            DomainError::TournamentStageNotFound(id) => {
                Self::not_found(format!("Tournament stage not found: {id}"))
            }
            DomainError::TournamentBracketNotFound(id) => {
                Self::not_found(format!("Tournament bracket not found: {id}"))
            }
            DomainError::TournamentMatchNotFound(id) => {
                Self::not_found(format!("Tournament match not found: {id}"))
            }
            DomainError::TournamentRegistrationNotFound(id) => {
                Self::not_found(format!("Tournament registration not found: {id}"))
            }
            DomainError::DisputeNotFound(id) => {
                Self::not_found(format!("Dispute not found: {id}"))
            }
            DomainError::ForfeitRecordNotFound(id) => {
                Self::not_found(format!("Forfeit record not found: {id}"))
            }
            DomainError::EvidenceNotFound(id) => {
                Self::not_found(format!("Evidence not found: {id}"))
            }
            DomainError::ResultClaimNotFound(id) => {
                Self::not_found(format!("Result claim not found: {id}"))
            }
            DomainError::VetoSessionNotFound(id) => {
                Self::not_found(format!("Veto session not found: {id}"))
            }
            DomainError::DemoNotFound(id) => {
                Self::not_found(format!("Demo not found: {id}"))
            }
            DomainError::DemoMatchLinkNotFound(id) => {
                Self::not_found(format!("Demo-match link not found: {id}"))
            }
            DomainError::DemoNotLinkedToMatch(link_id, match_id) => {
                Self::bad_request(format!("Demo link {link_id} is not linked to match {match_id}"))
            }
            DomainError::ResultReviewNotFound(id) => {
                Self::not_found(format!("Result review not found: {id}"))
            }
            DomainError::LookupFailed { resource, query } => {
                Self::not_found(format!("{resource} not found: {query}"))
            }
            DomainError::InvalidReviewState(status, msg) => {
                Self::conflict(format!("Invalid review state '{status}': {msg}"))
            }
            DomainError::ReviewAlreadyAcknowledged(captain) => {
                Self::conflict(format!("Review already acknowledged by captain {captain}"))
            }

            // Authorization errors
            DomainError::NotAuthorized(msg) => Self::unauthorized(msg),
            DomainError::Forbidden(msg) => Self::forbidden(msg),

            // Authentication errors
            DomainError::InvalidToken => Self::unauthorized("Invalid or missing token"),
            DomainError::TokenExpired => Self::unauthorized("Token has expired"),
            DomainError::RefreshTokenExpired => Self::unauthorized("Refresh token has expired"),
            DomainError::RefreshTokenRevoked => Self::unauthorized("Refresh token has been revoked"),
            DomainError::InvalidCredentials => Self::unauthorized("Invalid credentials"),

            // Conflict errors
            DomainError::AlreadyTeamMember => {
                Self::conflict("Player is already a member of this team")
            }
            DomainError::AlreadyInQueue => Self::conflict("Player is already in queue"),
            DomainError::AlreadyInLobby => Self::conflict("Player is already in a lobby"),
            DomainError::AlreadyRegistered => {
                Self::conflict("Team is already registered for this tournament")
            }
            DomainError::AlreadyRegisteredForTournament => {
                Self::conflict("Already registered for this tournament")
            }
            DomainError::Conflict(msg) => Self::conflict(msg),
            DomainError::InvitationAlreadyExists => {
                Self::conflict("Player already has a pending invitation")
            }

            // Bad request errors
            DomainError::NotTeamMember => Self::bad_request("Player is not a member of this team"),
            DomainError::CannotRemoveFounder => Self::bad_request("Cannot remove the team founder"),
            DomainError::CannotDemoteFounder => Self::bad_request("Cannot demote the team founder"),
            DomainError::TeamMustHaveCaptain => {
                Self::bad_request("Team must have at least one captain")
            }
            DomainError::InvitationExpired => Self::bad_request("Invitation has expired"),
            DomainError::InvitationInvalid => Self::bad_request("Invitation is invalid or already used"),
            DomainError::TeamFull => Self::bad_request("Team has reached maximum member count"),
            DomainError::NotInQueue => Self::bad_request("Player is not in queue"),
            DomainError::MatchAlreadyStarted => Self::bad_request("Match has already started"),
            DomainError::MatchAlreadyEnded => Self::bad_request("Match has already ended"),
            DomainError::LobbyFull => Self::bad_request("Lobby is full"),
            DomainError::NotInLobby => Self::bad_request("Player is not in lobby"),
            DomainError::InvalidLobbyTransition { from, to } => {
                Self::bad_request(format!("Invalid lobby transition: {from} -> {to}"))
            }
            DomainError::RegistrationClosed => Self::bad_request("Tournament registration is closed"),
            DomainError::RequirementsNotMet(msg) => {
                Self::bad_request(format!("Requirements not met: {msg}"))
            }
            DomainError::NotLeagueMember => Self::bad_request("Not a league member"),
            DomainError::LeagueInviteOnly => Self::bad_request("League is invite-only"),
            DomainError::InvalidState(msg) => Self::bad_request(format!("Invalid state: {msg}")),

            // Tournament-specific errors
            DomainError::TournamentNotOpen => {
                Self::bad_request("Tournament is not open for registration")
            }
            DomainError::TournamentRegistrationClosed => {
                Self::bad_request("Tournament registration has closed")
            }
            DomainError::TournamentAlreadyStarted => {
                Self::bad_request("Tournament has already started")
            }
            DomainError::TournamentFull => Self::bad_request("Tournament is at maximum capacity"),
            DomainError::EligibilityViolation(msg) => Self::bad_request(msg),
            DomainError::NotRegisteredForTournament => {
                Self::bad_request("Not registered for this tournament")
            }
            DomainError::TournamentRegistrationPending => {
                Self::bad_request("Tournament registration is pending approval")
            }
            DomainError::NotCheckedIn => Self::bad_request("Participant has not checked in"),
            DomainError::MatchNotReady => Self::bad_request("Match is not ready to start"),
            DomainError::MatchAlreadyCompleted => {
                Self::bad_request("Match has already been completed")
            }
            DomainError::InvalidMatchResult(msg) => {
                Self::bad_request(format!("Invalid match result: {msg}"))
            }
            DomainError::InsufficientParticipants => {
                Self::bad_request("Insufficient participants for tournament")
            }
            DomainError::InvalidTournamentTransition { from, to } => {
                Self::bad_request(format!("Invalid tournament state transition: {from} -> {to}"))
            }
            DomainError::BracketGenerationFailed(msg) => {
                Self::internal(format!("Bracket generation failed: {msg}"))
            }

            // Validation errors
            DomainError::Validation(validation_err) => {
                let field_errors: Vec<FieldErrorDto> = validation_err
                    .errors()
                    .iter()
                    .map(|e| FieldErrorDto {
                        field: e.field.clone(),
                        message: e.message.clone(),
                        code: e.code.clone(),
                    })
                    .collect();
                Self::validation(field_errors)
            }

            // Internal errors
            DomainError::RatingCalculationFailed(msg) => Self::internal(msg),
            DomainError::SagaFailed(msg) => Self::internal(format!("Operation failed: {msg}")),
            DomainError::SagaStepFailed { step, reason } => {
                Self::internal(format!("Operation failed at {step}: {reason}"))
            }
            DomainError::SagaPaused(msg) => {
                Self::conflict(format!("Operation paused: {msg}"))
            }
            DomainError::ResultRejectedByReview(msg) => {
                Self::conflict(format!("Result rejected by review: {msg}"))
            }
            DomainError::Internal(msg) => Self::internal(msg),
        }
    }
}

impl From<ValidationError> for ApiError {
    fn from(err: ValidationError) -> Self {
        let field_errors: Vec<FieldErrorDto> = err
            .errors()
            .iter()
            .map(|e| FieldErrorDto {
                field: e.field.clone(),
                message: e.message.clone(),
                code: e.code.clone(),
            })
            .collect();
        Self::validation(field_errors)
    }
}

impl From<validator::ValidationErrors> for ApiError {
    fn from(err: validator::ValidationErrors) -> Self {
        let field_errors: Vec<FieldErrorDto> = err
            .field_errors()
            .iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |e| FieldErrorDto {
                    field: (*field).to_string(),
                    message: e
                        .message
                        .as_ref().map_or_else(|| format!("Invalid value for {field}"), std::string::ToString::to_string),
                    code: e.code.to_string(),
                })
            })
            .collect();
        Self::validation(field_errors)
    }
}

impl From<portal_db::RepositoryError> for ApiError {
    fn from(err: portal_db::RepositoryError) -> Self {
        // Convert through DomainError for consistent error handling
        let domain_err: DomainError = err.into();
        domain_err.into()
    }
}

/// Result type for API handlers.
pub type ApiResult<T> = Result<T, ApiError>;
