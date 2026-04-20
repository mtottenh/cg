//! Domain-scoped sub-states extracted from [`AppState`].
//!
//! # Why this exists
//!
//! `AppState` aggregates every service, repository, and config the portal
//! uses — 55+ fields. Because Axum re-extracts `State<T>` on every
//! handler call via `FromRef`, a handler that takes `State<AppState>`
//! shares the full state surface even when it only needs a narrow slice
//! (e.g. login only touches the user service, JWT secret, and refresh
//! token repo).
//!
//! Sub-states fix three things:
//!
//! 1. **Clarity of dependencies.** A handler signature that takes
//!    `State<AuthState>` is self-documenting about what it touches.
//! 2. **Compile-time decoupling.** A service added to the tournament
//!    stack no longer forces the auth handlers to recompile.
//! 3. **Test doubles.** A unit test can build just the sub-state it
//!    needs instead of constructing the full `AppState`.
//!
//! Each sub-state is `Clone`-cheap (every field is `Arc`-wrapped or a
//! small value type) and has a `FromRef<AppState>` impl, so routes still
//! mount with `.with_state(app_state)` unchanged.
//!
//! ## Sub-state catalog
//!
//! Roughly one struct per handler module (or handler cluster when the
//! modules are tiny, e.g. a single `BanState` covers `bans.rs`). When
//! a module needs cross-domain helpers (`tournaments/` has both
//! `check_eligibility_for_players` and `auto_create_veto_session` in
//! its `mod.rs`), the sub-state folds in every field those helpers
//! touch so the module can continue calling them internally without
//! reaching for `AppState` again.

use std::sync::Arc;

use axum::extract::FromRef;
use portal_db::{
    ActionItemRepository, GameRepository, PermissionRepository, PgApiKeyRepository,
    PgPlayerMatchHistoryRepository, PgPlayerMmStatsRepository, PgPlayerRatingHistoryRepository,
    PgRefreshTokenRepository, PgTournamentMapPoolRepository, PgTournamentMatchRepository,
    RoleRepository, StatsRepository,
};
use portal_storage::StorageBackend;

use super::{
    AppAvailabilityService, AppBanService, AppCheckInService, AppDemoService,
    AppDiscoveredMatchService, AppDisputeService, AppEligibilityService, AppEvidenceService,
    AppForfeitService, AppLeagueSeasonParticipantService, AppLeagueSeasonService, AppLeagueService,
    AppLeagueTeamInvitationService, AppLeagueTeamService, AppMatchLifecycleService,
    AppPermissionService, AppPlayerGameProfileService, AppPlayerService, AppProgressionService,
    AppRegistrationService, AppResultReviewService, AppResultService, AppSchedulingService,
    AppSeedingService, AppStandingsService, AppState, AppSteamTrackingService,
    AppTournamentService, AppUserService, AppVetoAuthorizationService, AppVetoLobbyChatService,
    AppVetoService, TokenConfig,
};
use crate::websocket::VetoLobbyManager;
use portal_plugins::PluginManager;

// ============================================================================
// AUTH / USERS / ADMIN
// ============================================================================

/// State slice used by authentication / session handlers.
///
/// Wraps everything needed to issue, validate, refresh, and rotate JWTs:
/// the signing secret, the user + player services (for credential
/// verification and profile lookup during token refresh), the
/// refresh-token repo (for rotation + replay detection), the
/// role repo (so `register` can grant the default `user` role), and
/// the token expiry configuration.
#[derive(Clone)]
pub struct AuthState {
    /// Shared JWT signing secret.
    pub jwt_secret: Arc<str>,
    /// User service for register/authenticate/lookup.
    pub user_service: AppUserService,
    /// Player service — refresh() needs it to fetch the player record
    /// tied to the user when issuing a new access token.
    pub player_service: AppPlayerService,
    /// Refresh-token repository for rotation and replay detection.
    pub refresh_token_repo: Arc<PgRefreshTokenRepository>,
    /// Role repository — `register` grants the default `user` role.
    pub role_repo: RoleRepository,
    /// Access + refresh token expiry configuration.
    pub token_config: TokenConfig,
}

