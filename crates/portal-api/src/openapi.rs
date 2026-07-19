//! `OpenAPI` documentation.

use crate::dto::common::{Meta, PaginationMeta, PaginationParams};
use crate::dto::requests::{
    AcceptScheduleProposalRequest, AddDisputeMessageRequest, AddLeagueTeamMemberRequest,
    AddLinkEvidenceRequest, AddMapRequest, AddPermissionToRoleRequest, AdminDisputeMessageRequest,
    AdminDisqualifyRequest, AdminDoubleForfeitRequest, AdminForfeitMatchRequest,
    AdminMatchTransitionRequest, AdminScheduleRequest, ApplyToLeagueRequest,
    ApplyToLeagueTeamRequest, AssignRoleRequest, AssociateDemoRequest, AutoSeedRequest,
    BatchCatalogDemoEntry, BatchCatalogDemosRequest, CatalogDemoRequest, CategorizeDemoRequest,
    CounterProposeRequest, CreateAvailabilityOverrideRequest, CreateAvailabilityWindowRequest,
    CreateBanRequest, CreateLeagueRequest, CreateLeagueSeasonRequest, CreateLeagueTeamRequest,
    CreateRoleRequest, CreateTournamentRequest, CreateTournamentStageRequest,
    CreateVetoSessionRequest, DemoPlayerInputDto, DisputeResultClaimRequest, DisqualifyRequest,
    ForfeitMatchRequest, GenerateSuggestionsRequest, GetAvailabilityQuery, GetDemosForMatchQuery,
    InitiateUploadRequest, InviteToLeagueRequest, InviteToLeagueTeamRequest, LiftBanRequest,
    LinkDemoRequest, LinkDemoToMatchRequest, LinkDiscoveredEvidenceRequest, ListBansQuery,
    ListDisputesQuery, LoginRequest, ManualSeedRequest, MarkDemoFailedRequest, MatchCheckInRequest,
    PerformVetoActionRequest, ProcessProgressionRequest, ProposeScheduleRequest,
    RaiseDisputeRequest, RankTierInput, ReapplyProgressionRequest, RecordCoinFlipRequest,
    RefreshTokenRequest, RegisterPlayerRequest, RegisterRequest, RegisterTeamForSeasonRequest,
    RegisterTeamRequest, RejectRegistrationRequest, RejectScheduleProposalRequest,
    ResolveAdjustedRequest, ResolveDoubleDqRequest, ResolveOverturnRequest, ResolveRematchRequest,
    ResolveUpholdRequest, RespondToInvitationRequest, RevokeRoleRequest, ScheduleMatchRequest,
    SeedAssignment, SelectSideRequest, SetDemoNotesRequest, SetDemoVisibilityRequest,
    SetMapPoolRequest, SetRankTiersRequest, SetTournamentMapPoolRequest, SocialLinksRequest,
    SubmitDemoStatsRequest, SubmitMatchResultRequest, SubmitResultClaimRequest,
    TransferOwnershipRequest, UpdateAvailabilityWindowRequest, UpdateGameRequest,
    UpdateLeagueMemberRoleRequest, UpdateLeagueRequest, UpdateLeagueSeasonRequest,
    UpdateLeagueTeamMemberRequest, UpdateLeagueTeamRequest, UpdateMapRequest,
    UpdatePlayerProfileRequest, UpdateRoleRequest, UpdateTeamSizeRequest, UpdateTournamentRequest,
    ValidateDemoRequest, ValidateEvidenceRequest, WithdrawFromTournamentRequest,
};
use crate::dto::responses::demo::{
    BatchCatalogErrorResponse, BatchCatalogResultResponse, DemoDownloadResponse, DemoListResponse,
    DemoMatchLinkResponse, DemoMatchLinkWithDemoResponse, DemoMetadataResponse, DemoPlayerResponse,
    DemoPlayerStatsResponse as DemoCatalogPlayerStatsResponse, DemoResponse,
    DemoStatusCountsResponse, DemoValidationResultResponse,
};
use crate::dto::responses::{
    AccessUrlResponse, AdvancementResponse, AvailabilityOverrideResponse,
    AvailabilityWindowResponse, BanListResponse, BanResponse, CheckInStatusResponse,
    DateAvailabilityResponse, DemoPlayerStatsResponse, DemoStatsResponse, DemoValidationResponse,
    DiscoveredEvidenceResponse, DisputeListResponse, DisputeMessageResponse,
    DisputeResolutionResponse, DisputeResolutionResultResponse, DisputeResponse,
    DisputeWithThreadResponse, DisqualificationResponse, EvidenceResponse, EvidenceSummaryResponse,
    ExtractedResultResponse, ForfeitRecordResponse, ForfeitResponse, GameDetailResponse,
    GameResultResponse, GameSummaryResponse, LeagueInvitationResponse, LeagueMemberBasicResponse,
    LeagueMemberResponse, LeagueResponse, LeagueSeasonResponse, LeagueTeamInvitationResponse,
    LeagueTeamInvitationWithTeamResponse, LeagueTeamMemberResponse,
    LeagueTeamMemberWithPlayerResponse, LeagueTeamResponse, LeagueTeamSeasonResponse,
    LeagueTeamSummaryResponse, LeagueTeamWithSeasonResponse, LoginResponse, LoserResultResponse,
    MapInfoResponse, MapPickBanFormatResponse, MapStatusResponse, MatchStatusDetailsResponse,
    MatchStatusLogResponse, PaginationMetaResponse, PermissionResponse, PlatformStatsResponse,
    PlayerLeagueTeamMembershipResponse, PlayerResponse, PlayerSearchResponse, ProgressionResponse,
    RankTierResponse, RegisterResponse, ResultClaimResponse, ResultClaimSubmissionResponse,
    ResultConfirmationResponse, ResultDisputeResponse, RoleResponse, RoleWithPermissionsResponse,
    ScheduleProposalResponse, SeededParticipantResponse, SocialLinksResponse,
    SuggestedTimeResponse, TeamSizeConfig, TimeSlotResponse, TournamentBracketResponse,
    TournamentMapPoolResponse, TournamentMatchResponse, TournamentRegistrationResponse,
    TournamentResponse, TournamentStageResponse, TournamentSummaryResponse, UploadInfoResponse,
    UserLeagueMembershipResponse, UserResponse, UserRoleAssignmentResponse,
    ValidationResultResponse, VetoActionResponse, VetoActionResultResponse, VetoFormatResponse,
    VetoSessionResponse, VetoSessionStateResponse, WithdrawalResponse,
};
use crate::error::{ApiError, FieldErrorDto};
use crate::handlers::{
    admin, auth, availability, bans, demos, dispute, evidence, forfeit, games, league_teams,
    leagues, player_game_profiles, players, progression, result_reviews, results, roles,
    steam_tracking, tournaments, uploads, users, veto, veto_delegates,
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
        // Roles and Permissions (RBAC)
        roles::list_roles,
        roles::create_role,
        roles::get_role,
        roles::update_role,
        roles::delete_role,
        roles::add_permission_to_role,
        roles::remove_permission_from_role,
        roles::list_permissions,
        roles::get_user_roles,
        roles::assign_role_to_user,
        roles::revoke_role_from_user,
        // Auth
        auth::register,
        auth::login,
        auth::refresh,
        // Games
        games::list_games,
        games::get_game,
        games::get_maps,
        games::get_rank_tiers,
        games::update_game,
        games::set_map_pool,
        games::enable_game,
        games::disable_game,
        games::add_map,
        games::update_map,
        games::remove_map,
        games::set_rank_tiers,
        games::update_team_size,
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
        player_game_profiles::list_player_game_profiles,
        player_game_profiles::get_player_game_profile,
        player_game_profiles::get_my_game_profiles,
        player_game_profiles::submit_player_rating,
        player_game_profiles::get_player_rating_history,
        player_game_profiles::get_player_mm_stats,
        player_game_profiles::get_player_match_history,
        uploads::upload_player_avatar,
        uploads::upload_player_banner,
        uploads::upload_team_logo,
        uploads::upload_team_banner,
        // Users
        users::get_current_user,
        users::get_my_roles,
        users::get_my_matches,
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
        tournaments::close_registration,
        tournaments::reopen_registration,
        tournaments::cancel_tournament,
        tournaments::complete_tournament,
        tournaments::finalize_tournament,
        tournaments::create_stage,
        tournaments::get_stages,
        tournaments::register_team,
        tournaments::register_player,
        tournaments::get_registrations,
        tournaments::check_in,
        // Registration management
        tournaments::withdraw,
        tournaments::approve_registration,
        tournaments::reject_registration,
        tournaments::disqualify,
        tournaments::admin_check_in,
        tournaments::get_check_in_status,
        tournaments::process_no_shows,
        // Seeding
        tournaments::get_seeding,
        tournaments::auto_seed,
        tournaments::manual_seed,
        tournaments::clear_seeding,
        // Brackets and matches
        tournaments::get_brackets,
        tournaments::get_matches,
        tournaments::get_match,
        // Match lifecycle
        tournaments::get_match_status,
        tournaments::get_match_status_history,
        tournaments::match_check_in,
        tournaments::schedule_match,
        tournaments::forfeit_match,
        tournaments::admin_match_transition,
        // Match scheduling (proposal workflow)
        tournaments::propose_schedule,
        tournaments::accept_schedule_proposal,
        tournaments::reject_schedule_proposal,
        tournaments::counter_propose,
        tournaments::get_active_proposal,
        tournaments::get_proposal_history,
        tournaments::admin_schedule_match,
        // Standings
        tournaments::get_bracket_standings,
        // Swiss next round
        tournaments::admin_generate_next_swiss_round,
        // Tournament map pool
        tournaments::get_tournament_map_pool,
        tournaments::set_tournament_map_pool,
        tournaments::delete_tournament_map_pool,
        // Availability
        availability::create_player_window,
        availability::get_player_windows,
        availability::update_player_window,
        availability::delete_player_window,
        availability::create_player_override,
        availability::get_player_overrides,
        availability::delete_player_override,
        availability::get_player_date_availability,
        availability::get_player_date_availability_public,
        availability::generate_suggestions,
        availability::get_suggestions,
        // Result submission
        results::submit_result,
        results::get_result_claim,
        results::list_result_claims,
        results::confirm_result,
        results::dispute_result,
        // Veto (map pick/ban)
        veto::create_veto_session,
        veto::get_veto_session,
        veto::start_veto_session,
        veto::record_coin_flip,
        veto::perform_veto_action,
        veto::select_side,
        // Veto delegates
        veto_delegates::create_delegation,
        veto_delegates::list_delegations,
        veto_delegates::revoke_delegation,
        // Evidence (match evidence management)
        evidence::initiate_upload,
        evidence::complete_upload,
        evidence::add_link_evidence,
        evidence::list_evidence,
        evidence::get_evidence,
        evidence::get_access_url,
        evidence::delete_evidence,
        evidence::discover_evidence,
        evidence::link_discovered_evidence,
        evidence::validate_evidence,
        // CS2 Demo validation
        evidence::validate_demo,
        evidence::get_demo_stats,
        evidence::link_demo,
        // Progression (bracket advancement)
        progression::get_progression,
        progression::revert_progression,
        progression::reapply_progression,
        progression::process_progression,
        // Forfeits
        forfeit::withdraw_from_tournament,
        forfeit::admin_forfeit_match,
        forfeit::admin_disqualify,
        forfeit::admin_double_forfeit,
        // Disputes
        dispute::raise_dispute,
        dispute::get_match_dispute,
        dispute::add_dispute_message,
        dispute::get_dispute,
        dispute::admin_list_disputes,
        dispute::admin_add_message,
        dispute::admin_assign_dispute,
        dispute::admin_resolve_uphold,
        dispute::admin_resolve_overturn,
        dispute::admin_resolve_rematch,
        dispute::admin_resolve_adjusted,
        dispute::admin_resolve_double_dq,
        // Steam tracking
        steam_tracking::register_tracking,
        steam_tracking::get_tracking,
        steam_tracking::update_tracking,
        steam_tracking::delete_tracking,
        // Demo catalog
        demos::list_demos,
        demos::get_demo,
        demos::get_demo_players,
        demos::get_demo_links,
        demos::get_demo_download,
        demos::catalog_demo,
        demos::categorize_demo,
        demos::set_demo_visibility,
        demos::associate_demo,
        demos::link_demo_to_match,
        demos::get_demo_status_counts,
        demos::get_pending_demos,
        demos::get_demos_for_match,
        demos::unlink_demo_from_match,
        demos::batch_catalog_demos,
        demos::submit_demo_stats,
        demos::mark_demo_stats_failed,
        demos::delete_demo,
        demos::set_demo_notes,
        // Result reviews
        result_reviews::get_result_review,
        result_reviews::acknowledge_result_review,
        result_reviews::list_pending_reviews,
        result_reviews::get_result_review_by_id,
        result_reviews::approve_result_review,
        result_reviews::reject_result_review,
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
            AddMapRequest,
            UpdateMapRequest,
            SetRankTiersRequest,
            RankTierInput,
            UpdateTeamSizeRequest,

            // Players
            PlayerResponse,
            PlayerSearchResponse,
            SocialLinksRequest,
            SocialLinksResponse,
            UpdatePlayerProfileRequest,
            crate::dto::responses::PlayerGameProfileResponse,
            crate::dto::responses::DisplayStatResponse,
            crate::dto::responses::PlayerRatingHistoryResponse,
            crate::dto::responses::PublicMmStatsResponse,
            crate::dto::responses::MatchHistoryEntryResponse,
            crate::dto::requests::SubmitRatingRequest,
            crate::handlers::player_game_profiles::RatingHistoryQuery,
            crate::handlers::player_game_profiles::MatchHistoryQuery,

            // Users
            UserResponse,
            crate::dto::requests::tournament::MyMatchesQuery,

            // Auth
            RegisterRequest,
            RegisterResponse,
            LoginRequest,
            LoginResponse,
            RefreshTokenRequest,

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

            // Roles and Permissions (RBAC)
            RoleResponse,
            RoleWithPermissionsResponse,
            PermissionResponse,
            UserRoleAssignmentResponse,
            CreateRoleRequest,
            UpdateRoleRequest,
            AssignRoleRequest,
            RevokeRoleRequest,
            AddPermissionToRoleRequest,

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
            crate::dto::responses::tournament::EligibilityRestrictionsResponse,
            crate::dto::requests::tournament::EligibilityRestrictionsInput,
            TournamentStageResponse,
            TournamentBracketResponse,
            TournamentRegistrationResponse,
            TournamentMatchResponse,
            SeededParticipantResponse,
            crate::dto::responses::TournamentStandingResponse,
            CheckInStatusResponse,
            TournamentMapPoolResponse,
            SetTournamentMapPoolRequest,
            CreateTournamentRequest,
            UpdateTournamentRequest,
            CreateTournamentStageRequest,
            RegisterTeamRequest,
            RegisterPlayerRequest,
            ScheduleMatchRequest,
            SubmitMatchResultRequest,
            RejectRegistrationRequest,
            DisqualifyRequest,
            AutoSeedRequest,
            ManualSeedRequest,
            SeedAssignment,
            // Match lifecycle
            MatchStatusDetailsResponse,
            MatchStatusLogResponse,
            MatchCheckInRequest,
            AdminMatchTransitionRequest,
            ForfeitMatchRequest,
            // Match scheduling
            ScheduleProposalResponse,
            ProposeScheduleRequest,
            AcceptScheduleProposalRequest,
            RejectScheduleProposalRequest,
            CounterProposeRequest,
            AdminScheduleRequest,
            // Availability
            AvailabilityWindowResponse,
            AvailabilityOverrideResponse,
            DateAvailabilityResponse,
            TimeSlotResponse,
            SuggestedTimeResponse,
            CreateAvailabilityWindowRequest,
            UpdateAvailabilityWindowRequest,
            CreateAvailabilityOverrideRequest,
            GetAvailabilityQuery,
            GenerateSuggestionsRequest,
            // Result submission
            SubmitResultClaimRequest,
            crate::dto::requests::result::GameResultInput,
            DisputeResultClaimRequest,
            ResultClaimResponse,
            ResultClaimSubmissionResponse,
            ResultConfirmationResponse,
            ResultDisputeResponse,
            GameResultResponse,
            // Veto (map pick/ban)
            CreateVetoSessionRequest,
            RecordCoinFlipRequest,
            PerformVetoActionRequest,
            SelectSideRequest,
            VetoSessionResponse,
            VetoSessionStateResponse,
            VetoActionResponse,
            VetoActionResultResponse,
            VetoFormatResponse,
            MapStatusResponse,
            // Veto delegates
            crate::dto::requests::CreateVetoDelegateRequest,
            crate::dto::responses::VetoDelegateResponse,
            crate::dto::responses::VetoDelegateListResponse,
            // Evidence
            InitiateUploadRequest,
            AddLinkEvidenceRequest,
            LinkDiscoveredEvidenceRequest,
            ValidateEvidenceRequest,
            EvidenceResponse,
            EvidenceSummaryResponse,
            UploadInfoResponse,
            AccessUrlResponse,
            DiscoveredEvidenceResponse,
            ValidationResultResponse,
            ExtractedResultResponse,
            // CS2 Demo Validation
            ValidateDemoRequest,
            LinkDemoRequest,
            DemoValidationResponse,
            DemoStatsResponse,
            DemoPlayerStatsResponse,
            // Progression
            ProcessProgressionRequest,
            ReapplyProgressionRequest,
            ProgressionResponse,
            AdvancementResponse,
            LoserResultResponse,
            // Forfeits
            WithdrawFromTournamentRequest,
            AdminForfeitMatchRequest,
            AdminDisqualifyRequest,
            AdminDoubleForfeitRequest,
            ForfeitRecordResponse,
            ForfeitResponse,
            WithdrawalResponse,
            DisqualificationResponse,
            // Disputes
            RaiseDisputeRequest,
            AddDisputeMessageRequest,
            AdminDisputeMessageRequest,
            ListDisputesQuery,
            ResolveUpholdRequest,
            ResolveOverturnRequest,
            ResolveRematchRequest,
            ResolveAdjustedRequest,
            ResolveDoubleDqRequest,
            DisputeResponse,
            DisputeResolutionResponse,
            DisputeMessageResponse,
            DisputeWithThreadResponse,
            DisputeResolutionResultResponse,
            DisputeListResponse,
            // Demo Catalog
            CatalogDemoRequest,
            CategorizeDemoRequest,
            SetDemoVisibilityRequest,
            AssociateDemoRequest,
            LinkDemoToMatchRequest,
            GetDemosForMatchQuery,
            DemoResponse,
            DemoListResponse,
            DemoMatchLinkResponse,
            DemoMatchLinkWithDemoResponse,
            DemoPlayerResponse,
            DemoMetadataResponse,
            DemoStatusCountsResponse,
            DemoCatalogPlayerStatsResponse,
            DemoValidationResultResponse,
            // Demo Ingestion
            BatchCatalogDemosRequest,
            BatchCatalogDemoEntry,
            BatchCatalogResultResponse,
            BatchCatalogErrorResponse,
            SubmitDemoStatsRequest,
            DemoPlayerInputDto,
            MarkDemoFailedRequest,
            SetDemoNotesRequest,
            DemoDownloadResponse,
            // Steam Tracking
            crate::handlers::steam_tracking::RegisterSteamTrackingRequest,
            crate::handlers::steam_tracking::UpdateSteamTrackingRequest,
            crate::handlers::steam_tracking::SteamTrackingResponse,
            // Internal (enricher)
            crate::handlers::internal::DemoPlayerRating,
            // Result Reviews
            crate::dto::requests::AdminReviewDecisionRequest,
            crate::dto::responses::ResultReviewResponse,
            crate::dto::responses::ResultReviewSummaryResponse,
            crate::dto::responses::ResultReviewListResponse,
            crate::dto::responses::AcknowledgmentResponse,
            crate::dto::responses::UnrecognizedPlayerResponse,
        )
    ),
    tags(
        (name = "admin", description = "Admin dashboard and management"),
        (name = "admin_rbac", description = "Role-based access control management"),
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
        (name = "tournaments", description = "Tournament management and bracket generation"),
        (name = "match_lifecycle", description = "Match status management and state transitions"),
        (name = "match_scheduling", description = "Match scheduling proposals and negotiation"),
        (name = "availability", description = "Player and participant availability management"),
        (name = "results", description = "Match result submission and confirmation"),
        (name = "veto", description = "Map pick/ban (veto) system for matches"),
        (name = "veto_delegates", description = "Veto delegation management (authorize others to perform picks/bans)"),
        (name = "evidence", description = "Match evidence management (demos, screenshots, videos)"),
        (name = "progression", description = "Bracket progression and winner advancement management"),
        (name = "forfeits", description = "Forfeit handling (withdrawal, disqualification, no-show)"),
        (name = "disputes", description = "Dispute workflow and admin resolution"),
        (name = "demos", description = "Demo file catalog and browsing"),
        (name = "result_reviews", description = "Result review and validation discrepancy handling"),
        (name = "steam_tracking", description = "CS2 Steam match tracking registration")
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
