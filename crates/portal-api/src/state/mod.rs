//! Application state for dependency injection.
//!
//! `AppState` is the workspace-wide composite state passed to
//! `.with_state(...)` at router construction. Domain-scoped projections
//! of it live in the [`substates`] sub-module and are pulled out of
//! `AppState` via `FromRef`. Prefer a sub-state in new handlers; it keeps
//! the dependency list of each handler narrow and self-documenting.

pub mod substates;

pub use substates::{
    AdminState, AuthState, AvailabilityState, AwardsState, BanState, DemoState, DisputeState,
    EvidenceState, ForfeitState, GamesState, InternalState, LeagueTeamState, LeaguesState,
    PlayerState, ProgressionState, ResultReviewState, ResultState, RolesState, SteamTrackingState,
    TournamentState, UploadsState, UsersState, VetoDelegatesState, VetoState, VetoWsState,
};

use crate::adapters::{
    DemoValidatorAdapter, PluginSideSelectionProvider, PluginVetoFormatProvider,
    ReviewCreatorAdapter, StatsUpdaterAdapter,
};
use crate::adapters::{EvidenceStorageBackend, LocalEvidenceStorage, S3EvidenceStorageAdapter};
use crate::steam_openid::{HttpSteamOpenIdVerifier, SteamAuthConfig, SteamOpenIdVerifier};
use crate::websocket::VetoLobbyManager;
use portal_db::{
    ActionItemRepository, DbPool, GameRepository, PermissionRepository, PgApiKeyRepository,
    PgAvailabilityOverrideRepository, PgAvailabilityWindowRepository, PgAwardRepository,
    PgBanRepository, PgDemoMatchLinkRepository, PgDemoPlayerRepository,
    PgDemoPlayerStatsRepository, PgDemoRepository, PgDiscoveredMatchRepository,
    PgDisputeMessageRepository, PgDisputeRepository, PgEvidenceRepository,
    PgForfeitRecordRepository, PgLeagueInvitationRepository, PgLeagueMemberRepository,
    PgLeagueRepository, PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository,
    PgLeagueTeamInvitationRepository, PgLeagueTeamMemberRepository, PgLeagueTeamRepository,
    PgLeagueTeamSeasonRepository, PgMatchStatusLogRepository, PgPermissionRepository,
    PgPlayerGameProfileRepository, PgPlayerMatchHistoryRepository, PgPlayerMmStatsRepository,
    PgPlayerRatingHistoryRepository, PgPlayerRepository, PgProgressionLogRepository,
    PgRefreshTokenRepository, PgResultClaimRepository, PgResultReviewRepository,
    PgSagaExecutionRepository, PgScheduleProposalRepository, PgSteamTrackingRepository,
    PgSuggestedTimeRepository, PgSystemSettingsRepository, PgTournamentBracketRepository,
    PgTournamentMapPoolRepository, PgTournamentMatchRepository, PgTournamentRegistrationRepository,
    PgTournamentRepository, PgTournamentStageRepository, PgTournamentStandingsRepository,
    PgUserRepository, PgVetoActionRepository, PgVetoDelegateRepository,
    PgVetoLobbyMessageRepository, PgVetoSessionRepository, RoleRepository, StatsRepository,
};
use portal_domain::services::{
    AwardService, BanService, DemoService, DiscoveredMatchService, LeagueSeasonParticipantService,
    LeagueSeasonService, LeagueService, LeagueTeamInvitationService, LeagueTeamService,
    PermissionService, PlayerGameProfileService, PlayerService, SteamTrackingService,
    SystemSettingsService, TournamentService, UserService,
    tournament::{
        AvailabilityService, CheckInService, DisputeService, EvidenceService,
        EvidenceServiceConfig, ForfeitService, MatchCompletionSaga, MatchLifecycleService,
        ProgressionService, RegistrationService, ResultReviewService, ResultService,
        SchedulingService, SeedingService, StandingsService, VetoAuthorizationService,
        VetoLobbyChatService, VetoService,
    },
};
use portal_plugins::PluginManager;
use portal_storage::{LocalStorage, StorageBackend};
use std::sync::Arc;

/// Type aliases for services with concrete repository implementations.
pub type AppSteamTrackingService =
    SteamTrackingService<PgSteamTrackingRepository, PgPlayerRepository>;