impl FromRef<AppState> for AuthState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            jwt_secret: Arc::clone(&s.jwt_secret),
            user_service: s.user_service.clone(),
            player_service: s.player_service.clone(),
            refresh_token_repo: Arc::clone(&s.refresh_token_repo),
            role_repo: s.role_repo.clone(),
            token_config: s.token_config.clone(),
        }
    }
}

/// State slice used by the `/users/me`-style handlers.
///
/// Thinner than [`AuthState`] — these are read-mostly account endpoints
/// that don't issue tokens. They consult `role_repo` to list the
/// caller's role assignments, `tournament_service` to list the
/// caller's upcoming matches, and `action_item_repo` for the captain
/// pending-actions badge.
#[derive(Clone)]
pub struct UsersState {
    /// User service.
    pub user_service: AppUserService,
    /// Role repository (for listing the caller's role assignments).
    pub role_repo: RoleRepository,
    /// Tournament service (GET /users/me/matches).
    pub tournament_service: AppTournamentService,
    /// Action item repository (GET /users/me/action-items).
    pub action_item_repo: ActionItemRepository,
}

impl FromRef<AppState> for UsersState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            user_service: s.user_service.clone(),
            role_repo: s.role_repo.clone(),
            tournament_service: s.tournament_service.clone(),
            action_item_repo: s.action_item_repo.clone(),
        }
    }
}

/// State slice used by platform-admin handlers (dashboard stats).
#[derive(Clone)]
pub struct AdminState {
    /// Permission service (for `is_admin` checks).
    pub permission_service: AppPermissionService,
    /// Stats repository (platform-wide counters).
    pub stats_repo: StatsRepository,
}

impl FromRef<AppState> for AdminState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            permission_service: s.permission_service.clone(),
            stats_repo: s.stats_repo.clone(),
        }
    }
}

/// State slice used by RBAC management (roles + permissions CRUD).
#[derive(Clone)]
pub struct RolesState {
    /// Permission service.
    pub permission_service: AppPermissionService,
    /// Role repository.
    pub role_repo: RoleRepository,
    /// Permission repository (list permissions in scope).
    pub permission_repo: PermissionRepository,
}

impl FromRef<AppState> for RolesState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            permission_service: s.permission_service.clone(),
            role_repo: s.role_repo.clone(),
            permission_repo: s.permission_repo.clone(),
        }
    }
}

/// State slice used by ban-management handlers.
#[derive(Clone)]
pub struct BanState {
    /// Ban service.
    pub ban_service: AppBanService,
    /// Permission service (admin-only lifecycle ops).
    pub permission_service: AppPermissionService,
}

impl FromRef<AppState> for BanState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            ban_service: s.ban_service.clone(),
            permission_service: s.permission_service.clone(),
        }
    }
}

// ============================================================================
// GAMES + PLAYERS
// ============================================================================

/// State slice used by game-metadata handlers.
///
/// Anything that renders map pools, rank tiers, or pluggable game
/// configuration reads from these. The `permission_repo` is here for
/// the `require_games_admin` helper inside `games.rs` — admin writes
/// to the catalog check `admin.games.manage`.
#[derive(Clone)]
pub struct GamesState {
    /// Game repository.
    pub game_repo: GameRepository,
    /// Plugin manager (for looking up game-specific plugin metadata).
    pub plugin_manager: Arc<PluginManager>,
    /// Permission repository (admin.games.manage check).
    pub permission_repo: PermissionRepository,
}

impl FromRef<AppState> for GamesState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            game_repo: s.game_repo.clone(),
            plugin_manager: Arc::clone(&s.plugin_manager),
            permission_repo: s.permission_repo.clone(),
        }
    }
}

