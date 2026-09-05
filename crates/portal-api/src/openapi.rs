//! `OpenAPI` documentation.

use crate::dto::common::{Meta, PaginationMeta, PaginationParams};
use crate::dto::requests::{
    AcceptScheduleProposalRequest, AddDisputeMessageRequest, AddLeagueTeamMemberRequest,
    AddLinkEvidenceRequest, AddMapRequest, AddPermissionToRoleRequest, AdminDisputeMessageRequest,
    AdminDisqualifyRequest, AdminDoubleForfeitRequest, AdminForfeitMatchRequest,
    AdminMatchTransitionRequest, AdminOverrideMatchResultRequest, AdminScheduleRequest,
    ApplyToLeagueRequest, ApplyToLeagueTeamRequest, AssignRoleRequest, AssociateDemoRequest,
    AutoSeedRequest, BatchCatalogDemoEntry, BatchCatalogDemosRequest,
    CancelScheduleProposalRequest, CatalogDemoRequest, CategorizeDemoRequest,
    CounterProposeRequest, CreateAvailabilityOverrideRequest, CreateAvailabilityWindowRequest,
    CreateBanRequest, CreateLeagueRequest, CreateLeagueSeasonRequest, CreateLeagueTeamRequest,
    CreateRoleRequest, CreateTournamentRequest, CreateTournamentStageRequest,
    CreateVetoSessionRequest, DeclareLineupRequest, DemoPlayerInputDto, DisputeResultClaimRequest,
    DisqualifyRequest, ForfeitMatchRequest, GenerateSuggestionsRequest, GetAvailabilityQuery,
    GetDemosForMatchQuery, InitiateUploadRequest, InviteToLeagueRequest, InviteToLeagueTeamRequest,
    LiftBanRequest, LinkDemoRequest, LinkDemoToMatchRequest, LinkDiscoveredEvidenceRequest,
    ListBansQuery, ListDisputesQuery, LoginRequest, ManualSeedRequest, MarkDemoFailedRequest,
    MatchCheckInRequest, MoveTeamRequest, MoveTournamentRequest, PerformVetoActionRequest,
    ProcessProgressionRequest, ProposeScheduleRequest, RaiseDisputeRequest, RankTierInput,
    ReapplyProgressionRequest, RecordCoinFlipRequest, RefreshTokenRequest, RegisterPlayerRequest,
    RegisterRequest, RegisterTeamForSeasonRequest, RegisterTeamRequest, RejectRegistrationRequest,
    RejectScheduleProposalRequest, RequeueDiscoveredMatchesRequest, ResolveAdjustedRequest,
    ResolveDoubleDqRequest, ResolveOverturnRequest, ResolveRematchRequest, ResolveUpholdRequest,
    RespondToInvitationRequest, RevokeRoleRequest, SeedAssignment, SelectSideRequest,
    SetDemoNotesRequest, SetDemoVisibilityRequest, SetMapPoolRequest, SetRankTiersRequest,
    SetTournamentMapPoolRequest, SocialLinksRequest, SubmitDemoStatsRequest,
    SubmitMatchResultRequest, SubmitResultClaimRequest, TransferOwnershipRequest,
    UpdateAutoLinkSettingRequest, UpdateAvailabilityWindowRequest, UpdateGameRequest,
    UpdateLeagueMemberRoleRequest, UpdateLeagueRequest, UpdateLeagueSeasonRequest,
    UpdateLeagueTeamMemberRequest, UpdateLeagueTeamRequest, UpdateMapRequest,
    UpdatePlayerProfileRequest, UpdateRoleRequest, UpdateTeamSizeRequest, UpdateTournamentRequest,
    UpdateTournamentStageRequest, ValidateDemoRequest, ValidateEvidenceRequest,
    WithdrawFromTournamentRequest,
};
use crate::dto::responses::AutoLinkSettingResponse;
use crate::dto::responses::demo::{
    BatchCatalogErrorResponse, BatchCatalogResultResponse, DemoDownloadResponse, DemoListResponse,
    DemoMatchLinkResponse, DemoMatchLinkWithDemoResponse, DemoMetadataResponse, DemoPlayerResponse,
    DemoPlayerStatsResponse as DemoCatalogPlayerStatsResponse, DemoResponse,
    DemoStatusCountsResponse, DemoValidationResultResponse, ProcessUnlinkedDemosResponse,
};
use crate::dto::responses::pipeline::{
    DemoExtractionQueueResponse, DiscoveredMatchAdminResponse, DiscoveredMatchQueueResponse,
    PipelineOverviewResponse, RequeueDiscoveredMatchesResponse, TrackingHealthEntryResponse,
    TrackingHealthSummaryResponse,
};
use crate::dto::responses::{
    AccessUrlResponse, AdvancementResponse, AvailabilityOverrideResponse,
    AvailabilityWindowResponse, BanListResponse, BanResponse, CheckInStatusResponse,
    DateAvailabilityResponse, DemoPlayerStatsResponse, DemoStatsResponse, DemoValidationResponse,
    DiscoveredEvidenceResponse, DisputeListResponse, DisputeMessageResponse,
    DisputeResolutionResponse, DisputeResolutionResultResponse, DisputeResponse,
    DisputeWithThreadResponse, DisqualificationResponse, EntityChangeResponse, EvidenceResponse,
    EvidenceSummaryResponse, ExtractedResultResponse, ForfeitRecordResponse, ForfeitResponse,
    GameDetailResponse, GameResultResponse, GameSummaryResponse, LeagueInvitationResponse,
    LeagueMemberBasicResponse, LeagueMemberResponse, LeagueResponse, LeagueSeasonResponse,
    LeagueTeamInvitationResponse, LeagueTeamInvitationWithTeamResponse, LeagueTeamMemberResponse,
    LeagueTeamMemberWithPlayerResponse, LeagueTeamResponse, LeagueTeamSeasonResponse,
    LeagueTeamSummaryResponse, LeagueTeamWithSeasonResponse, LoginResponse, LogoutResponse,
    LoserResultResponse, MapInfoResponse, MapPickBanFormatResponse, MapStatusResponse,
    MatchLineupPlayerResponse, MatchLineupResponse, MatchParticipantsResponse,
    MatchResultOverrideResponse, MatchStatusDetailsResponse, MatchStatusLogResponse,
    MovedTeamResponse, MyTournamentRegistrationsResponse, PaginationMetaResponse,
    PermissionResponse, PlatformStatsResponse, PlayerLeagueTeamMembershipResponse, PlayerResponse,
    PlayerSearchResponse, ProgressionResponse, RankTierResponse, RegisterResponse,
    ResultClaimResponse, ResultClaimSubmissionResponse, ResultConfirmationResponse,
    ResultDisputeResponse, RoleResponse, RoleWithPermissionsResponse, ScheduleProposalResponse,
    SeededParticipantResponse, SocialLinksResponse, SuggestedTimeResponse, TeamSizeConfig,
    TimeSlotResponse, TournamentBracketResponse, TournamentInvitationResponse,
    TournamentMapPoolResponse, TournamentMatchResponse, TournamentRegistrationResponse,
    TournamentResponse, TournamentStageResponse, TournamentSummaryResponse, UploadInfoResponse,
    UserLeagueMembershipResponse, UserResponse, UserRoleAssignmentResponse,
    ValidationResultResponse, VetoActionResponse, VetoActionResultResponse, VetoFormatResponse,
    VetoSessionResponse, VetoSessionStateResponse, WithdrawalResponse, WithdrawnEntryResponse,
    WorkshopMapDetailsResponse,
};
use crate::error::{ApiError, FieldErrorDto};
use crate::handlers::{
    admin, auth, availability, awards, bans, demos, dispute, evidence, forfeit, game_servers,
    games, league_teams, leagues, player_game_profiles, players, progression, pugs, result_reviews,
    results, roles, steam_auth, steam_tracking, tournaments, uploads, users, veto, veto_delegates,
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
        auth::logout,
        auth::logout_all,
        steam_auth::steam_login,
        steam_auth::steam_callback,
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
        games::get_workshop_map_details,
        games::remove_map,
        games::set_rank_tiers,
        games::update_team_size,
        // Leagues
        leagues::create_league,
        leagues::get_league,
        leagues::get_league_by_slug,
        leagues::list_leagues,
        leagues::admin_list_leagues,
        leagues::archive_league,
        leagues::restore_league,
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
        // P-65: the route (`routes/users.rs`) and the `#[utoipa::path]`
        // annotation both existed; only this line was missing, so the operation
        // never reached the spec and `captainActions.ts` had to cast the path to
        // `never` to call it. Same family as P-52.
        users::get_my_action_items,
        // League Seasons
        league_teams::season::create_season,
        league_teams::season::get_season,
        league_teams::season::list_seasons,
        league_teams::season::update_season,
        league_teams::season::archive_season,
        league_teams::season::restore_season,
        // League Teams (Persistent Identity)
        league_teams::team::create_team,
        league_teams::team::get_team,
        league_teams::team::list_teams_in_season,
        league_teams::team::archive_team,
        league_teams::team::move_team,
        league_teams::team::restore_team,
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
        tournaments::archive_tournament,
        tournaments::move_tournament,
        tournaments::restore_tournament,
        tournaments::complete_tournament,
        tournaments::finalize_tournament,
        tournaments::create_stage,
        tournaments::get_stages,
        tournaments::update_stage,
        tournaments::create_invitation,
        tournaments::list_invitations,
        tournaments::revoke_invitation,
        tournaments::register_team,
        tournaments::register_player,
        tournaments::get_registrations,
        tournaments::get_my_registrations,
        tournaments::get_registration_counts,
        tournaments::check_in,
        // Registration management
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
        tournaments::get_match_participants,
        // Match lifecycle
        tournaments::get_match_status,
        tournaments::get_match_status_history,
        tournaments::match_check_in,
        tournaments::declare_lineup,
        tournaments::get_match_lineups,
        tournaments::forfeit_match,
        tournaments::admin_match_transition,
        // Match scheduling (proposal workflow)
        tournaments::propose_schedule,
        tournaments::accept_schedule_proposal,
        tournaments::reject_schedule_proposal,
        tournaments::cancel_schedule_proposal,
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
        results::admin_override_match_result,
        results::admin_list_match_result_overrides,
        results::admin_list_entity_changes,
        // Veto (map pick/ban)
        veto::create_veto_session,
        veto::get_veto_session,
        veto::start_veto_session,
        veto::record_coin_flip,
        veto::perform_veto_action,
        veto::select_side,
        // Pick-up games (PUGs)
        pugs::create_pug,
        pugs::get_pug,
        pugs::my_pugs,
        pugs::open_pugs,
        pugs::recent_pugs,
        pugs::preview_by_code,
        pugs::join_by_code,
        pugs::rotate_code,
        pugs::leave_pug,
        pugs::kick_player,
        pugs::set_team,
        pugs::set_captain,
        pugs::shuffle_teams,
        pugs::swap_teams,
        pugs::draft_pick,
        pugs::nominate_map,
        pugs::lock_pug,
        pugs::spin_wheel,
        pugs::cancel_pug,
        pugs::rematch_pug,
        pugs::player_pug_stats,
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
        // Game servers (admin registry)
        game_servers::admin::list_game_servers,
        game_servers::admin::create_game_server,
        game_servers::admin::get_game_server,
        game_servers::admin::update_game_server,
        game_servers::admin::delete_game_server,
        game_servers::admin::mint_enrollment_token,
        game_servers::admin::revoke_agent,
        game_servers::admin::list_bookings,
        game_servers::admin::create_booking,
        game_servers::admin::delete_booking,
        game_servers::console::send_command,
        game_servers::console::get_console,
        game_servers::console::get_console_history,
        game_servers::console::change_map,
        game_servers::console::run_action,
        game_servers::match_server::get_match_server,
        game_servers::match_server::assign_match_server,
        game_servers::match_server::cancel_match_server,
        game_servers::substitutions::create_substitution,
        game_servers::substitutions::list_substitutions,
        game_servers::substitutions::cancel_substitution,
        game_servers::substitutions::approve_substitution,
        game_servers::substitutions::reject_substitution,
        game_servers::substitutions::substitution_options,
        game_servers::match_server::restore_match_server,
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
        demos::requeue_demo,
        demos::link_demo_to_match,
        demos::get_demo_status_counts,
        demos::get_demos_for_match,
        demos::unlink_demo_from_match,
        demos::batch_catalog_demos,
        demos::submit_demo_stats,
        demos::mark_demo_stats_failed,
        demos::delete_demo,
        demos::set_demo_notes,
        demos::process_unlinked_demos,
        demos::get_auto_link_setting,
        demos::update_auto_link_setting,
        // Ingestion pipeline operator reads (P-73) — admin-authenticated
        // equivalents of the X-API-Key /v1/internal reads.
        demos::get_pipeline_overview,
        demos::list_pipeline_tracking,
        demos::resume_pipeline_tracking,
        demos::list_pipeline_discovered_matches,
        demos::requeue_pipeline_discovered_matches,
        demos::requeue_pipeline_discovered_match,
        // Result reviews
        result_reviews::get_result_review,
        result_reviews::acknowledge_result_review,
        result_reviews::list_pending_reviews,
        result_reviews::get_result_review_by_id,
        result_reviews::approve_result_review,
        result_reviews::reject_result_review,
        // Awards + leaderboards
        awards::list_tournament_awards,
        awards::create_tournament_award,
        awards::update_tournament_award,
        awards::void_tournament_award,
        awards::get_tournament_award_standings,
        awards::finalize_tournament_award,
        awards::list_season_awards,
        awards::create_season_award,
        awards::update_season_award,
        awards::void_season_award,
        awards::get_season_award_standings,
        awards::finalize_season_award,
        awards::get_tournament_leaderboard,
        awards::get_season_leaderboard,
        awards::get_tournament_player_stats,
        awards::get_season_player_stats,
        awards::get_stat_catalog,
        awards::list_award_templates,
        awards::get_player_awards,
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
            WorkshopMapDetailsResponse,
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
            crate::dto::responses::ActionItemResponse,
            crate::dto::requests::tournament::MyMatchesQuery,

            // Auth
            RegisterRequest,
            RegisterResponse,
            LoginRequest,
            LoginResponse,
            RefreshTokenRequest,
            LogoutResponse,

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
            portal_domain::entities::ban::BanType,
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
            MovedTeamResponse,
            WithdrawnEntryResponse,
            LeagueTeamSeasonResponse,
            LeagueTeamWithSeasonResponse,
            LeagueTeamSummaryResponse,
            CreateLeagueTeamRequest,
            UpdateLeagueTeamRequest,
            RegisterTeamForSeasonRequest,
            TransferOwnershipRequest,
            MoveTeamRequest,
            MoveTournamentRequest,

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
            crate::dto::requests::eligibility::EligibilityRestrictionsInput,
            TournamentStageResponse,
            TournamentBracketResponse,
            TournamentRegistrationResponse,
            MyTournamentRegistrationsResponse,
            crate::dto::responses::TournamentRegistrationCountsResponse,
            MatchParticipantsResponse,
            MatchResultOverrideResponse,
            AdminOverrideMatchResultRequest,
            TournamentInvitationResponse,
            portal_core::types::TournamentInvitationStatus,
            crate::dto::requests::CreateTournamentInvitationRequest,
            TournamentMatchResponse,
            SeededParticipantResponse,
            crate::dto::responses::TournamentStandingResponse,
            CheckInStatusResponse,
            TournamentMapPoolResponse,
            SetTournamentMapPoolRequest,
            CreateTournamentRequest,
            UpdateTournamentRequest,
            CreateTournamentStageRequest,
            UpdateTournamentStageRequest,
            RegisterTeamRequest,
            RegisterPlayerRequest,
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
            DeclareLineupRequest,
            MatchLineupResponse,
            MatchLineupPlayerResponse,
            portal_core::types::LineupStatus,
            portal_core::types::LineupSource,
            portal_core::types::ParticipationStatus,
            AdminMatchTransitionRequest,
            ForfeitMatchRequest,
            // Match scheduling
            ScheduleProposalResponse,
            ProposeScheduleRequest,
            AcceptScheduleProposalRequest,
            RejectScheduleProposalRequest,
            CancelScheduleProposalRequest,
            portal_core::types::TournamentMatchStatus,
            portal_core::types::TournamentStatus,
            portal_core::types::TournamentRegistrationStatus,
            portal_core::types::StageStatus,
            // P-112: every entry below this comment used to be stringified by the
            // DTO in front of an enum that already derived `Serialize` +
            // `ToSchema`, so no union reached the generated client and the
            // matching `statusMaps.ts` entry could not be compile-locked.
            portal_core::types::StageFormat,
            portal_core::types::BracketStatus,
            portal_core::types::ProposalStatus,
            portal_core::types::SeasonStatus,
            portal_core::types::RosterLockStatus,
            portal_core::types::LeagueTeamStatus,
            portal_core::types::LeagueTeamSeasonStatus,
            portal_core::types::LeagueTeamMemberStatus,
            portal_core::types::LeagueTeamRole,
            portal_core::types::LeagueTeamInvitationStatus,
            portal_core::types::DemoStatus,
            portal_core::types::DemoCategory,
            portal_domain::entities::result_claim::ClaimStatus,
            portal_domain::entities::result_review::ResultReviewStatus,
            portal_domain::entities::evidence::EvidenceStatus,
            portal_core::types::EvidenceType,
            portal_domain::entities::league::LeagueStatus,
            portal_domain::entities::dispute::DisputeStatus,
            portal_domain::entities::dispute::DisputePriority,
            portal_domain::entities::dispute::DisputeReason,
            portal_domain::entities::audit::ChangeType,
            EntityChangeResponse,
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
            // Pick-up games (PUGs)
            crate::handlers::pugs::CreatePugRequest,
            crate::handlers::pugs::SetTeamRequest,
            crate::handlers::pugs::SetCaptainRequest,
            crate::handlers::pugs::KickPlayerRequest,
            crate::handlers::pugs::WheelEntryRequest,
            crate::handlers::pugs::LockPugRequest,
            crate::handlers::pugs::DraftPickRequest,
            crate::handlers::pugs::DraftPickResponse,
            crate::handlers::pugs::PugResponse,
            crate::handlers::pugs::PugPlayerResponse,
            crate::handlers::pugs::PugWheelEntryResponse,
            crate::handlers::pugs::PugWheelSpinResponse,
            crate::handlers::pugs::PugDetailResponse,
            crate::handlers::pugs::PugPreviewResponse,
            crate::handlers::pugs::SpinResponse,
            crate::handlers::pugs::JoinCodeResponse,
            crate::handlers::pugs::PugStatsResponse,
            portal_core::types::PugStatus,
            portal_core::types::PugMapSelectionMode,
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
            ProcessUnlinkedDemosResponse,
            UpdateAutoLinkSettingRequest,
            AutoLinkSettingResponse,
            // Ingestion pipeline (P-73)
            PipelineOverviewResponse,
            TrackingHealthSummaryResponse,
            TrackingHealthEntryResponse,
            DiscoveredMatchQueueResponse,
            DemoExtractionQueueResponse,
            DiscoveredMatchAdminResponse,
            RequeueDiscoveredMatchesRequest,
            RequeueDiscoveredMatchesResponse,
            // Game servers
            crate::handlers::game_servers::admin::CreateGameServerRequest,
            crate::handlers::game_servers::admin::UpdateGameServerRequest,
            crate::handlers::game_servers::admin::GameServerResponse,
            crate::handlers::game_servers::admin::EnrollmentTokenResponse,
            crate::handlers::game_servers::admin::RevokeAgentResponse,
            crate::handlers::game_servers::admin::CreateServerBookingRequest,
            crate::handlers::game_servers::admin::ServerBookingResponse,
            crate::handlers::game_servers::console::SendCommandRequest,
            crate::handlers::game_servers::console::SendCommandResponse,
            crate::handlers::game_servers::console::ConsoleSnapshotResponse,
            crate::handlers::game_servers::console::ConsoleAgentInfo,
            crate::handlers::game_servers::console::ConsoleStatus,
            crate::handlers::game_servers::console::ConsolePlayer,
            crate::handlers::game_servers::console::ConsolePlayerLink,
            crate::handlers::game_servers::console::ConsoleReservation,
            crate::handlers::game_servers::console::ConsoleHold,
            crate::handlers::game_servers::console::AdminServerCommandResponse,
            crate::handlers::game_servers::console::MapChangeRequest,
            crate::handlers::game_servers::console::MapChangeResponse,
            crate::handlers::game_servers::console::MapTarget,
            crate::handlers::game_servers::console::ConsoleAction,
            crate::handlers::game_servers::console::ConsoleActionArgs,
            crate::handlers::game_servers::console::ConsoleActionRequest,
            crate::handlers::game_servers::console::ConsoleActionResponse,
            portal_core::types::ReservationKind,
            portal_core::types::GameServerStatus,
            portal_core::types::ReservationStatus,
            crate::handlers::game_servers::match_server::MatchServerResponse,
            crate::handlers::game_servers::match_server::LiveScoreResponse,
            crate::handlers::game_servers::substitutions::CreateSubstitutionRequest,
            crate::handlers::game_servers::substitutions::SubstitutionResponse,
            crate::handlers::game_servers::substitutions::SubstitutionOptionsSide,
            crate::handlers::game_servers::substitutions::SubstitutionPlayerOption,
            portal_core::types::SubstitutionStatus,
            crate::handlers::game_servers::match_server::RestoreBackupRequest,
            crate::handlers::game_servers::match_server::RestoreBackupResponse,
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
            // Awards + leaderboards
            crate::dto::requests::CreateAwardRequest,
            crate::dto::requests::UpdateAwardRequest,
            crate::dto::requests::LeaderboardQueryParams,
            crate::dto::requests::PlayerStatsQueryParams,
            crate::dto::requests::StandingsQueryParams,
            crate::dto::responses::AwardResponse,
            crate::dto::responses::AwardTemplateResponse,
            crate::dto::responses::AwardResultResponse,
            crate::dto::responses::AwardStandingsResponse,
            crate::dto::responses::FinalizedAwardResponse,
            crate::dto::responses::LeaderboardEntryResponse,
            crate::dto::responses::PlayerStatsEntryResponse,
            crate::dto::responses::PlayerTrophyResponse,
            crate::dto::responses::StatCatalogEntryResponse,
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
        (name = "steam_tracking", description = "CS2 Steam match tracking registration"),
        (name = "game_servers", description = "Game server registry and agent management"),
        (name = "pugs", description = "Pick-up games: ephemeral lobbies, wheel map picks, separate stats"),
        (name = "awards", description = "Tournament/season awards, stat leaderboards, and trophy cases")
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

#[cfg(test)]
mod operation_id_uniqueness {
    use super::*;
    use std::collections::HashMap;
    use utoipa::OpenApi;

    /// Every `operationId` in the served spec must be unique.
    ///
    /// utoipa defaults `operationId` to the handler's function name, so two
    /// handlers with the same name in different modules collide silently — the
    /// spec still serves, but `openapi-typescript` emits a client that does not
    /// compile (`TS2300: Duplicate identifier`). That is exactly what happened
    /// with `leagues::list_invitations` vs `tournaments::list_invitations`: the
    /// tournament invitation endpoints could not be typed at all until one was
    /// renamed with an explicit `operation_id`. A spec that serves fine but
    /// breaks every generated client is the kind of defect nothing else here
    /// would catch.
    #[test]
    fn every_operation_id_is_unique() {
        let doc = ApiDoc::openapi();
        let mut seen: HashMap<String, Vec<String>> = HashMap::new();

        for (path, item) in doc.paths.paths {
            let ops = [
                ("GET", item.get),
                ("PUT", item.put),
                ("POST", item.post),
                ("DELETE", item.delete),
                ("PATCH", item.patch),
                ("HEAD", item.head),
                ("OPTIONS", item.options),
                ("TRACE", item.trace),
            ];
            for (method, op) in ops {
                if let Some(id) = op.and_then(|o| o.operation_id) {
                    seen.entry(id).or_default().push(format!("{method} {path}"));
                }
            }
        }

        let dupes: Vec<_> = seen.iter().filter(|(_, v)| v.len() > 1).collect();
        assert!(
            dupes.is_empty(),
            "duplicate operationId(s) would produce an uncompilable generated client: {dupes:#?}\n\
             Fix by adding `operation_id = \"...\"` to the #[utoipa::path] of one handler."
        );
    }
}

#[cfg(test)]
mod enum_fields_are_not_stringified {
    use super::*;
    use utoipa::OpenApi;

    /// Response fields that MUST publish an enum union, not a bare `string`:
    /// `(schema, field, referenced enum)`.
    const TYPED: &[(&str, &str, &str)] = &[
        ("DisputeResponse", "priority", "DisputePriority"),
        ("DisputeResponse", "reason", "DisputeReason"),
        ("DemoResponse", "category", "DemoCategory"),
        ("BanResponse", "ban_type", "BanType"),
        ("TournamentStageResponse", "format", "StageFormat"),
        ("LeagueTeamMemberResponse", "role", "LeagueTeamRole"),
        (
            "LeagueTeamMemberWithPlayerResponse",
            "role",
            "LeagueTeamRole",
        ),
        (
            "PlayerLeagueTeamMembershipResponse",
            "role",
            "LeagueTeamRole",
        ),
        ("LeagueTeamInvitationResponse", "role", "LeagueTeamRole"),
        (
            "LeagueTeamInvitationWithTeamResponse",
            "role",
            "LeagueTeamRole",
        ),
        // P-175/P-178 follow-through: the two fields typed so
        // `evidenceTypeMap` and `leagueStatusMap` could be keyed.
        ("EvidenceResponse", "evidence_type", "EvidenceType"),
        ("EvidenceSummaryResponse", "evidence_type", "EvidenceType"),
        (
            "DiscoveredEvidenceResponse",
            "evidence_type",
            "EvidenceType",
        ),
        ("LeagueResponse", "status", "LeagueStatus"),
    ];

    /// P-112: a DTO field typed `String` in front of an enum that already derives
    /// `Serialize` + `ToSchema` throws the union away, and the frontend's status
    /// map for that field then cannot be keyed — so it becomes the only kind of
    /// map that can still drift. That is precisely where P-79 (`critical` vs
    /// `urgent`) and P-91 happened.
    ///
    /// This asserts the shape of the PUBLISHED SCHEMA rather than the Rust type,
    /// because the published schema is what the generated client is built from: a
    /// `#[schema(value_type = String)]`, a revert to `.to_string()`, or dropping
    /// the enum from `components(schemas)` all regress the client while the DTO
    /// still looks enum-typed at a glance.
    #[test]
    fn declared_enums_reach_the_client_as_unions() {
        let doc = serde_json::to_value(ApiDoc::openapi()).expect("serialise spec");
        let schemas = &doc["components"]["schemas"];

        let mut problems = Vec::new();
        for (schema, field, want) in TYPED {
            let prop = &schemas[schema]["properties"][field];
            if prop.is_null() {
                problems.push(format!("{schema}.{field} is absent from the spec"));
                continue;
            }
            // utoipa emits either a bare `$ref` or `allOf: [{$ref}]` for a
            // referenced schema; both become a union in the generated client.
            let reference = prop["$ref"]
                .as_str()
                .or_else(|| prop["allOf"][0]["$ref"].as_str());
            match reference {
                Some(r) if r.ends_with(&format!("/{want}")) => {}
                Some(r) => problems.push(format!("{schema}.{field} references {r}, want {want}")),
                None => problems.push(format!(
                    "{schema}.{field} publishes `{}` instead of a $ref to {want} — the enum \
                     exists and the DTO is throwing it away (P-112)",
                    prop["type"].as_str().unwrap_or("<no type>")
                )),
            }
            if !schemas[want]["enum"].is_array() {
                problems.push(format!(
                    "{want} is not registered in components(schemas), so no union is generated"
                ));
            }
        }

        assert!(problems.is_empty(), "P-112 regressions: {problems:#?}");
    }
}

#[cfg(test)]
mod every_route_is_documented {
    use std::collections::BTreeSet;

    /// Handler modules whose routes are deliberately absent from the public spec.
    ///
    /// `internal.rs` is the API-key surface the demo scanner and enricher call;
    /// it is not part of the client contract and publishing it would put
    /// service-to-service endpoints in the generated browser client.
    /// `veto_ws::ws_upgrade` is a WebSocket upgrade, not an HTTP operation —
    /// OpenAPI has no way to describe it.
    // `agent` and `matchzy` are game-server plumbing (token/X-API-Key
    // authenticated service-to-service routes from the MatchZy integration),
    // deliberately absent from the public spec for the same reason as
    // `internal` — no browser client should ever call them in a typed way.
    // pug_ws: websocket upgrade like veto_ws — no request/response schema to type.
    const EXEMPT: &[&str] = &["internal", "veto_ws", "pug_ws", "agent", "matchzy"];

    /// Every handler the router serves must appear in `paths(...)`.
    ///
    /// P-65: `GET /v1/users/me/action-items` had a live route AND a complete
    /// `#[utoipa::path]` annotation, and was still invisible to every client for
    /// months, because the one-line registration in `paths(...)` was never added.
    /// Nothing failed: the endpoint worked when called by hand, the annotation
    /// looked like proof it was documented, and the frontend papered over the
    /// missing type with `api.GET('…' as never)` — a cast that then silently
    /// disabled type checking on the whole call. That is the same shape as P-52
    /// (a spec that serves fine but breaks the generated client), and this is the
    /// guard that makes the class self-detecting: registration is now something
    /// you cannot forget, only deliberately exempt.
    ///
    /// Matching is by handler FUNCTION NAME, not by the full path, because
    /// `routes/*.rs` reaches handlers through re-exports (`league_teams::get_team`)
    /// while `paths(...)` names them fully (`league_teams::team::get_team`).
    #[test]
    fn every_router_handler_appears_in_paths() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

        // Handler idents in route position: `get(users::get_my_action_items)`.
        let mut routed: BTreeSet<(String, String)> = BTreeSet::new();
        for entry in std::fs::read_dir(manifest.join("src/routes")).expect("routes dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("read route file");
            for (idx, _) in src.match_indices("::") {
                // Walk back over `a::b::c` and forward to the closing token, then
                // require the whole thing to sit inside a method-router call.
                let head =
                    src[..idx].rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'));
                let start = head.map_or(0, |p| p + 1);
                let rest = &src[idx + 2..];
                let end = rest
                    .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                    .unwrap_or(rest.len());
                let full = &src[start..idx + 2 + end];
                let prefix = src[..start].trim_end();
                let is_route_position = ["get(", "post(", "put(", "patch(", "delete("]
                    .iter()
                    .any(|m| prefix.ends_with(m));
                if !is_route_position || !full.contains("::") {
                    continue;
                }
                let module = full.split("::").next().unwrap_or_default().to_string();
                let func = full.rsplit("::").next().unwrap_or_default().to_string();
                if EXEMPT.contains(&module.as_str()) {
                    continue;
                }
                routed.insert((func, full.to_string()));
            }
        }
        assert!(
            routed.len() > 200,
            "route extraction found only {} handlers — the parser has stopped matching, \
             which would make this guard vacuous",
            routed.len()
        );

        // Handler idents inside the `paths(...)` block of the #[openapi] attribute.
        let doc =
            std::fs::read_to_string(manifest.join("src/openapi.rs")).expect("read openapi.rs");
        let block_start = doc.find("    paths(").expect("paths( block");
        let block_end = doc[block_start..]
            .find("    components(")
            .expect("components( block")
            + block_start;
        let documented: BTreeSet<&str> = doc[block_start..block_end]
            .lines()
            .map(|l| l.trim().trim_end_matches(','))
            .filter(|l| !l.starts_with("//") && l.contains("::"))
            .filter_map(|l| l.rsplit("::").next())
            .collect();

        let missing: Vec<&String> = routed
            .iter()
            .filter(|(func, _)| !documented.contains(func.as_str()))
            .map(|(_, full)| full)
            .collect();

        assert!(
            missing.is_empty(),
            "these handlers are routed but missing from `paths(...)` in openapi.rs, so no \
             client can call them in a typed way (P-65): {missing:#?}\n\
             Add each to the `paths(...)` list, or add its module to EXEMPT with a reason."
        );
    }
}
