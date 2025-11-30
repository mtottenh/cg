//! `OpenAPI` documentation.

use crate::dto::common::{Meta, PaginationMeta, PaginationParams};
use crate::dto::requests::{
    AddLeagueTeamMemberRequest, ApplyToLeagueRequest, ApplyToLeagueTeamRequest,
    CreateBanRequest, CreateLeagueRequest, CreateLeagueSeasonRequest, CreateLeagueTeamRequest,
    CreateTournamentRequest, CreateTournamentStageRequest, InviteToLeagueRequest,
    InviteToLeagueTeamRequest, LiftBanRequest, ListBansQuery, LoginRequest, RegisterPlayerRequest,
    RegisterRequest, RegisterTeamForSeasonRequest, RegisterTeamRequest, RespondToInvitationRequest,
    ScheduleMatchRequest, SetMapPoolRequest, SocialLinksRequest, SubmitMatchResultRequest,
    TransferOwnershipRequest, UpdateGameRequest, UpdateLeagueMemberRoleRequest, UpdateLeagueRequest,
    UpdateLeagueSeasonRequest, UpdateLeagueTeamMemberRequest, UpdateLeagueTeamRequest,
    UpdatePlayerProfileRequest, UpdateTournamentRequest,
};
use crate::dto::responses::{
    BanListResponse, BanResponse, GameDetailResponse, GameSummaryResponse,
    LeagueInvitationResponse, LeagueMemberBasicResponse, LeagueMemberResponse, LeagueResponse,
    LeagueSeasonResponse, LeagueTeamInvitationResponse, LeagueTeamInvitationWithTeamResponse,
    LeagueTeamMemberResponse, LeagueTeamMemberWithPlayerResponse, LeagueTeamResponse,
    LeagueTeamSeasonResponse, LeagueTeamSummaryResponse, LeagueTeamWithSeasonResponse,
    LoginResponse, MapInfoResponse, MapPickBanFormatResponse, PaginationMetaResponse,
    PlatformStatsResponse, PlayerLeagueTeamMembershipResponse, PlayerResponse,
    PlayerSearchResponse, RankTierResponse, RegisterResponse, SocialLinksResponse, TeamSizeConfig,
    TournamentBracketResponse, TournamentMatchResponse, TournamentRegistrationResponse,
    TournamentResponse, TournamentStageResponse, TournamentSummaryResponse,
    UserLeagueMembershipResponse, UserResponse,
};
use crate::error::{ApiError, FieldErrorDto};
use crate::handlers::{
    admin, auth, bans, games, league_teams, leagues, players, tournaments, uploads, users,
};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Gaming Portal API",
        version = "1.0.0",
        description = "Multi-game competitive gaming platform API",
        license(name = "MIT"),
        contact(
            name = "Gaming Portal Team",
            email = "api@gaming-portal.com"
        )
    ),
    servers(
        (url = "/v1", description = "API v1")
    ),
    paths(
        // Admin
        admin::get_stats,
        // Bans
        bans::list_bans,
        bans::get_ban,
        bans::create_ban,
        bans::lift_ban,
        bans::get_user_bans,
        // Auth
        auth::register,
        auth::login,
        // Games
        games::list_games,
        games::get_game,
        games::get_maps,
        games::get_rank_tiers,
        games::update_game,
        games::set_map_pool,
        games::enable_game,
        games::disable_game,
        // Leagues
        leagues::create_league,
        leagues::get_league,
        leagues::get_league_by_slug,
        leagues::list_leagues,
        leagues::update_league,
        leagues::list_members,
        leagues::join_league,
        leagues::leave_league,
        leagues::update_member_role,
        leagues::remove_member,
        leagues::apply_to_league,
        leagues::invite_user,
        leagues::list_invitations,
        leagues::list_applications,
        leagues::approve_application,
        leagues::reject_application,
        leagues::get_my_leagues,
        leagues::get_my_invitations,
        leagues::accept_invitation,
        leagues::decline_invitation,
        // Players
        players::search_players,
        players::get_player,
        players::get_my_profile,
        players::update_my_profile,
        uploads::upload_player_avatar,
        uploads::upload_player_banner,
        // Users
        users::get_current_user,
        // League Seasons
        league_teams::season::create_season,
        league_teams::season::get_season,
        league_teams::season::list_seasons,
        league_teams::season::update_season,
        // League Teams (Persistent Identity)
        league_teams::team::create_team,
        league_teams::team::get_team,
        league_teams::team::list_teams_in_season,
        league_teams::team::update_team,
        league_teams::team::disband_team,
        league_teams::team::transfer_ownership,
        league_teams::team::register_team_for_season,
        // League Team Seasons
        league_teams::team_season::get_team_season,
        // League Team Members (Seasonal Roster)
        league_teams::team_season::get_team_season_members,
        league_teams::team_season::add_team_member,
        league_teams::team_season::remove_team_member,
        league_teams::team_season::leave_team,
        league_teams::team_season::promote_to_captain,
        league_teams::team_season::demote_from_captain,
        // Player league teams
        league_teams::team_season::get_my_league_teams,
        league_teams::team_season::get_player_league_teams,
        // League Team Invitations
        league_teams::invitation::invite_to_team,
        league_teams::invitation::apply_to_team,
        league_teams::invitation::get_my_invitations,
        league_teams::invitation::get_team_invitations,
        league_teams::invitation::accept_invitation,
        league_teams::invitation::decline_invitation,
        league_teams::invitation::cancel_invitation,
        // Tournaments
        tournaments::create_tournament,
        tournaments::get_tournament,
        tournaments::get_tournament_by_slug,
        tournaments::list_tournaments,
        tournaments::update_tournament,
        tournaments::publish_tournament,
        tournaments::open_registration,
        tournaments::start_tournament,
        tournaments::create_stage,
        tournaments::get_stages,
        tournaments::register_team,
        tournaments::register_player,
        tournaments::get_registrations,
        tournaments::check_in,
        tournaments::get_brackets,
        tournaments::get_matches,
    ),
    components(
        schemas(
            // Common
            Meta,
            PaginationMeta,
            PaginationParams,

            // Errors
            ApiError,
            FieldErrorDto,

            // Games
            GameSummaryResponse,
            GameDetailResponse,
            TeamSizeConfig,
            MapInfoResponse,
            RankTierResponse,
            MapPickBanFormatResponse,
            UpdateGameRequest,
            SetMapPoolRequest,

            // Players
            PlayerResponse,
            PlayerSearchResponse,
            SocialLinksRequest,
            SocialLinksResponse,
            UpdatePlayerProfileRequest,

            // Users
            UserResponse,

            // Auth
            RegisterRequest,
            RegisterResponse,
            LoginRequest,
            LoginResponse,

            // Leagues
            LeagueResponse,
            LeagueMemberResponse,
            LeagueMemberBasicResponse,
            LeagueInvitationResponse,
            UserLeagueMembershipResponse,
            CreateLeagueRequest,
            UpdateLeagueRequest,
            InviteToLeagueRequest,
            ApplyToLeagueRequest,
            UpdateLeagueMemberRoleRequest,

            // Admin
            PlatformStatsResponse,

            // Bans
            BanResponse,
            BanListResponse,
            PaginationMetaResponse,
            CreateBanRequest,
            LiftBanRequest,
            ListBansQuery,

            // League Seasons
            LeagueSeasonResponse,
            CreateLeagueSeasonRequest,
            UpdateLeagueSeasonRequest,

            // League Teams
            LeagueTeamResponse,
            LeagueTeamSeasonResponse,
            LeagueTeamWithSeasonResponse,
            LeagueTeamSummaryResponse,
            CreateLeagueTeamRequest,
            UpdateLeagueTeamRequest,
            RegisterTeamForSeasonRequest,
            TransferOwnershipRequest,

            // League Team Members
            LeagueTeamMemberResponse,
            LeagueTeamMemberWithPlayerResponse,
            PlayerLeagueTeamMembershipResponse,
            AddLeagueTeamMemberRequest,
            UpdateLeagueTeamMemberRequest,

            // League Team Invitations
            LeagueTeamInvitationResponse,
            LeagueTeamInvitationWithTeamResponse,
            InviteToLeagueTeamRequest,
            ApplyToLeagueTeamRequest,
            RespondToInvitationRequest,

            // Tournaments
            TournamentResponse,
            TournamentSummaryResponse,
            TournamentStageResponse,
            TournamentBracketResponse,
            TournamentRegistrationResponse,
            TournamentMatchResponse,
            CreateTournamentRequest,
            UpdateTournamentRequest,
            CreateTournamentStageRequest,
            RegisterTeamRequest,
            RegisterPlayerRequest,
            ScheduleMatchRequest,
            SubmitMatchResultRequest,
        )
    ),
    tags(
        (name = "admin", description = "Admin dashboard and management"),
        (name = "bans", description = "Ban management for platform moderation"),
        (name = "auth", description = "Authentication"),
        (name = "games", description = "Game metadata and configuration"),
        (name = "leagues", description = "League management"),
        (name = "league-seasons", description = "League season management"),
        (name = "league-teams", description = "League team management (persistent identity)"),
        (name = "league-team-seasons", description = "League team seasonal participation and roster management"),
        (name = "league-team-invitations", description = "League team invitations and applications"),
        (name = "players", description = "Player profiles"),
        (name = "users", description = "User account management"),
        (name = "tournaments", description = "Tournament management and bracket generation")
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

/// Handler that returns the `OpenAPI` spec as JSON.
async fn openapi_json() -> impl IntoResponse {
    (StatusCode::OK, Json(ApiDoc::openapi()))
}

/// Create `OpenAPI` documentation routes.
///
/// Provides:
/// - `/api-docs/openapi.json` - `OpenAPI` specification as JSON
pub fn openapi_routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/api-docs/openapi.json", get(openapi_json))
}

/// Create Swagger UI routes.
///
/// Provides interactive API documentation at `/swagger-ui`.
pub fn swagger_routes() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}