/// State slice used by player profile handlers (`handlers/players.rs`
/// and `handlers/player_game_profiles.rs`).
///
/// Holds the superset of fields both files need: the basic player
/// service, the per-game profile service, game metadata (for rendering
/// game-specific stats), the rating-history + MM-stats repos (used by
/// the game-profile endpoints), and a raw pool handle for the single
/// hand-rolled admin query in `submit_player_rating`.
#[derive(Clone)]
pub struct PlayerState {
    /// Player service.
    pub player_service: AppPlayerService,
    /// Per-game profile service.
    pub player_game_profile_service: AppPlayerGameProfileService,
    /// Game repository.
    pub game_repo: GameRepository,
    /// Plugin manager (for per-game stat rendering).
    pub plugin_manager: Arc<PluginManager>,
    /// Rating history repository.
    pub rating_history_repo: Arc<PgPlayerRatingHistoryRepository>,
    /// MM stats repository (public matchmaking aggregates).
    pub mm_stats_repo: Arc<PgPlayerMmStatsRepository>,
    /// Match history repository (individual public match results).
    pub match_history_repo: Arc<PgPlayerMatchHistoryRepository>,
    /// Database pool (for a single hand-rolled admin query in
    /// `submit_player_rating`; remove when that query is moved behind
    /// a repo trait).
    pub db_pool: portal_db::DbPool,
}

impl FromRef<AppState> for PlayerState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            player_service: s.player_service.clone(),
            player_game_profile_service: s.player_game_profile_service.clone(),
            game_repo: s.game_repo.clone(),
            plugin_manager: Arc::clone(&s.plugin_manager),
            rating_history_repo: Arc::clone(&s.rating_history_repo),
            mm_stats_repo: Arc::clone(&s.mm_stats_repo),
            match_history_repo: Arc::clone(&s.match_history_repo),
            db_pool: s.db_pool.clone(),
        }
    }
}

/// State slice used by Steam-tracking handlers.
#[derive(Clone)]
pub struct SteamTrackingState {
    /// Steam tracking service.
    pub steam_tracking_service: AppSteamTrackingService,
    /// Game repository (resolve game slug → id).
    pub game_repo: GameRepository,
    /// Player service (link Steam account → portal player).
    pub player_service: AppPlayerService,
}

impl FromRef<AppState> for SteamTrackingState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            steam_tracking_service: s.steam_tracking_service.clone(),
            game_repo: s.game_repo.clone(),
            player_service: s.player_service.clone(),
        }
    }
}

/// State slice used by file-upload handlers (player avatar/banner, team
/// logo/banner). Team uploads need the league-team service to persist the
/// stored URL back to the team row after a successful upload.
#[derive(Clone)]
pub struct UploadsState {
    /// Storage backend.
    pub storage: Arc<dyn StorageBackend>,
    /// Player service (to update profile URLs after successful upload).
    pub player_service: AppPlayerService,
    /// League-team service (to update team URLs after successful upload).
    pub league_team_service: AppLeagueTeamService,
}

impl FromRef<AppState> for UploadsState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            storage: Arc::clone(&s.storage),
            player_service: s.player_service.clone(),
            league_team_service: s.league_team_service.clone(),
        }
    }
}

// ============================================================================
// LEAGUES + LEAGUE TEAMS
// ============================================================================

/// State slice used by `handlers/leagues.rs`.
///
/// `eligibility_service` is folded in because league application flows
/// rehydrate a player's game profile to run eligibility checks before
/// auto-approving them.
#[derive(Clone)]
pub struct LeaguesState {
    /// League service.
    pub league_service: AppLeagueService,
    /// Eligibility service.
    pub eligibility_service: AppEligibilityService,
    /// Role repository (admin actions on league membership).
    pub role_repo: RoleRepository,
}

impl FromRef<AppState> for LeaguesState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            league_service: s.league_service.clone(),
            eligibility_service: s.eligibility_service.clone(),
            role_repo: s.role_repo.clone(),
        }
    }
}

/// State slice used by league-team handlers (team CRUD, invitations,
/// seasons, roster management).
#[derive(Clone)]
pub struct LeagueTeamState {
    /// League team service.
    pub league_team_service: AppLeagueTeamService,
    /// League team invitation service.
    pub league_team_invitation_service: AppLeagueTeamInvitationService,
    /// League season service (used by team creation to resolve league id).
    pub league_season_service: AppLeagueSeasonService,
    /// League season participant service (for individual format).
    pub league_season_participant_service: AppLeagueSeasonParticipantService,
}