pub type AppSystemSettingsService = SystemSettingsService<PgSystemSettingsRepository>;
pub type AppDiscoveredMatchService = DiscoveredMatchService<PgDiscoveredMatchRepository>;
pub type AppUserService = UserService<PgUserRepository, PgPlayerRepository>;
pub type AppPlayerService = PlayerService<PgPlayerRepository, PgLeagueTeamMemberRepository>;
pub type AppPermissionService = PermissionService<PgPermissionRepository>;
pub type AppLeagueService =
    LeagueService<PgLeagueRepository, PgLeagueMemberRepository, PgLeagueInvitationRepository>;
pub type AppLeagueSeasonService = LeagueSeasonService<PgLeagueSeasonRepository, PgLeagueRepository>;
pub type AppLeagueTeamService = LeagueTeamService<
    PgLeagueTeamRepository,
    PgLeagueTeamSeasonRepository,
    PgLeagueTeamMemberRepository,
    PgLeagueSeasonRepository,
>;
pub type AppLeagueTeamInvitationService = LeagueTeamInvitationService<
    PgLeagueTeamInvitationRepository,
    PgLeagueTeamRepository,
    PgLeagueTeamSeasonRepository,
    PgLeagueTeamMemberRepository,
    PgLeagueSeasonRepository,
>;
pub type AppLeagueSeasonParticipantService =
    LeagueSeasonParticipantService<PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository>;
pub type AppPlayerGameProfileService = PlayerGameProfileService<PgPlayerGameProfileRepository>;
pub type AppEligibilityService = portal_domain::services::EligibilityService<
    PgPlayerGameProfileRepository,
    PgPlayerRatingHistoryRepository,
>;
pub type AppBanService = BanService<PgBanRepository, PgUserRepository, PgRefreshTokenRepository>;
pub type AppTournamentService = TournamentService<
    PgTournamentRepository,
    PgTournamentStageRepository,
    PgTournamentBracketRepository,
    PgTournamentRegistrationRepository,
    PgTournamentMatchRepository,
    PgTournamentStandingsRepository,
>;
pub type AppRegistrationService =
    RegistrationService<PgTournamentRepository, PgTournamentRegistrationRepository>;
pub type AppCheckInService =
    CheckInService<PgTournamentRepository, PgTournamentRegistrationRepository>;
pub type AppSeedingService =
    SeedingService<PgTournamentRepository, PgTournamentRegistrationRepository>;
pub type AppMatchLifecycleService = MatchLifecycleService<
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
    PgMatchStatusLogRepository,
>;
pub type AppSchedulingService = SchedulingService<
    PgScheduleProposalRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
>;
pub type AppAvailabilityService = AvailabilityService<
    PgAvailabilityWindowRepository,
    PgAvailabilityOverrideRepository,
    PgSuggestedTimeRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
>;
pub type AppVetoService =
    VetoService<PgVetoSessionRepository, PgVetoActionRepository, PgTournamentMatchRepository>;
pub type AppResultService = ResultService<
    PgResultClaimRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
    PgDemoMatchLinkRepository,
>;
pub type AppProgressionService = ProgressionService<
    PgTournamentMatchRepository,
    PgTournamentBracketRepository,
    PgTournamentStageRepository,
    PgTournamentRegistrationRepository,
    PgTournamentStandingsRepository,
>;
pub type AppEvidenceService = EvidenceService<
    PgEvidenceRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
    EvidenceStorageBackend,
>;
pub type AppForfeitService = ForfeitService<
    PgForfeitRecordRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
>;
pub type AppDisputeService = DisputeService<
    PgDisputeRepository,
    PgDisputeMessageRepository,
    PgTournamentMatchRepository,
    PgResultClaimRepository,
>;
pub type AppDemoService = DemoService<
    PgDemoRepository,
    PgDemoMatchLinkRepository,
    PgDemoPlayerRepository,
    PgTournamentMatchRepository,
>;
pub type AppAwardService = AwardService<PgAwardRepository, PgDemoPlayerStatsRepository>;
pub type AppResultReviewService =
    ResultReviewService<PgResultReviewRepository, PgTournamentMatchRepository>;
pub type AppStandingsService =
    StandingsService<PgTournamentStandingsRepository, PgTournamentMatchRepository>;
pub type AppVetoLobbyChatService = VetoLobbyChatService<
    PgVetoLobbyMessageRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
>;
pub type AppVetoAuthorizationService = VetoAuthorizationService<
    PgVetoDelegateRepository,
    PgTournamentRegistrationRepository,
    PgLeagueTeamSeasonRepository,
    PgLeagueTeamRepository,
    PgLeagueTeamMemberRepository,
    PgPermissionRepository,
