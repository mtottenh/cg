//! `PostgreSQL` adapters for league team repositories.
//!
//! This module contains `PostgreSQL` implementations of the league team repository traits
//! defined in portal-domain. Each repository handles a specific entity:
//!
//! - `PgLeagueSeasonRepository`: League seasons
//! - `PgLeagueTeamRepository`: Persistent league teams
//! - `PgLeagueTeamSeasonRepository`: Seasonal team participation
//! - `PgLeagueTeamMemberRepository`: Seasonal team rosters
//! - `PgLeagueTeamInvitationRepository`: Team invitations
//! - `PgLeagueSeasonParticipantRepository`: Individual format participants

mod conversions;
mod invitation;
mod member;
mod participant;
mod season;
mod team;
mod team_season;

pub use invitation::PgLeagueTeamInvitationRepository;
pub use member::PgLeagueTeamMemberRepository;
pub use participant::PgLeagueSeasonParticipantRepository;
pub use season::PgLeagueSeasonRepository;
pub use team::PgLeagueTeamRepository;
pub use team_season::PgLeagueTeamSeasonRepository;
