//! Repository traits for data access.
//!
//! These traits define the interface between domain services and data storage.
//! Implementations are in the `portal-db` crate.

pub mod api_key;
pub mod audit;
pub mod availability;
pub mod award;
pub mod ban;
pub mod demo;
pub mod demo_stats;
pub mod discovered_match;
pub mod dispute;
pub mod evidence;
pub mod forfeit;
pub mod league;
pub mod league_team;
pub mod match_lifecycle;
pub mod permission;
pub mod player_game_profile;
pub mod player_match_history;
pub mod player_mm_stats;
pub mod player_rating_history;
pub mod refresh_token;
pub mod result_review;
pub mod schedule_proposal;
pub mod steam_tracking;
pub mod system_settings;
pub mod tournament;
pub mod user;
pub mod veto_delegate;
pub mod veto_lobby_message;

pub use api_key::{ApiKeyRepository, CreateApiKey};
pub use audit::{CreateEntityChange, EntityChangeRepository};
pub use availability::{
    AvailabilityOverrideRepository, AvailabilityWindowRepository, SuggestedTimeRepository,
};
pub use award::{
    AwardRepository, CreateAward, CreateAwardResult, PlayerTrophy, UpdateAwardPresentation,
};
pub use ban::{BanRepository, PaginatedBans, PaginationMeta};
pub use demo::{
    CreateDemo, CreateDemoMatchLink, CreateDemoPlayer, DemoMatchLinkRepository,
    DemoMatchLinkWithData, DemoPlayerRepository, DemoRepository,
};
pub use demo_stats::{
    CURRENT_EXTRACTOR_VERSION, DemoPlayerStatsRepository, DemoStatFact, LeaderboardEntry,
    LeaderboardQuery, LeaderboardScope,
};
pub use discovered_match::{CreateDiscoveredMatch, DiscoveredMatchRepository};
pub use dispute::{
    CreateDispute, CreateDisputeMessage, DisputeMessageRepository, DisputeRepository, UpdateDispute,
};
pub use evidence::{
    CreateEvidence, CreateEvidenceAccessLog, CreateProgressionLog, CreateSagaExecution,
    EvidenceRepository, ProgressionLog, ProgressionLogRepository, ProgressionType,
    SagaExecutionRepository, UpdateEvidence,
};
pub use forfeit::{CreateForfeitRecord, ForfeitRecordRepository};
pub use league::{
    AddLeagueMember, CreateLeague, CreateLeagueInvitation, LeagueInvitationRepository,
    LeagueMemberRepository, LeagueRepository, UpdateLeague,
};
pub use league_team::{
    AddLeagueTeamMember, CreateLeagueSeason, CreateLeagueTeam, CreateLeagueTeamInvitation,
    LeagueSeasonRepository, LeagueTeamInvitationRepository, LeagueTeamMemberRepository,
    LeagueTeamRepository, LeagueTeamSeasonRepository, UpdateLeagueSeason, UpdateLeagueTeam,
};
pub use match_lifecycle::{CreateMatchStatusLog, MatchStatusLogRepository};
pub use permission::PermissionRepository;
pub use player_game_profile::PlayerGameProfileRepository;
pub use player_match_history::{CreatePlayerMatchHistory, PlayerMatchHistoryRepository};
pub use player_mm_stats::{AccumulateMatchStats, PlayerMmStatsRepository};
pub use player_rating_history::{
    CreatePlayerRatingHistory, PlayerRatingHistoryRepository, RatingStats,
};
pub use refresh_token::RefreshTokenRepository;
pub use result_review::{CreateResultReview, ResultReviewRepository};
pub use schedule_proposal::ScheduleProposalRepository;
pub use steam_tracking::{CreateSteamTracking, SteamTrackingRepository};
pub use tournament::{
    CreateResultClaim, CreateTournament, CreateTournamentBracket, CreateTournamentMatch,
    CreateTournamentMatchGame, CreateTournamentRegistration, CreateTournamentStage,
    CreateTournamentStanding, CreateVetoAction, CreateVetoSession, ParticipantSlot,
    ResultClaimRepository, TournamentBracketRepository, TournamentFilters,
    TournamentMapPoolRepository, TournamentMatchGameRepository, TournamentMatchRepository,
    TournamentRegistrationRepository, TournamentRepository, TournamentStageRepository,
    TournamentStandingsRepository, UpdateResultClaim, UpdateTournament, UpdateTournamentBracket,
    UpdateTournamentMatch, UpdateTournamentMatchGame, UpdateTournamentRegistration,
    UpdateTournamentStage, UpdateTournamentStanding, UpdateVetoSession, UpsertTournamentMapPool,
    VetoActionRepository, VetoSessionRepository,
};
pub use user::{
    CreatePlayer, CreateUser, PlayerRepository, PlayerSearchFilters, UpdatePlayer, UserRepository,
};
pub use veto_delegate::{CreateVetoDelegate, VetoDelegateRepository};
pub use veto_lobby_message::{CreateVetoLobbyMessage, VetoLobbyMessageRepository};
