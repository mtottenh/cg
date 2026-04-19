//! Domain-scoped sub-states extracted from [`AppState`].
//!
//! # Why this exists
//!
//! `AppState` aggregates every service, repository, and config the portal
//! uses — at the time of writing, 55+ fields. Because Axum re-extracts
//! `State<T>` on every handler call via `FromRef`, and because every
//! handler takes `State(state): State<AppState>`, *all* handlers share the
//! full state surface even when they only need a narrow slice (e.g. login
//! only touches the user service, JWT secret, and refresh-token repo).
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

use std::sync::Arc;

use axum::extract::FromRef;
use portal_db::{PgRefreshTokenRepository, PgTournamentMatchRepository};

use super::{
    AppLeagueTeamService, AppPlayerService, AppRegistrationService, AppState, AppUserService,
    TokenConfig,
};

/// State slice used by authentication / session handlers.
///
/// Wraps everything needed to issue, validate, refresh, and rotate JWTs:
/// the signing secret, the user service (for credential verification),
/// the refresh-token repo (for rotation + replay detection), and the
/// token expiry configuration.
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
            token_config: s.token_config.clone(),
        }
    }
}

/// State slice used by dispute read handlers.
///
/// The participant check in `handlers/dispute.rs::get_dispute` needs:
/// - the dispute service (to load the dispute + thread)
/// - the tournament-match repo (to resolve the match's participants)
/// - the registration service (to inspect each registration)
/// - the league-team service (to check team-season membership)
#[derive(Clone)]
pub struct DisputeReadState {
    /// Dispute service.
    pub dispute_service: super::AppDisputeService,
    /// Registration service.
    pub registration_service: AppRegistrationService,
    /// League-team service (for team-season membership checks).
    pub league_team_service: AppLeagueTeamService,
    /// Tournament match repository.
    pub tournament_match_repo: Arc<PgTournamentMatchRepository>,
}

impl FromRef<AppState> for DisputeReadState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            dispute_service: s.dispute_service.clone(),
            registration_service: s.registration_service.clone(),
            league_team_service: s.league_team_service.clone(),
            tournament_match_repo: Arc::clone(&s.tournament_match_repo),
        }
    }
}

/// State slice used by demo catalog handlers.
///
/// The `cs2_demo_base_url` field carries the per-process demo service URL
/// that was validated at startup by
/// [`portal_plugins::validate_demo_service_url`]; handlers consult it when
/// building download URLs.
#[derive(Clone)]
pub struct DemoState {
    /// Demo service.
    pub demo_service: super::AppDemoService,
    /// Validated CS2 demo service base URL (None if unset at startup).
    pub cs2_demo_base_url: Option<String>,
}

impl FromRef<AppState> for DemoState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            demo_service: s.demo_service.clone(),
            cs2_demo_base_url: s.cs2_demo_base_url.clone(),
        }
    }
}

/// State slice used by league-team handlers.
///
/// Covers team CRUD, invitations, seasons, and the league metadata needed
/// to resolve a season to its league.
#[derive(Clone)]
pub struct LeagueTeamState {
    /// League team service.
    pub league_team_service: AppLeagueTeamService,
    /// League team invitation service.
    pub league_team_invitation_service: super::AppLeagueTeamInvitationService,
    /// League season service (used by team creation to resolve league id).
    pub league_season_service: super::AppLeagueSeasonService,
}

impl FromRef<AppState> for LeagueTeamState {
    fn from_ref(s: &AppState) -> Self {
        Self {
            league_team_service: s.league_team_service.clone(),
            league_team_invitation_service: s.league_team_invitation_service.clone(),
            league_season_service: s.league_season_service.clone(),
        }
    }
}