impl FromRef<AppState> for LeagueTeamState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            league_team_service: s.league_team_service.clone(),
            league_team_invitation_service: s.league_team_invitation_service.clone(),
            league_season_service: s.league_season_service.clone(),
            league_season_participant_service: s.league_season_participant_service.clone(),
        }
    }
}

// ============================================================================
// TOURNAMENTS
// ============================================================================

/// State slice used by every handler under `handlers/tournaments/`.
///
/// This is the widest sub-state by design — tournaments weave together
/// registration, scheduling, seeding, standings, match lifecycle,
/// progression, map-pool, and veto-bootstrap concerns. Narrower slices
/// inside `tournaments/*` would force the shared helpers in
/// [`super::super::handlers::tournaments`] (`check_eligibility_for_players`,
/// `auto_create_veto_session`) to rebind to different sub-states per
/// call site. One sub-state shared across the cluster keeps those
/// helpers clean.
#[derive(Clone)]
pub struct TournamentState {
    /// Core tournament service (create / CRUD / stages / matches).
    pub tournament_service: AppTournamentService,
    /// Tournament registration service (withdraw / approve / reject / DQ).
    pub registration_service: AppRegistrationService,
    /// Tournament check-in service (check-in status, admin check-in,
    /// no-shows).
    pub checkin_service: AppCheckInService,
    /// Tournament seeding service.
    pub seeding_service: AppSeedingService,
    /// Match lifecycle service (status / check-in / schedule / forfeit
    /// / admin transition).
    pub match_lifecycle_service: AppMatchLifecycleService,
    /// Match scheduling service (proposal workflow).
    pub scheduling_service: AppSchedulingService,
    /// Standings service (RR / Swiss).
    pub standings_service: AppStandingsService,
    /// League service (tournament creation defaults season from league).
    pub league_service: AppLeagueService,
    /// League team service (team registrations pull rosters for
    /// eligibility).
    pub league_team_service: AppLeagueTeamService,
    /// Eligibility service (used by `check_eligibility_for_players`).
    pub eligibility_service: AppEligibilityService,
    /// Veto service (auto-bootstrapped on PickBan transition).
    pub veto_service: AppVetoService,
    /// Tournament match repository (direct access by scheduling +
    /// veto auto-create).
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
    /// Tournament map pool repository (effective pool resolution).
    pub tournament_map_pool_repo: Arc<PgTournamentMapPoolRepository>,
    /// Game repository (map-pool fallback, veto side-selection mode).
    pub game_repo: GameRepository,
    /// Plugin manager (veto side-selection defaults).
    pub plugin_manager: Arc<PluginManager>,
}

impl FromRef<AppState> for TournamentState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            tournament_service: s.tournament_service.clone(),
            registration_service: s.registration_service.clone(),
            checkin_service: s.checkin_service.clone(),
            seeding_service: s.seeding_service.clone(),
            match_lifecycle_service: s.match_lifecycle_service.clone(),
            scheduling_service: s.scheduling_service.clone(),
            standings_service: s.standings_service.clone(),
            league_service: s.league_service.clone(),
            league_team_service: s.league_team_service.clone(),
            eligibility_service: s.eligibility_service.clone(),
            veto_service: s.veto_service.clone(),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
            tournament_map_pool_repo: Arc::clone(&s.tournament_map_pool_repo),
            game_repo: s.game_repo.clone(),
            plugin_manager: Arc::clone(&s.plugin_manager),
        }
    }
}

/// State slice used by bracket-progression handlers.
#[derive(Clone)]
pub struct ProgressionState {
    /// Progression service.
    pub progression_service: AppProgressionService,
}

impl FromRef<AppState> for ProgressionState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            progression_service: s.progression_service.clone(),
        }
    }
}

/// State slice used by availability handlers (player / participant
/// schedule windows + override + suggestion generation).
#[derive(Clone)]
pub struct AvailabilityState {
    /// Availability service.
    pub availability_service: AppAvailabilityService,
}

impl FromRef<AppState> for AvailabilityState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            availability_service: s.availability_service.clone(),
        }
    }
}