>;
pub type AppStatsUpdaterAdapter = StatsUpdaterAdapter<
    PgTournamentMatchRepository,
    PgTournamentRepository,
    PgTournamentRegistrationRepository,
    PgDemoMatchLinkRepository,
>;
pub type AppMatchCompletionSaga = MatchCompletionSaga<
    PgTournamentMatchRepository,
    PgTournamentBracketRepository,
    PgTournamentRegistrationRepository,
    PgTournamentStandingsRepository,
    PgSagaExecutionRepository,
    PgProgressionLogRepository,
    DemoValidatorAdapter,
    ReviewCreatorAdapter,
    AppStatsUpdaterAdapter,
>;

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub db_pool: DbPool,
    /// JWT secret for token signing/verification.
    pub jwt_secret: Arc<str>,
    /// User service.
    pub user_service: AppUserService,
    /// Player service.
    pub player_service: AppPlayerService,
    /// Player game profile service.
    pub player_game_profile_service: AppPlayerGameProfileService,
    /// Eligibility checking service (shared by league + tournament handlers).
    pub eligibility_service: AppEligibilityService,
    /// League service.
    pub league_service: AppLeagueService,
    /// League season service.
    pub league_season_service: AppLeagueSeasonService,
    /// League team service.
    pub league_team_service: AppLeagueTeamService,
    /// League team invitation service.
    pub league_team_invitation_service: AppLeagueTeamInvitationService,
    /// League season participant service (for individual format).
    pub league_season_participant_service: AppLeagueSeasonParticipantService,
    /// Ban service.
    pub ban_service: AppBanService,
    /// Tournament service.
    pub tournament_service: AppTournamentService,
    /// Tournament registration service.
    pub registration_service: AppRegistrationService,
    /// Tournament check-in service.
    pub checkin_service: AppCheckInService,
    /// Tournament seeding service.
    pub seeding_service: AppSeedingService,
    /// Match lifecycle service.
    pub match_lifecycle_service: AppMatchLifecycleService,
    /// Match scheduling service.
    pub scheduling_service: AppSchedulingService,
    /// Availability service for player/participant availability.
    pub availability_service: AppAvailabilityService,
    /// Veto service for map pick/ban.
    pub veto_service: AppVetoService,
    /// Result service for match result submission.
    pub result_service: AppResultService,
    /// Progression service for bracket advancement.
    pub progression_service: AppProgressionService,
    /// Evidence service for match evidence management.
    pub evidence_service: AppEvidenceService,
    /// Forfeit service for handling forfeits (no-show, withdrawal, disqualification).
    pub forfeit_service: AppForfeitService,
    /// Dispute service for handling match result disputes.
    pub dispute_service: AppDisputeService,
    /// Demo catalog service for browsing and categorizing demos.
    pub demo_service: AppDemoService,
    /// Demo stat-fact (EAV) repository — extraction at ingest + leaderboards.
    pub demo_stats_repo: Arc<PgDemoPlayerStatsRepository>,
    /// Award service (templates, instances, standings, finalization).
    pub award_service: AppAwardService,
    /// Result review service for validation discrepancy handling.
    pub result_review_service: AppResultReviewService,
    /// Veto lobby chat service for real-time chat messages.
    pub veto_lobby_chat_service: AppVetoLobbyChatService,
    /// Veto authorization service for veto permission checks.
    pub veto_authorization_service: AppVetoAuthorizationService,
    /// Veto lobby manager for WebSocket connections.
    pub veto_lobby_manager: Arc<VetoLobbyManager>,
    /// Standings service for round robin/swiss standings.
    pub standings_service: AppStandingsService,
    /// Tournament match repository for direct match access.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
    /// Tournament map pool repository for veto auto-creation.
    pub tournament_map_pool_repo: Arc<PgTournamentMapPoolRepository>,
    /// Permission service for high-level authorization checks (`is_admin`, etc).
    pub permission_service: AppPermissionService,
    /// Permission repository for low-level/scoped permission checks.
    pub permission_repo: PermissionRepository,
    /// Role repository for RBAC management.
    pub role_repo: RoleRepository,
    /// Game repository for game metadata.
    pub game_repo: GameRepository,
    /// Stats repository for admin dashboard.
    pub stats_repo: StatsRepository,
    /// Action item repository for captain pending actions.
    pub action_item_repo: ActionItemRepository,
    /// Storage backend for file uploads.
    pub storage: Arc<dyn StorageBackend>,
    /// Plugin manager for game-specific logic.
    pub plugin_manager: Arc<PluginManager>,
    /// Match completion saga for orchestrating post-confirmation workflow.
    pub match_completion_saga: AppMatchCompletionSaga,
    /// CS2 demo service base URL (for CS2-specific handlers).
    pub cs2_demo_base_url: Option<String>,
    /// API key repository for service-to-service authentication.
    pub api_key_repo: Arc<PgApiKeyRepository>,
    /// Steam tracking service.
    pub steam_tracking_service: AppSteamTrackingService,
    /// System settings service (runtime-togglable platform settings).
    pub system_settings_service: AppSystemSettingsService,
    /// Discovered match service.
    pub discovered_match_service: AppDiscoveredMatchService,
    /// Player rating history repository for rating submissions.
    pub rating_history_repo: Arc<PgPlayerRatingHistoryRepository>,
    /// Player MM stats repository (public matchmaking aggregates).
    pub mm_stats_repo: Arc<PgPlayerMmStatsRepository>,
    /// Player match history repository (individual public match results).
    pub match_history_repo: Arc<PgPlayerMatchHistoryRepository>,
    /// Base path for local file uploads (used for static file serving).
    pub uploads_path: String,
    /// Refresh token repository.
    pub refresh_token_repo: Arc<PgRefreshTokenRepository>,
    /// Token expiry configuration.
    pub token_config: TokenConfig,
    /// Steam OpenID verifier (outbound check_authentication seam).
    pub steam_verifier: Arc<dyn SteamOpenIdVerifier>,
    /// Steam sign-in configuration (public/frontend URLs, optional API key).
    pub steam_auth_config: SteamAuthConfig,
}

