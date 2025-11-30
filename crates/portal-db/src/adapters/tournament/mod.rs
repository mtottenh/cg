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

mod bracket;
mod conversions;
mod map_pool;
mod match_;
mod match_game;
mod registration;
mod stage;
mod standings;
mod tournament;

pub use bracket::PgTournamentBracketRepository;
pub use map_pool::PgTournamentMapPoolRepository;
pub use match_::PgTournamentMatchRepository;
pub use match_game::PgTournamentMatchGameRepository;
pub use registration::PgTournamentRegistrationRepository;
pub use stage::PgTournamentStageRepository;
pub use standings::PgTournamentStandingsRepository;
pub use tournament::PgTournamentRepository;