/// State slice used by result-claim handlers.
///
/// `tournament_match_repo` + `match_completion_saga` are in here
/// because confirming a claim can trigger the match-completion saga
/// which needs to re-read the match row.
#[derive(Clone)]
pub struct ResultState {
    /// Result claim service.
    pub result_service: AppResultService,
    /// Tournament match repository.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
    /// Match completion saga (kicked off on claim confirmation).
    pub match_completion_saga: super::AppMatchCompletionSaga,
}

impl FromRef<AppState> for ResultState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            result_service: s.result_service.clone(),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
            match_completion_saga: s.match_completion_saga.clone(),
        }
    }
}

/// State slice used by admin result-review handlers.
///
/// `tournament_match_repo` is here because approve/reject paths load
/// the underlying match to report its current state alongside the
/// review decision.
#[derive(Clone)]
pub struct ResultReviewState {
    /// Result review service.
    pub result_review_service: AppResultReviewService,
    /// Tournament match repository.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
    /// Match completion saga (review approval can advance a stalled match).
    pub match_completion_saga: super::AppMatchCompletionSaga,
}

impl FromRef<AppState> for ResultReviewState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            result_review_service: s.result_review_service.clone(),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
            match_completion_saga: s.match_completion_saga.clone(),
        }
    }
}

/// State slice used by forfeit handlers.
#[derive(Clone)]
pub struct ForfeitState {
    /// Forfeit service.
    pub forfeit_service: AppForfeitService,
}

impl FromRef<AppState> for ForfeitState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            forfeit_service: s.forfeit_service.clone(),
        }
    }
}

// ============================================================================
// DISPUTES
// ============================================================================

/// State slice used by dispute read handlers.
///
/// The participant check in `handlers/dispute.rs::get_dispute` needs:
/// - the dispute service (to load the dispute + thread)
/// - the tournament-match repo (to resolve the match's participants)
/// - the registration service (to inspect each registration)
/// - the league-team service (to check team-season membership)
#[derive(Clone)]
pub struct DisputeState {
    /// Dispute service.
    pub dispute_service: AppDisputeService,
    /// Registration service.
    pub registration_service: AppRegistrationService,
    /// League-team service (for team-season membership checks).
    pub league_team_service: AppLeagueTeamService,
    /// Tournament match repository.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
}

impl FromRef<AppState> for DisputeState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            dispute_service: s.dispute_service.clone(),
            registration_service: s.registration_service.clone(),
            league_team_service: s.league_team_service.clone(),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
        }
    }
}

// ============================================================================
// DEMOS + EVIDENCE
// ============================================================================

/// State slice used by demo catalog handlers.
///
/// The `cs2_demo_base_url` field carries the per-process demo service
/// URL that was validated at startup by
/// [`portal_plugins::validate_demo_service_url`]; handlers consult it
/// when building download URLs. Admin-side demo catalog endpoints
/// (`catalog_demo`, `set_demo_visibility`, etc.) check
/// `admin.demos.manage` via `permission_service`, and validate the
/// incoming `game_id` against `game_repo`.
#[derive(Clone)]
pub struct DemoState {
    /// Demo service.
    pub demo_service: AppDemoService,
    /// Validated CS2 demo service base URL (None if unset at startup).
    pub cs2_demo_base_url: Option<String>,
    /// Permission service (admin demo catalog endpoints).
    pub permission_service: AppPermissionService,
    /// Game repository (validate game_id on catalog_demo).
    pub game_repo: GameRepository,
}

impl FromRef<AppState> for DemoState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            demo_service: s.demo_service.clone(),
            cs2_demo_base_url: s.cs2_demo_base_url.clone(),
            permission_service: s.permission_service.clone(),
            game_repo: s.game_repo.clone(),
        }
    }
}