/// Token expiry configuration.
#[derive(Debug, Clone)]
pub struct TokenConfig {
    /// Access token expiry in minutes.
    pub access_token_expiry_minutes: i64,
    /// Refresh token expiry in minutes.
    pub refresh_token_expiry_minutes: i64,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            access_token_expiry_minutes: 15,
            refresh_token_expiry_minutes: 10080, // 7 days
        }
    }
}

/// Storage configuration.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Base path for local storage.
    pub base_path: String,
    /// Base URL for public access.
    pub base_url: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_path: "./uploads".to_string(),
            base_url: "http://localhost:3000/uploads".to_string(),
        }
    }
}

impl AppState {
    /// Create new application state.
    pub async fn new(db_pool: DbPool, jwt_secret: impl Into<Arc<str>>) -> Self {
        Self::with_storage(db_pool, jwt_secret, StorageConfig::default()).await
    }

    /// Create new application state with custom storage configuration.
    pub async fn with_storage(
        db_pool: DbPool,
        jwt_secret: impl Into<Arc<str>>,
        storage_config: StorageConfig,
    ) -> Self {
        // Create repository adapters
        let user_repo = Arc::new(PgUserRepository::new(db_pool.clone()));
        let player_repo = Arc::new(PgPlayerRepository::new(db_pool.clone()));
        let league_repo = Arc::new(PgLeagueRepository::new(db_pool.clone()));
        let league_member_repo = Arc::new(PgLeagueMemberRepository::new(db_pool.clone()));
        let league_invitation_repo = Arc::new(PgLeagueInvitationRepository::new(db_pool.clone()));

        // Create API key repository
        let api_key_repo = Arc::new(PgApiKeyRepository::new(db_pool.clone()));

        // Create refresh token repository
        let refresh_token_repo = Arc::new(PgRefreshTokenRepository::new(db_pool.clone()));

        // Create steam tracking repository
        let steam_tracking_repo = Arc::new(PgSteamTrackingRepository::new(db_pool.clone()));

        // Create discovered match repository
        let discovered_match_repo = Arc::new(PgDiscoveredMatchRepository::new(db_pool.clone()));

        // Create RBAC repositories and services
        let pg_permission_repo = Arc::new(PgPermissionRepository::new(db_pool.clone()));
        let permission_service = PermissionService::new(Arc::clone(&pg_permission_repo));
        let permission_repo = PermissionRepository::new(db_pool.clone());
        let role_repo = RoleRepository::new(db_pool.clone());

        // Create game repository
        let game_repo = GameRepository::new(db_pool.clone());

        // Create stats repository
        let stats_repo = StatsRepository::new(db_pool.clone());

        // Create action item repository
        let action_item_repo = ActionItemRepository::new(db_pool.clone());

        // Create storage backend
        // Save paths before consuming storage_config
        let uploads_path = storage_config.base_path.clone();
        let evidence_base_url = storage_config.base_url.clone();
        let storage: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(
            storage_config.base_path,
            storage_config.base_url,
        ));

