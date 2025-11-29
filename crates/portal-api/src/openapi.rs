//! OpenAPI documentation.

use crate::dto::common::{Meta, PaginationMeta, PaginationParams};
use crate::dto::requests::{
    ApplyToLeagueRequest, CreateLeagueRequest, CreateTeamRequest, InvitePlayerRequest,
    InviteToLeagueRequest, LoginRequest, RegisterRequest, SetMapPoolRequest, SocialLinksRequest,
    UpdateGameRequest, UpdateLeagueMemberRoleRequest, UpdateLeagueRequest, UpdateMemberRoleRequest,
    UpdatePlayerProfileRequest, UpdateTeamRequest,
};
use crate::dto::responses::{
    GameDetailResponse, GameSummaryResponse, InvitationCountResponse, LeagueInvitationResponse,
    LeagueMemberBasicResponse, LeagueMemberResponse, LeagueResponse, LoginResponse, MapInfoResponse,
    MapPickBanFormatResponse, PlatformStatsResponse, PlayerResponse, PlayerSearchResponse,
    PlayerTeamMembershipResponse, RankTierResponse, RegisterResponse, SocialLinksResponse,
    TeamInvitationResponse, TeamInvitationWithTeamResponse, TeamMemberResponse, TeamResponse,
    TeamSizeConfig, TeamWithMembersResponse, UserLeagueMembershipResponse, UserResponse,
};
use crate::error::{ApiError, FieldErrorDto};
use crate::handlers::{admin, auth, games, invitations, leagues, players, teams, uploads, users};
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
        // Teams
        teams::create_team,
        teams::get_team,
        teams::list_teams,
        teams::update_team,
        teams::list_members,
        teams::update_member_role,
        teams::remove_member,
        teams::leave_team,
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
        players::get_player_teams,
        players::get_my_profile,
        players::update_my_profile,
        uploads::upload_player_avatar,
        uploads::upload_player_banner,
        // Uploads (Teams)
        uploads::upload_team_logo,
        uploads::upload_team_banner,
        // Users
        users::get_current_user,
        // Invitations
        invitations::invite_player,
        invitations::get_team_invitations,
        invitations::get_my_invitations,
        invitations::count_my_invitations,
        invitations::accept_invitation,
        invitations::decline_invitation,
        invitations::cancel_invitation,
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

            // Teams
            TeamResponse,
            TeamMemberResponse,
            TeamWithMembersResponse,
            CreateTeamRequest,
            UpdateTeamRequest,
            UpdateMemberRoleRequest,

            // Players
            PlayerResponse,
            PlayerSearchResponse,
            PlayerTeamMembershipResponse,
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

            // Invitations
            InvitePlayerRequest,
            TeamInvitationResponse,
            TeamInvitationWithTeamResponse,
            InvitationCountResponse,

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
        )
    ),
    tags(
        (name = "admin", description = "Admin dashboard and management"),
        (name = "auth", description = "Authentication"),
        (name = "games", description = "Game metadata and configuration"),
        (name = "teams", description = "Team management"),
        (name = "leagues", description = "League management"),
        (name = "players", description = "Player profiles"),
        (name = "users", description = "User account management"),
        (name = "invitations", description = "Team invitation management")
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

/// Handler that returns the OpenAPI spec as JSON.
async fn openapi_json() -> impl IntoResponse {
    (StatusCode::OK, Json(ApiDoc::openapi()))
}

/// Create OpenAPI documentation routes.
///
/// Provides:
/// - `/api-docs/openapi.json` - OpenAPI specification as JSON
pub fn openapi_routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/api-docs/openapi.json", get(openapi_json))
}

/// Create Swagger UI routes.
///
/// Provides interactive API documentation at `/swagger-ui`.
pub fn swagger_routes() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}
