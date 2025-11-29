//! Application state for dependency injection.

use portal_db::{
    DbPool, GameRepository, PermissionRepository, PgLeagueInvitationRepository,
    PgLeagueMemberRepository, PgLeagueRepository, PgPermissionRepository, PgPlayerRepository,
    PgTeamInvitationRepository, PgTeamMemberRepository, PgTeamRepository, PgUserRepository,
    RoleRepository, StatsRepository,
};
use portal_domain::services::{
    LeagueService, PermissionService, PlayerService, TeamInvitationService, TeamService,
    UserService,
};
use portal_plugins::PluginManager;
use portal_storage::{LocalStorage, StorageBackend};
use std::sync::Arc;

/// Type aliases for services with concrete repository implementations.
pub type AppUserService = UserService<PgUserRepository, PgPlayerRepository>;
pub type AppPlayerService = PlayerService<PgPlayerRepository, PgTeamRepository, PgTeamMemberRepository>;
pub type AppTeamService = TeamService<PgTeamRepository, PgTeamMemberRepository, PgPlayerRepository>;
pub type AppTeamInvitationService = TeamInvitationService<
    PgTeamInvitationRepository,
    PgTeamRepository,
    PgTeamMemberRepository,
    PgPlayerRepository,
>;
pub type AppPermissionService = PermissionService<PgPermissionRepository>;
pub type AppLeagueService =
    LeagueService<PgLeagueRepository, PgLeagueMemberRepository, PgLeagueInvitationRepository>;

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
    /// Team service.
    pub team_service: AppTeamService,
    /// Team invitation service.
    pub invitation_service: AppTeamInvitationService,
    /// League service.
    pub league_service: AppLeagueService,
    /// Permission service for high-level authorization checks (is_admin, etc).
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
        let team_repo = Arc::new(PgTeamRepository::new(db_pool.clone()));
        let team_member_repo = Arc::new(PgTeamMemberRepository::new(db_pool.clone()));
        let invitation_repo = Arc::new(PgTeamInvitationRepository::new(db_pool.clone()));
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

        // Create services
        let user_service = UserService::new(Arc::clone(&user_repo), Arc::clone(&player_repo));
        let player_service = PlayerService::new(
            Arc::clone(&player_repo),
            Arc::clone(&team_repo),
            Arc::clone(&team_member_repo),
        );
        let team_service = TeamService::new(
            Arc::clone(&team_repo),
            Arc::clone(&team_member_repo),
            Arc::clone(&player_repo),
        );
        let invitation_service = TeamInvitationService::new(
            Arc::clone(&invitation_repo),
            Arc::clone(&team_repo),
            Arc::clone(&team_member_repo),
            Arc::clone(&player_repo),
        );
        let league_service = LeagueService::new(
            Arc::clone(&league_repo),
            Arc::clone(&league_member_repo),
            Arc::clone(&league_invitation_repo),
        );

        Self {
            db_pool,
            jwt_secret: jwt_secret.into(),
            user_service,
            player_service,
            team_service,
            invitation_service,
            league_service,
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