        // Create plugin manager with built-in plugins.
        //
        // If `CS2_DEMO_SERVICE_URL` is set we validate it up-front: must be
        // https and point at a non-private, non-loopback host. An invalid
        // value hard-fails startup rather than silently letting the demo
        // plugin issue SSRF-adjacent requests.
        let cs2_demo_base_url = std::env::var("CS2_DEMO_SERVICE_URL").ok().map(|raw| {
            portal_plugins::validate_demo_service_url(&raw)
                .expect("CS2_DEMO_SERVICE_URL failed validation")
        });
        let plugin_manager = Arc::new(portal_plugins::create_plugin_manager_with_config(
            cs2_demo_base_url.clone(),
        ));

        // Create league team repositories
        let league_season_repo = Arc::new(PgLeagueSeasonRepository::new(db_pool.clone()));
        let league_team_repo = Arc::new(PgLeagueTeamRepository::new(db_pool.clone()));
        let league_team_season_repo = Arc::new(PgLeagueTeamSeasonRepository::new(db_pool.clone()));
        let league_team_member_repo = Arc::new(PgLeagueTeamMemberRepository::new(db_pool.clone()));
        let league_team_invitation_repo =
            Arc::new(PgLeagueTeamInvitationRepository::new(db_pool.clone()));
        let league_season_participant_repo =
            Arc::new(PgLeagueSeasonParticipantRepository::new(db_pool.clone()));

        // Create services
        let user_service = UserService::new(Arc::clone(&user_repo), Arc::clone(&player_repo));
        let player_service = PlayerService::new(
            Arc::clone(&player_repo),
            Arc::clone(&league_team_member_repo),
        );
        let steam_tracking_service =
            SteamTrackingService::new(Arc::clone(&steam_tracking_repo), Arc::clone(&player_repo));
        let system_settings_service =
            SystemSettingsService::new(Arc::new(PgSystemSettingsRepository::new(db_pool.clone())));
        let discovered_match_service =
            DiscoveredMatchService::new(Arc::clone(&discovered_match_repo));
        let player_game_profile_repo =
            Arc::new(PgPlayerGameProfileRepository::new(db_pool.clone()));
        let player_game_profile_service =
            PlayerGameProfileService::new(Arc::clone(&player_game_profile_repo));
        let rating_history_repo = Arc::new(PgPlayerRatingHistoryRepository::new(db_pool.clone()));
        let eligibility_service = portal_domain::services::EligibilityService::new(
            PlayerGameProfileService::new(Arc::clone(&player_game_profile_repo)),
            Arc::clone(&rating_history_repo),
        );
        let mm_stats_repo = Arc::new(PgPlayerMmStatsRepository::new(db_pool.clone()));
        let match_history_repo = Arc::new(PgPlayerMatchHistoryRepository::new(db_pool.clone()));
        let league_service = LeagueService::new(
            Arc::clone(&league_repo),
            Arc::clone(&league_member_repo),
            Arc::clone(&league_invitation_repo),
        );
        let league_season_service =
            LeagueSeasonService::new(Arc::clone(&league_season_repo), Arc::clone(&league_repo));
        let league_team_service = LeagueTeamService::new(
            Arc::clone(&league_team_repo),
            Arc::clone(&league_team_season_repo),
            Arc::clone(&league_team_member_repo),
            Arc::clone(&league_season_repo),
        );
        let league_team_invitation_service = LeagueTeamInvitationService::new(
            Arc::clone(&league_team_invitation_repo),
            Arc::clone(&league_team_repo),
            Arc::clone(&league_team_season_repo),
            Arc::clone(&league_team_member_repo),
            Arc::clone(&league_season_repo),
        );
        let league_season_participant_service = LeagueSeasonParticipantService::new(
            Arc::clone(&league_season_participant_repo),
            Arc::clone(&league_season_repo),
        );

        // Create ban service
        let ban_repo = Arc::new(PgBanRepository::new(db_pool.clone()));
        let ban_service = BanService::new(
            Arc::clone(&ban_repo),
            Arc::clone(&user_repo),
            Arc::clone(&refresh_token_repo),
        );

