//! Adapters that implement domain repository traits.
//!
//! These adapters bridge the gap between the database layer and the domain layer
//! by implementing domain traits and converting between db row types and domain entities.

mod api_key;
mod audit;
mod availability;
mod award;
mod ban;
mod demo;
mod demo_stats;
mod discovered_match;
mod dispute;
mod evidence;
mod forfeit;
mod league;
mod league_team;
mod permission;
mod player_game_profile;
mod player_match_history;
mod player_mm_stats;
mod player_rating_history;
mod refresh_token;
mod result_review;
mod saga;
mod steam_tracking;
mod tournament;
mod user;
mod veto_delegate;
mod veto_lobby_message;

pub use api_key::PgApiKeyRepository;
pub use audit::PgEntityChangeRepository;
pub use availability::{
    PgAvailabilityOverrideRepository, PgAvailabilityWindowRepository, PgSuggestedTimeRepository,
};
pub use award::PgAwardRepository;
pub use ban::PgBanRepository;
pub use demo::{PgDemoMatchLinkRepository, PgDemoPlayerRepository, PgDemoRepository};
pub use demo_stats::PgDemoPlayerStatsRepository;
pub use discovered_match::PgDiscoveredMatchRepository;
pub use dispute::{PgDisputeMessageRepository, PgDisputeRepository};
pub use evidence::PgEvidenceRepository;
pub use forfeit::PgForfeitRecordRepository;
pub use league::{PgLeagueInvitationRepository, PgLeagueMemberRepository, PgLeagueRepository};
pub use league_team::{
    PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository,
    PgLeagueTeamInvitationRepository, PgLeagueTeamMemberRepository, PgLeagueTeamRepository,
    PgLeagueTeamSeasonRepository,
};
pub use permission::PgPermissionRepository;
pub use player_game_profile::PgPlayerGameProfileRepository;
pub use player_match_history::PgPlayerMatchHistoryRepository;
pub use player_mm_stats::PgPlayerMmStatsRepository;
pub use player_rating_history::PgPlayerRatingHistoryRepository;
pub use refresh_token::PgRefreshTokenRepository;
pub use result_review::PgResultReviewRepository;
pub use saga::{PgProgressionLogRepository, PgSagaExecutionRepository};
pub use steam_tracking::PgSteamTrackingRepository;
pub use tournament::{
    MatchCompletionTxInput, MatchCompletionTxOutput, PgMatchStatusLogRepository,
    PgResultClaimRepository, PgScheduleProposalRepository, PgTournamentBracketRepository,
    PgTournamentMapPoolRepository, PgTournamentMatchGameRepository, PgTournamentMatchRepository,
    PgTournamentRegistrationRepository, PgTournamentRepository, PgTournamentStageRepository,
    PgTournamentStandingsRepository, PgVetoActionRepository, PgVetoSessionRepository,
    complete_match_in_transaction,
};
pub use user::{PgPlayerRepository, PgUserRepository};
pub use veto_delegate::PgVetoDelegateRepository;
pub use veto_lobby_message::PgVetoLobbyMessageRepository;
