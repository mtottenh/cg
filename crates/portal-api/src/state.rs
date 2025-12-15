//! Application state for dependency injection.

use portal_db::{
    DbPool, GameRepository, LocalEvidenceStorage, PermissionRepository,
    PgAvailabilityOverrideRepository, PgAvailabilityWindowRepository, PgBanRepository,
    PgDemoMatchLinkRepository, PgDemoPlayerRepository, PgDemoRepository,
    PgDisputeMessageRepository, PgDisputeRepository, PgEvidenceRepository,
    PgForfeitRecordRepository, PgLeagueInvitationRepository, PgLeagueMemberRepository,
    PgLeagueRepository, PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository,
    PgLeagueTeamInvitationRepository, PgLeagueTeamMemberRepository, PgLeagueTeamRepository,
    PgLeagueTeamSeasonRepository, PgMatchStatusLogRepository, PgPermissionRepository,
    PgPlayerRepository, PgResultClaimRepository, PgResultReviewRepository,
    PgScheduleProposalRepository, PgSuggestedTimeRepository, PgTournamentBracketRepository,
    PgTournamentMatchRepository, PgTournamentRegistrationRepository, PgTournamentRepository,
    PgTournamentStageRepository, PgTournamentStandingsRepository, PgUserRepository,
    PgVetoActionRepository, PgVetoDelegateRepository, PgVetoLobbyMessageRepository,
    PgVetoSessionRepository, RoleRepository, StatsRepository,
};
use portal_domain::services::{
    tournament::{
        AvailabilityService, CheckInService, DisputeService, EvidenceService,
        EvidenceServiceConfig, ForfeitService, MatchLifecycleService, ProgressionService,
        RegistrationService, ResultReviewService, ResultService, SchedulingService,
        SeedingService, VetoAuthorizationService, VetoLobbyChatService, VetoService,
    },
    BanService, DemoService, LeagueSeasonParticipantService, LeagueSeasonService, LeagueService,
    LeagueTeamInvitationService, LeagueTeamService, PermissionService, PlayerService,
    TournamentService, UserService,
};
use crate::websocket::VetoLobbyManager;
use portal_plugins::PluginManager;
use portal_storage::{LocalStorage, StorageBackend};
use std::sync::Arc;

/// Type aliases for services with concrete repository implementations.
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
pub type AppBanService = BanService<PgBanRepository>;
pub type AppTournamentService = TournamentService<
    PgTournamentRepository,
    PgTournamentStageRepository,
    PgTournamentBracketRepository,
    PgTournamentRegistrationRepository,
    PgTournamentMatchRepository,
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
pub type AppVetoService = VetoService<
    PgVetoSessionRepository,
    PgVetoActionRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
>;
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
    LocalEvidenceStorage,
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
pub type AppDemoService =
    DemoService<PgDemoRepository, PgDemoMatchLinkRepository, PgDemoPlayerRepository>;
