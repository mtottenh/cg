//! Tournament services.
//!
//! This module contains the business logic for tournament operations:
//!
//! - `TournamentService`: Core tournament management
//! - `BracketGenerator`: Bracket generation for various formats
//! - `RegistrationService`: Registration management (withdraw, approve, reject)
//! - `CheckInService`: Check-in operations
//! - `SeedingService`: Participant seeding algorithms
//! - `MatchLifecycleService`: Match state machine and lifecycle management
//! - `SchedulingService`: Match scheduling through proposals
//! - `AvailabilityService`: Availability windows and time suggestions
//! - `VetoService`: Map veto (pick/ban) system
//! - `ResultService`: Match result submission and confirmation
//! - `EvidenceService`: Evidence upload and management
//! - `ProgressionService`: Bracket progression after match completion
//! - `StandingsService`: Standings calculation and tiebreakers
//! - `SagaCoordinator`: Multi-step operation orchestration
//! - `ForfeitService`: Forfeit handling (no-show, withdrawal, disqualification)
//! - `DisputeService`: Dispute workflow and admin resolution

mod availability;
mod bracket_generator;
mod checkin;
mod dispute;
mod evidence;
mod forfeit;
mod match_completion;
mod match_lifecycle;
mod progression;
mod registration;
mod result;
mod result_review;
mod saga;
mod scheduling;
mod seeding;
mod service;
mod standings;
mod veto;
mod veto_authorization;
mod veto_lobby_chat;

pub use availability::AvailabilityService;
pub use bracket_generator::{BracketGenerator, GeneratedBracket};
pub use checkin::{CheckInService, CheckInStatus};
pub use evidence::{EvidencePluginClient, EvidenceS3Client, EvidenceService, EvidenceServiceConfig};
pub use match_completion::{
    DemoValidationOutcome, MatchCompletionInput, MatchCompletionOutput, MatchCompletionSaga,
    MatchDemoValidator, MatchStatsUpdater, ReviewCreator,
};
pub use match_lifecycle::{MatchLifecycleService, MatchStatusDetails};
pub use progression::{Advancement, LoserResult, ProgressionResult, ProgressionService};
pub use registration::RegistrationService;
pub use result::ResultService;
pub use saga::{Saga, SagaCoordinator, SagaDefinition, SagaResult, SagaStep};
pub use scheduling::SchedulingService;
pub use seeding::{SeededParticipant, SeedingService};
pub use service::TournamentService;
pub use standings::StandingsService;
pub use veto::VetoService;
pub use veto_authorization::{VetoAuthorizationRole, VetoAuthorizationService};
pub use forfeit::ForfeitService;
pub use dispute::DisputeService;
pub use result_review::ResultReviewService;
pub use veto_lobby_chat::{VetoLobbyChatConfig, VetoLobbyChatService};