        // Create tournament repositories
        let tournament_repo = Arc::new(PgTournamentRepository::new(db_pool.clone()));
        let tournament_stage_repo = Arc::new(PgTournamentStageRepository::new(db_pool.clone()));
        let tournament_bracket_repo = Arc::new(PgTournamentBracketRepository::new(db_pool.clone()));
        let tournament_registration_repo =
            Arc::new(PgTournamentRegistrationRepository::new(db_pool.clone()));
        let tournament_match_repo = Arc::new(PgTournamentMatchRepository::new(db_pool.clone()));

        // Create tournament standings repository (used by tournament service + standings service)
        let tournament_standings_repo =
            Arc::new(PgTournamentStandingsRepository::new(db_pool.clone()));

        // Create tournament service
        let tournament_service = TournamentService::new(
            Arc::clone(&tournament_repo),
            Arc::clone(&tournament_stage_repo),
            Arc::clone(&tournament_bracket_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_standings_repo),
        );

        // Create Phase 2 tournament services
        let registration_service = RegistrationService::new(
            Arc::clone(&tournament_repo),
            Arc::clone(&tournament_registration_repo),
        );
        let checkin_service = CheckInService::new(
            Arc::clone(&tournament_repo),
            Arc::clone(&tournament_registration_repo),
        );
        let seeding_service = SeedingService::new(
            Arc::clone(&tournament_repo),
            Arc::clone(&tournament_registration_repo),
        );

