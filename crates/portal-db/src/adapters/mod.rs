//! Adapters that implement domain repository traits.
//!
//! These adapters bridge the gap between the database layer and the domain layer
//! by implementing domain traits and converting between db row types and domain entities.

mod audit;
mod ban;
mod league;
mod league_team;
mod permission;
mod tournament;
mod user;

pub use audit::PgEntityChangeRepository;
pub use ban::PgBanRepository;
pub use league::{PgLeagueInvitationRepository, PgLeagueMemberRepository, PgLeagueRepository};
pub use league_team::{
    PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository, PgLeagueTeamInvitationRepository,
    PgLeagueTeamMemberRepository, PgLeagueTeamRepository, PgLeagueTeamSeasonRepository,
};
pub use permission::PgPermissionRepository;
pub use tournament::{
    PgTournamentBracketRepository, PgTournamentMapPoolRepository, PgTournamentMatchGameRepository,
    PgTournamentMatchRepository, PgTournamentRegistrationRepository, PgTournamentRepository,
    PgTournamentStageRepository, PgTournamentStandingsRepository,
};
pub use user::{PgPlayerRepository, PgUserRepository};
