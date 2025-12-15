//! Adapters that implement domain repository traits.
//!
//! These adapters bridge the gap between the database layer and the domain layer
//! by implementing domain traits and converting between db row types and domain entities.

mod audit;
mod availability;
mod ban;
mod demo;
mod dispute;
mod evidence;
mod forfeit;
mod league;
mod league_team;
mod permission;
mod result_review;
mod tournament;
mod user;
mod veto_delegate;
mod veto_lobby_message;

pub use audit::PgEntityChangeRepository;
pub use availability::{
    PgAvailabilityOverrideRepository, PgAvailabilityWindowRepository, PgSuggestedTimeRepository,
};
pub use ban::PgBanRepository;
pub use demo::{PgDemoMatchLinkRepository, PgDemoPlayerRepository, PgDemoRepository};
pub use evidence::{LocalEvidenceStorage, PgEvidenceRepository};
pub use league::{PgLeagueInvitationRepository, PgLeagueMemberRepository, PgLeagueRepository};
pub use league_team::{
    PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository, PgLeagueTeamInvitationRepository,
    PgLeagueTeamMemberRepository, PgLeagueTeamRepository, PgLeagueTeamSeasonRepository,
};
pub use permission::PgPermissionRepository;
pub use tournament::{
    complete_match_in_transaction, MatchCompletionTxInput, MatchCompletionTxOutput,
    PgMatchStatusLogRepository, PgResultClaimRepository, PgScheduleProposalRepository,
    PgTournamentBracketRepository, PgTournamentMapPoolRepository, PgTournamentMatchGameRepository,
    PgTournamentMatchRepository, PgTournamentRegistrationRepository, PgTournamentRepository,
    PgTournamentStageRepository, PgTournamentStandingsRepository, PgVetoActionRepository,
    PgVetoSessionRepository,
};
pub use user::{PgPlayerRepository, PgUserRepository};
pub use forfeit::PgForfeitRecordRepository;
pub use dispute::{PgDisputeMessageRepository, PgDisputeRepository};
pub use result_review::PgResultReviewRepository;
pub use veto_delegate::PgVetoDelegateRepository;
pub use veto_lobby_message::PgVetoLobbyMessageRepository;