pub type AppResultReviewService =
    ResultReviewService<PgResultReviewRepository, PgTournamentMatchRepository>;
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
    /// Result review service for validation discrepancy handling.
    pub result_review_service: AppResultReviewService,
    /// Veto lobby chat service for real-time chat messages.
    pub veto_lobby_chat_service: AppVetoLobbyChatService,
    /// Veto authorization service for veto permission checks.
    pub veto_authorization_service: AppVetoAuthorizationService,
    /// Veto lobby manager for WebSocket connections.
    pub veto_lobby_manager: Arc<VetoLobbyManager>,
    /// Tournament match repository for direct match access.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
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
    /// Storage backend for file uploads.
    pub storage: Arc<dyn StorageBackend>,
    /// Plugin manager for game-specific logic.
    pub plugin_manager: Arc<PluginManager>,
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
    #[must_use]
    pub fn new(db_pool: DbPool, jwt_secret: impl Into<Arc<str>>) -> Self {
        Self::with_storage(db_pool, jwt_secret, StorageConfig::default())
    }

    /// Create new application state with custom storage configuration.
    #[must_use]
    pub fn with_storage(
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

        // Create RBAC repositories and services
        let pg_permission_repo = Arc::new(PgPermissionRepository::new(db_pool.clone()));
        let permission_service = PermissionService::new(Arc::clone(&pg_permission_repo));
        let permission_repo = PermissionRepository::new(db_pool.clone());
        let role_repo = RoleRepository::new(db_pool.clone());

        // Create game repository
        let game_repo = GameRepository::new(db_pool.clone());

        // Create stats repository
        let stats_repo = StatsRepository::new(db_pool.clone());

        // Create storage backend
        // Save evidence base path before consuming storage_config
        let evidence_base_path = format!("{}/evidence", storage_config.base_path);
        let storage: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(
            storage_config.base_path,
            storage_config.base_url,
        ));

        // Create plugin manager with built-in plugins
        let plugin_manager = Arc::new(portal_plugins::create_default_plugin_manager());

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
        let player_service =
            PlayerService::new(Arc::clone(&player_repo), Arc::clone(&league_team_member_repo));
        let league_service = LeagueService::new(
            Arc::clone(&league_repo),
            Arc::clone(&league_member_repo),
            Arc::clone(&league_invitation_repo),
        );
        let league_season_service = LeagueSeasonService::new(
            Arc::clone(&league_season_repo),
            Arc::clone(&league_repo),
        );
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
        let ban_service = BanService::new(Arc::clone(&ban_repo));

        // Create tournament repositories
        let tournament_repo = Arc::new(PgTournamentRepository::new(db_pool.clone()));
        let tournament_stage_repo = Arc::new(PgTournamentStageRepository::new(db_pool.clone()));
        let tournament_bracket_repo = Arc::new(PgTournamentBracketRepository::new(db_pool.clone()));
        let tournament_registration_repo =
            Arc::new(PgTournamentRegistrationRepository::new(db_pool.clone()));
        let tournament_match_repo = Arc::new(PgTournamentMatchRepository::new(db_pool.clone()));

        // Create tournament service
        let tournament_service = TournamentService::new(
            Arc::clone(&tournament_repo),
            Arc::clone(&tournament_stage_repo),
            Arc::clone(&tournament_bracket_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&tournament_match_repo),
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
        let availability_window_repo = Arc::new(PgAvailabilityWindowRepository::new(db_pool.clone()));
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
        let veto_service = VetoService::new(
            Arc::clone(&veto_session_repo),
            Arc::clone(&veto_action_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
        );

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
        let tournament_standings_repo = Arc::new(PgTournamentStandingsRepository::new(db_pool.clone()));
        let progression_service = ProgressionService::new(
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_bracket_repo),
            Arc::clone(&tournament_stage_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&tournament_standings_repo),
        );

        // Create evidence service with local storage
        let evidence_repo = Arc::new(PgEvidenceRepository::new(db_pool.clone()));
        let evidence_storage = Arc::new(LocalEvidenceStorage::new(evidence_base_path));
        let evidence_config = EvidenceServiceConfig {
            evidence_bucket: "evidence".to_string(),
            ..Default::default()
        };
        let evidence_service = EvidenceService::new(
            Arc::clone(&evidence_repo),
            Arc::clone(&tournament_match_repo),
            Arc::clone(&tournament_registration_repo),
            Arc::clone(&evidence_storage),
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
        );

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

        Self {
            db_pool,
            jwt_secret: jwt_secret.into(),
            user_service,
            player_service,
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
            result_review_service,
            veto_lobby_chat_service,
            veto_authorization_service,
            veto_lobby_manager,
            tournament_match_repo,
            permission_service,
            permission_repo,
            role_repo,
            game_repo,
            stats_repo,
            storage,
            plugin_manager,
        }
    }
}
