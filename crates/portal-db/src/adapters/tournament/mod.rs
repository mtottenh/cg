//! `PostgreSQL` adapters for tournament repositories.
//!
//! This module contains `PostgreSQL` implementations of the tournament repository traits
//! defined in portal-domain. Each repository handles a specific entity:
//!
//! - `PgTournamentRepository`: Core tournament CRUD
//! - `PgTournamentStageRepository`: Multi-stage tournament stages
//! - `PgTournamentBracketRepository`: Bracket structures
//! - `PgTournamentRegistrationRepository`: Participant registrations
//! - `PgTournamentMatchRepository`: Matches within brackets
//! - `PgTournamentMatchGameRepository`: Individual games in a match series
//! - `PgTournamentStandingsRepository`: Round robin/swiss standings
//! - `PgTournamentMapPoolRepository`: Map pool configuration
//! - `PgMatchStatusLogRepository`: Match status transition logs
//! - `PgVetoSessionRepository`: Map veto sessions
//! - `PgVetoActionRepository`: Individual veto actions
//! - `PgResultClaimRepository`: Match result claims

mod bracket;
mod conversions;
mod map_pool;
mod match_;
mod match_completion_tx;
mod match_game;
mod match_status_log;
mod registration;
mod result_claim;
mod schedule_proposal;
mod stage;
mod standings;
mod tournament;
mod veto;

pub use bracket::PgTournamentBracketRepository;
pub use map_pool::PgTournamentMapPoolRepository;
pub use match_::PgTournamentMatchRepository;
pub use match_completion_tx::{
    MatchCompletionTxInput, MatchCompletionTxOutput, complete_match_in_transaction,
};
pub use match_game::PgTournamentMatchGameRepository;
pub use match_status_log::PgMatchStatusLogRepository;
pub use registration::PgTournamentRegistrationRepository;
pub use result_claim::PgResultClaimRepository;
pub use schedule_proposal::PgScheduleProposalRepository;
pub use stage::PgTournamentStageRepository;
pub use standings::PgTournamentStandingsRepository;
pub use tournament::PgTournamentRepository;
pub use veto::{PgVetoActionRepository, PgVetoSessionRepository};