/// State slice used by match-evidence handlers.
///
/// Evidence touches several surfaces: the evidence service proper, the
/// demo service (for demo-type evidence), tournament/registration
/// lookups for authorization, raw uploads_path for local filesystem
/// storage, and the CS2 demo service URL for demo validation. The
/// `tournament_service` + `registration_service` + `game_repo` fields
/// support evidence-validation paths that need to consult the match's
/// tournament to pick the right game plugin + validator.
#[derive(Clone)]
pub struct EvidenceState {
    /// Evidence service.
    pub evidence_service: AppEvidenceService,
    /// Demo service (for linking demo evidence).
    pub demo_service: AppDemoService,
    /// Tournament match repository.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
    /// Tournament service (resolve tournament from match for plugin selection).
    pub tournament_service: AppTournamentService,
    /// Registration service (resolve reporter registration).
    pub registration_service: AppRegistrationService,
    /// League service (org-level authorization on evidence access).
    pub league_service: AppLeagueService,
    /// Player service (resolve reporters + participants).
    pub player_service: AppPlayerService,
    /// Game repository (resolve game for plugin selection).
    pub game_repo: GameRepository,
    /// Base filesystem path for local uploads.
    pub uploads_path: String,
    /// Validated CS2 demo service base URL (for demo validation).
    pub cs2_demo_base_url: Option<String>,
    /// Storage backend (evidence blob reads).
    pub storage: Arc<dyn StorageBackend>,
    /// Plugin manager (per-game evidence validators).
    pub plugin_manager: Arc<PluginManager>,
}

impl FromRef<AppState> for EvidenceState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            evidence_service: s.evidence_service.clone(),
            demo_service: s.demo_service.clone(),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
            tournament_service: s.tournament_service.clone(),
            registration_service: s.registration_service.clone(),
            league_service: s.league_service.clone(),
            player_service: s.player_service.clone(),
            game_repo: s.game_repo.clone(),
            uploads_path: s.uploads_path.clone(),
            cs2_demo_base_url: s.cs2_demo_base_url.clone(),
            storage: Arc::clone(&s.storage),
            plugin_manager: Arc::clone(&s.plugin_manager),
        }
    }
}

// ============================================================================
// VETO
// ============================================================================

/// State slice used by REST veto handlers (`handlers/veto.rs`).
///
/// `veto_authorization_service` gates pick/ban actions (captain +
/// delegates only). `tournament_service` + `tournament_map_pool_repo`
/// + `game_repo` support the auto-session-start path that runs when
/// a match transitions to PickBan — we rebuild the map pool and
/// side-selection mode from tournament config there.
#[derive(Clone)]
pub struct VetoState {
    /// Veto service.
    pub veto_service: AppVetoService,
    /// Veto authorization service (captain / delegate checks).
    pub veto_authorization_service: AppVetoAuthorizationService,
    /// Veto lobby manager (WebSocket fan-out for state changes).
    pub veto_lobby_manager: Arc<VetoLobbyManager>,
    /// Plugin manager (game-specific veto format resolution).
    pub plugin_manager: Arc<PluginManager>,
    /// Tournament service (resolve tournament settings → veto format).
    pub tournament_service: AppTournamentService,
    /// Tournament match repository (reload match after veto mutation).
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
    /// Tournament map pool repository (resolve effective map pool).
    pub tournament_map_pool_repo: Arc<PgTournamentMapPoolRepository>,
    /// Game repository (default map pool fallback).
    pub game_repo: GameRepository,
}

impl FromRef<AppState> for VetoState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            veto_service: s.veto_service.clone(),
            veto_authorization_service: s.veto_authorization_service.clone(),
            veto_lobby_manager: Arc::clone(&s.veto_lobby_manager),
            plugin_manager: Arc::clone(&s.plugin_manager),
            tournament_service: s.tournament_service.clone(),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
            tournament_map_pool_repo: Arc::clone(&s.tournament_map_pool_repo),
            game_repo: s.game_repo.clone(),
        }
    }
}

/// State slice used by veto-delegate handlers (captain-assigned
/// delegation of pick/ban authority).
#[derive(Clone)]
pub struct VetoDelegatesState {
    /// Veto authorization service.
    pub veto_authorization_service: AppVetoAuthorizationService,
}

impl FromRef<AppState> for VetoDelegatesState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            veto_authorization_service: s.veto_authorization_service.clone(),
        }
    }
}