        // Create Phase 3 tournament services
        let match_status_log_repo = Arc::new(PgMatchStatusLogRepository::new(db_pool.clone()));
        let match_lifecycle_service = MatchLifecycleService::new(
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&match_status_log_repo),
        );

        let schedule_proposal_repo = Arc::new(PgScheduleProposalRepository::new(db_pool.clone()));
        let scheduling_service = SchedulingService::new(
            Arc::clone(&schedule_proposal_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
        );

        // Create availability repositories and service
        let availability_window_repo =
            Arc::new(PgAvailabilityWindowRepository::new(db_pool.clone()));
        let availability_override_repo =
            Arc::new(PgAvailabilityOverrideRepository::new(db_pool.clone()));
        let suggested_time_repo = Arc::new(PgSuggestedTimeRepository::new(db_pool.clone()));
        let availability_service = AvailabilityService::new(
            Arc::clone(&availability_window_repo),
            Arc::clone(&availability_override_repo),
            Arc::clone(&suggested_time_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
        );

        // Create veto and result repositories and services
        let veto_session_repo = Arc::new(PgVetoSessionRepository::new(db_pool.clone()));
        let veto_action_repo = Arc::new(PgVetoActionRepository::new(db_pool.clone()));
        let tournament_map_pool_repo =
            Arc::new(PgTournamentMapPoolRepository::new(db_pool.clone()));

        let format_provider = Arc::new(PluginVetoFormatProvider::new(Arc::clone(&plugin_manager)));
        let side_provider = Arc::new(PluginSideSelectionProvider::new(Arc::clone(
            &plugin_manager,
        )));

        let veto_service = VetoService::new(
            Arc::clone(&veto_session_repo),
            Arc::clone(&veto_action_repo),
            Arc::clone(&tournament_match_repo),
        )
        .with_format_provider(format_provider)
        .with_side_provider(side_provider);

        let result_claim_repo = Arc::new(PgResultClaimRepository::new(db_pool.clone()));

        // Create demo repositories early (also used by demo_service below)
        let demo_repo = Arc::new(PgDemoRepository::new(db_pool.clone()));
        let demo_match_link_repo = Arc::new(PgDemoMatchLinkRepository::new(db_pool.clone()));
        let demo_player_repo = Arc::new(PgDemoPlayerRepository::new(db_pool.clone()));

        let result_service = ResultService::new(
            Arc::clone(&result_claim_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&demo_match_link_repo),
        );

        // Create progression service for bracket advancement
        let progression_service = ProgressionService::new(
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_bracket_repo),
            Arc::clone(&tournament_stage_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&tournament_standings_repo),
        );

        // Create standings service for RR/Swiss standings
        let standings_service = StandingsService::new(
            Arc::clone(&tournament_standings_repo),
            Arc::clone(&tournament_match_repo),
        );

        // Create evidence service — storage backend chosen by EVIDENCE_STORAGE env var
        let evidence_repo = Arc::new(PgEvidenceRepository::new(db_pool.clone()));
        let evidence_storage_mode =
            std::env::var("EVIDENCE_STORAGE").unwrap_or_else(|_| "local".to_string());

        let (evidence_storage, evidence_bucket) = if evidence_storage_mode == "s3" {
            let bucket = std::env::var("S3_EVIDENCE_BUCKET")
                .expect("S3_EVIDENCE_BUCKET must be set when EVIDENCE_STORAGE=s3");
            let region = std::env::var("S3_EVIDENCE_REGION")
                .expect("S3_EVIDENCE_REGION must be set when EVIDENCE_STORAGE=s3");
            let public_url = std::env::var("S3_EVIDENCE_PUBLIC_URL")
                .expect("S3_EVIDENCE_PUBLIC_URL must be set when EVIDENCE_STORAGE=s3");
            let endpoint = std::env::var("S3_EVIDENCE_ENDPOINT").ok();

            let s3_config = portal_storage::S3Config {
                bucket: bucket.clone(),
                region,
                public_url,
                endpoint,
            };
            let adapter = S3EvidenceStorageAdapter::new(s3_config).await;
            tracing::info!(bucket = %bucket, "Evidence storage: S3");
            (EvidenceStorageBackend::S3(adapter), bucket)
        } else {
            let local = LocalEvidenceStorage::new(&uploads_path, evidence_base_url);
            tracing::info!("Evidence storage: local filesystem");
            (EvidenceStorageBackend::Local(local), "evidence".to_string())
        };

        let evidence_config = EvidenceServiceConfig {
            evidence_bucket,
            ..Default::default()
        };
        let evidence_service = EvidenceService::new(
            Arc::clone(&evidence_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::new(evidence_storage),
            evidence_config,
        );

        // Create forfeit service
        let forfeit_repo = Arc::new(PgForfeitRecordRepository::new(db_pool.clone()));
        let forfeit_service = ForfeitService::new(
            Arc::clone(&forfeit_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
        );

        // Create dispute service
        let dispute_repo = Arc::new(PgDisputeRepository::new(db_pool.clone()));
        let dispute_message_repo = Arc::new(PgDisputeMessageRepository::new(db_pool.clone()));
        let dispute_service = DisputeService::new(
            Arc::clone(&dispute_repo),
            Arc::clone(&dispute_message_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&result_claim_repo),
        );

        // Create demo catalog service (repositories already created above)
        let demo_service = DemoService::new(
            Arc::clone(&demo_repo),
            Arc::clone(&demo_match_link_repo),
            Arc::clone(&demo_player_repo),
            Arc::clone(&tournament_match_repo),
        );

        // Create demo stat-fact repository + award service
        let demo_stats_repo = Arc::new(PgDemoPlayerStatsRepository::new(db_pool.clone()));
        let award_repo = Arc::new(PgAwardRepository::new(db_pool.clone()));
        let award_service = AwardService::new(award_repo, Arc::clone(&demo_stats_repo));

        // Create result review service
        let result_review_repo = Arc::new(PgResultReviewRepository::new(db_pool.clone()));
        let result_review_service = ResultReviewService::new(
            Arc::clone(&result_review_repo),
            Arc::clone(&tournament_match_repo),
        );

        // Create veto lobby chat service
        let veto_lobby_message_repo = Arc::new(PgVetoLobbyMessageRepository::new(db_pool.clone()));
        let veto_lobby_chat_service = VetoLobbyChatService::new(
            Arc::clone(&veto_lobby_message_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
        );

        // Create veto authorization service
        let veto_delegate_repo = Arc::new(PgVetoDelegateRepository::new(db_pool.clone()));
        let veto_authorization_service = VetoAuthorizationService::new(
            Arc::clone(&veto_delegate_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&league_team_season_repo),
            Arc::clone(&league_team_repo),
            Arc::clone(&league_team_member_repo),
            Arc::clone(&pg_permission_repo),
        );

        // Create veto lobby manager for WebSocket connections
        let veto_lobby_manager = Arc::new(VetoLobbyManager::new());

        // Create match completion saga with adapters
        let saga_execution_repo = Arc::new(PgSagaExecutionRepository::new(db_pool.clone()));
        let progression_log_repo = Arc::new(PgProgressionLogRepository::new(db_pool.clone()));
        let demo_validator_adapter = Arc::new(DemoValidatorAdapter::new(
            demo_service.clone(),
            result_service.clone(),
        ));
        let review_creator_adapter =
            Arc::new(ReviewCreatorAdapter::new(result_review_service.clone()));
        let stats_updater_adapter = Arc::new(StatsUpdaterAdapter::new(
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&demo_match_link_repo),
            game_repo.clone(),
            player_game_profile_service.clone(),
            Arc::clone(&plugin_manager),
        ));
        let match_completion_saga = MatchCompletionSaga::new(
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_bracket_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&tournament_standings_repo),
            saga_execution_repo,
            progression_log_repo,
            demo_validator_adapter,
            review_creator_adapter,
            stats_updater_adapter,
        );

        Self {
            db_pool,
            jwt_secret: jwt_secret.into(),
            user_service,
            player_service,
            player_game_profile_service,
            eligibility_service,
            league_service,
            league_season_service,
            league_team_service,
            league_team_invitation_service,
            league_season_participant_service,
            ban_service,
            tournament_service,
            registration_service,
            checkin_service,
            seeding_service,
            match_lifecycle_service,
            scheduling_service,
            availability_service,
            veto_service,
            result_service,
            progression_service,
            evidence_service,
            forfeit_service,
            dispute_service,
            demo_service,
            demo_stats_repo,
            award_service,
            result_review_service,
            veto_lobby_chat_service,
            veto_authorization_service,
            veto_lobby_manager,
            standings_service,
            tournament_match_repo,
            tournament_map_pool_repo,
            permission_service,
            permission_repo,
            role_repo,
            game_repo,
            stats_repo,
            action_item_repo,
            storage,
            plugin_manager,
            match_completion_saga,
            cs2_demo_base_url,
            api_key_repo,
            steam_tracking_service,
            system_settings_service,
            discovered_match_service,
            rating_history_repo,
            mm_stats_repo,
            match_history_repo,
            uploads_path,
            refresh_token_repo,
            token_config: TokenConfig::default(),
            steam_verifier: Arc::new(HttpSteamOpenIdVerifier::new()),
            steam_auth_config: SteamAuthConfig::from_env(),
        }
    }

    /// Set custom token configuration (access + refresh token expiry).
    #[must_use]
    pub fn with_token_config(mut self, config: TokenConfig) -> Self {
        self.token_config = config;
        self
    }

    /// Replace the Steam OpenID verifier.
    ///
    /// Integration tests inject a double here so the Steam callback
    /// handler can be exercised without any network access.
    #[must_use]
    pub fn with_steam_verifier(mut self, verifier: Arc<dyn SteamOpenIdVerifier>) -> Self {
        self.steam_verifier = verifier;
        self
    }

    /// Replace the Steam sign-in configuration (public/frontend URLs,
    /// optional Steam Web API key).
    #[must_use]
    pub fn with_steam_auth_config(mut self, config: SteamAuthConfig) -> Self {
        self.steam_auth_config = config;
        self
    }

    /// Replace the evidence storage backend.
    ///
    /// Used by integration tests to inject a MinIO-backed S3 backend
    /// without relying on process-wide environment variables.
    #[must_use]
    pub fn with_evidence_storage(
        mut self,
        storage: EvidenceStorageBackend,
        bucket: String,
    ) -> Self {
        let evidence_repo = Arc::new(PgEvidenceRepository::new(self.db_pool.clone()));
        let match_repo = Arc::clone(&self.tournament_match_repo);
        let reg_repo = Arc::new(PgTournamentRegistrationRepository::new(
            self.db_pool.clone(),
        ));
        let config = EvidenceServiceConfig {
            evidence_bucket: bucket,
            ..Default::default()
        };
        self.evidence_service = EvidenceService::new(
            evidence_repo,
            match_repo,
            reg_repo,
            Arc::new(storage),
            config,
        );
        self
    }

    /// Override the CS2 demo stats service base URL WITHOUT the https /
    /// non-private-host validation that `CS2_DEMO_SERVICE_URL` enforces.
    ///
    /// Test-only: lets integration tests point the evidence validate-demo /
    /// demo-stats endpoints at a local mock stats server. Never expose this
    /// outside the test-utils feature — the validation exists to keep the
    /// demo client from issuing SSRF-adjacent requests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn with_cs2_demo_url_unchecked(mut self, url: String) -> Self {
        self.cs2_demo_base_url = Some(url);
        self
    }
}
