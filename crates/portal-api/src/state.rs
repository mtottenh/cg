//! Application state for dependency injection.

use portal_db::{
    DbPool, GameRepository, PermissionRepository, PgBanRepository, PgLeagueInvitationRepository,
    PgLeagueMemberRepository, PgLeagueRepository, PgLeagueSeasonParticipantRepository,
    PgLeagueSeasonRepository, PgLeagueTeamInvitationRepository, PgLeagueTeamMemberRepository,
    PgLeagueTeamRepository, PgLeagueTeamSeasonRepository, PgPermissionRepository,
    PgPlayerRepository, PgTournamentBracketRepository, PgTournamentMatchRepository,
    PgTournamentRegistrationRepository, PgTournamentRepository, PgTournamentStageRepository,
    PgUserRepository, RoleRepository, StatsRepository,
};
use portal_domain::services::{
    BanService, LeagueSeasonParticipantService, LeagueSeasonService, LeagueService,
    LeagueTeamInvitationService, LeagueTeamService, PermissionService, PlayerService,
    TournamentService, UserService,
};
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
            tournament_repo,
            tournament_stage_repo,
            tournament_bracket_repo,
            tournament_registration_repo,
            tournament_match_repo,
        );

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