/// State slice used by the veto-lobby WebSocket handlers.
///
/// The WS entry point needs to authenticate the incoming socket (JWT +
/// user service + permission service for admin/spectator checks),
/// route it to the right lobby (match repo + lobby manager), publish
/// veto-state events (veto service + lobby chat service), and gate
/// pick/ban actions (veto authorization service).
#[derive(Clone)]
pub struct VetoWsState {
    /// Shared JWT signing secret (validate the socket's bearer token).
    pub jwt_secret: Arc<str>,
    /// User service (hydrate the authenticated connection).
    pub user_service: AppUserService,
    /// Permission service (admin / spectator override checks).
    pub permission_service: AppPermissionService,
    /// Veto service (state reads / state mutations).
    pub veto_service: AppVetoService,
    /// Veto authorization service (captain / delegate pick/ban gate).
    pub veto_authorization_service: AppVetoAuthorizationService,
    /// Veto lobby chat service.
    pub veto_lobby_chat_service: AppVetoLobbyChatService,
    /// Veto lobby manager (the in-process hub for fan-out).
    pub veto_lobby_manager: Arc<VetoLobbyManager>,
    /// Tournament match repository.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
}

impl FromRef<AppState> for VetoWsState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            jwt_secret: Arc::clone(&s.jwt_secret),
            user_service: s.user_service.clone(),
            permission_service: s.permission_service.clone(),
            veto_service: s.veto_service.clone(),
            veto_authorization_service: s.veto_authorization_service.clone(),
            veto_lobby_chat_service: s.veto_lobby_chat_service.clone(),
            veto_lobby_manager: Arc::clone(&s.veto_lobby_manager),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
        }
    }
}

// ============================================================================
// INTERNAL (service-to-service, X-API-Key gated)
// ============================================================================

/// State slice used by internal / service-to-service handlers
/// (`handlers/internal.rs`).
///
/// The enricher / scanner daemons call these with an X-API-Key. They
/// catalog discovered matches, submit enriched demo stats, look up
/// steam-tracking targets, and back-fill per-player rating +
/// match-history aggregates from parsed GC data. The trailing
/// `player_game_profile_service` / `rating_history_repo` /
/// `mm_stats_repo` / `match_history_repo` fields are specifically for
/// that enricher flow.
#[derive(Clone)]
pub struct InternalState {
    /// Game repository.
    pub game_repo: GameRepository,
    /// Steam tracking service.
    pub steam_tracking_service: AppSteamTrackingService,
    /// Demo service (catalog + stats submission).
    pub demo_service: AppDemoService,
    /// Discovered match service.
    pub discovered_match_service: AppDiscoveredMatchService,
    /// Player service (resolve steam IDs → portal players).
    pub player_service: AppPlayerService,
    /// API key repository (API key lifecycle endpoints).
    pub api_key_repo: Arc<PgApiKeyRepository>,
    /// Action item repository (captain pending action list).
    pub action_item_repo: ActionItemRepository,
    /// Per-game profile service (enricher creates/updates profiles).
    pub player_game_profile_service: AppPlayerGameProfileService,
    /// Rating history repository (enricher appends Premier rating rows).
    pub rating_history_repo: Arc<PgPlayerRatingHistoryRepository>,
    /// Match-history repository (enricher backfills public match results).
    pub match_history_repo: Arc<PgPlayerMatchHistoryRepository>,
    /// MM-stats repository (enricher accumulates aggregate stats).
    pub mm_stats_repo: Arc<PgPlayerMmStatsRepository>,
}

impl FromRef<AppState> for InternalState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            game_repo: s.game_repo.clone(),
            steam_tracking_service: s.steam_tracking_service.clone(),
            demo_service: s.demo_service.clone(),
            discovered_match_service: s.discovered_match_service.clone(),
            player_service: s.player_service.clone(),
            api_key_repo: Arc::clone(&s.api_key_repo),
            action_item_repo: s.action_item_repo.clone(),
            player_game_profile_service: s.player_game_profile_service.clone(),
            rating_history_repo: Arc::clone(&s.rating_history_repo),
            match_history_repo: Arc::clone(&s.match_history_repo),
            mm_stats_repo: Arc::clone(&s.mm_stats_repo),
        }
    }
}
