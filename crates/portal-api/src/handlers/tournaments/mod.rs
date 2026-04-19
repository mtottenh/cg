//! Tournament handlers.
//!
//! This module is in the middle of a split — see audit item N1. The
//! `map_pool` subsystem has already been extracted; the remaining ~2200
//! lines below are candidates for further extraction:
//!
//! * **`lifecycle`** — create / get / list / update / publish / start /
//!   complete / cancel / finalize, plus open/close/reopen registration.
//! * **`stages`** — create_stage, get_stages.
//! * **`registration`** — register_team / register_player /
//!   get_registrations / check_in / withdraw / approve / reject /
//!   disqualify / admin_check_in / process_no_shows.
//! * **`brackets`** — get_brackets / get_matches / get_match.
//! * **`seeding`** — get_seeding / auto_seed / manual_seed /
//!   clear_seeding.
//! * **`match_lifecycle`** — match status / check-in / schedule /
//!   forfeit / admin_match_transition.
//! * **`scheduling`** — propose / accept / reject / counter_propose /
//!   get_active_proposal / get_proposal_history / admin_schedule_match,
//!   plus the standings + swiss helpers at the bottom.
//!
//! Because `openapi.rs` references every handler as
//! `tournaments::handler_name`, each future sub-module must be
//! re-exported via `pub use` here so those paths keep working.

pub mod map_pool;
// Glob re-export so `openapi.rs` can keep referencing
// `tournaments::get_tournament_map_pool` (etc.) *and* so the
// `__path_<handler>` types the `utoipa::path` macro generates are
// visible at the same `tournaments::` path — which is what utoipa's
// `paths(...)` list actually resolves against.
pub use map_pool::*;

use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    AcceptScheduleProposalRequest, AdminMatchTransitionRequest, AdminScheduleRequest,
    AutoSeedRequest, CounterProposeRequest, CreateTournamentRequest, CreateTournamentStageRequest,
    DisqualifyRequest, ForfeitMatchRequest, ListTournamentsQuery, ManualSeedRequest,
    MatchCheckInRequest, ProposeScheduleRequest, RegisterPlayerRequest, RegisterTeamRequest,
    RejectRegistrationRequest, RejectScheduleProposalRequest, ScheduleMatchRequest,
    UpdateTournamentRequest,
};
use crate::dto::responses::{
    CheckInStatusResponse, MatchStatusDetailsResponse, MatchStatusLogResponse,
    ScheduleProposalResponse, SeededParticipantResponse, TournamentBracketResponse,
    TournamentMatchResponse, TournamentRegistrationResponse,
    TournamentResponse, TournamentStageResponse, TournamentStandingResponse,
    TournamentSummaryResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::types::{MatchFormat, TournamentMatchStatus, TournamentStatus};
use portal_core::{PlayerId, ScheduleProposalId, TournamentId, TournamentMatchId, TournamentRegistrationId};
use portal_core::VetoFormatConfig;
use portal_domain::entities::schedule_proposal::{
    AcceptProposalCommand, CounterProposeCommand, RejectProposalCommand,
};
use portal_domain::repositories::tournament::{
    TournamentFilters, TournamentMapPoolRepository,
};

/// Extract request ID from headers.
///
/// `pub(super)` so the extracted sub-modules (currently just `map_pool`)
/// can reuse the same helper without duplicating it.
pub(super) fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Check eligibility restrictions for a set of player IDs against a tournament.
///
/// Delegates to the `EligibilityService` which fetches each player's game
/// profile and rating stats for the tournament's game, then runs the checker.
async fn check_eligibility_for_players(
    state: &AppState,
    tournament: &portal_domain::entities::Tournament,
    player_ids: &[PlayerId],
) -> Result<(), ApiError> {
    let restrictions = tournament.eligibility_restrictions();
    let violations = state
        .eligibility_service
        .check_players(&restrictions, tournament.game_id, player_ids)
        .await?;

    if violations.is_empty() {
        return Ok(());
    }

    let messages: Vec<String> = violations
        .iter()
        .map(|v| {
            if v.player_id == PlayerId::from_uuid(uuid::Uuid::nil()) {
                format!("[{}] {}", v.restriction, v.message)
            } else {
                format!("[{}] Player {}: {}", v.restriction, v.player_id, v.message)
            }
        })
        .collect();
    Err(ApiError::bad_request(format!(
        "Eligibility check failed: {}",
        messages.join("; ")
    )))
}

// =============================================================================
// TOURNAMENT CRUD
// =============================================================================

/// Create a new tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments",
    request_body = CreateTournamentRequest,
    responses(
        (status = 201, description = "Tournament created", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Tournament slug already taken", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn create_tournament(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<CreateTournamentRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentResponse>>)> {
    let request_id = get_request_id(&headers);

    let mut cmd = req.into_command()?;

    // Default season_id to the league's current season when not specified
    if cmd.league_id.is_some() && cmd.season_id.is_none() {
        if let Some(league_id) = cmd.league_id {
            if let Ok(league) = state.league_service.get_league(league_id).await {
                cmd.season_id = league.current_season_id;
            }
        }
    }

    let tournament = state
        .tournament_service
        .create_tournament(cmd, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TournamentResponse::from(tournament),
            request_id,
        )),
    ))
}

/// Get a tournament by ID.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Tournament found", body = DataResponse<TournamentResponse>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_tournament(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state.tournament_service.get_tournament(tournament_id).await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Get a tournament by slug.
#[utoipa::path(
    get,
    path = "/v1/tournaments/by-slug/{slug}",
    params(
        ("slug" = String, Path, description = "Tournament slug")
    ),
    responses(
        (status = 200, description = "Tournament found", body = DataResponse<TournamentResponse>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_tournament_by_slug(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state.tournament_service.get_tournament_by_slug(&slug).await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// List tournaments with filters.
#[utoipa::path(
    get,
    path = "/v1/tournaments",
    params(
        ("game_id" = Option<String>, Query, description = "Filter by game ID"),
        ("league_id" = Option<String>, Query, description = "Filter by league ID"),
        ("season_id" = Option<String>, Query, description = "Filter by season ID"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("format" = Option<String>, Query, description = "Filter by format"),
        ("search" = Option<String>, Query, description = "Search by name"),
        ("page" = Option<u32>, Query, description = "Page number"),
        ("per_page" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of tournaments", body = PaginatedResponse<TournamentSummaryResponse>),
    ),
    tag = "tournaments"
)]
pub async fn list_tournaments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ListTournamentsQuery>,
    Query(pagination): Query<PaginationParams>,
) -> ApiResult<Json<PaginatedResponse<TournamentSummaryResponse>>> {
    let request_id = get_request_id(&headers);

    // Parse filter IDs
    let game_id = params
        .game_id
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid game ID format"))
        })
        .transpose()?;

    let league_id = params
        .league_id
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid league ID format"))
        })
        .transpose()?;

    let season_id = params
        .season_id
        .map(|id| {
            id.parse()
                .map_err(|_| ApiError::bad_request("Invalid season ID format"))
        })
        .transpose()?;

    let status = params
        .status
        .map(|s| {
            s.parse()
                .map_err(|_| ApiError::bad_request("Invalid tournament status"))
        })
        .transpose()?;

    let format = params
        .format
        .map(|f| {
            f.parse()
                .map_err(|_| ApiError::bad_request("Invalid tournament format"))
        })
        .transpose()?;

    let filters = TournamentFilters {
        game_id,
        league_id,
        season_id,
        status,
        format,
        participant_type: None,
        search: params.search,
        upcoming: None,
        active: None,
    };

    let (tournaments, total) = state
        .tournament_service
        .list_tournaments(filters, pagination.limit(), pagination.offset())
        .await?;

    let data: Vec<TournamentSummaryResponse> =
        tournaments.into_iter().map(Into::into).collect();

    Ok(Json(PaginatedResponse::new(
        data,
        &pagination,
        total as u64,
        request_id,
    )))
}

/// Update a tournament.
#[utoipa::path(
    patch,
    path = "/v1/tournaments/{tournament_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = UpdateTournamentRequest,
    responses(
        (status = 200, description = "Tournament updated", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Validation error or tournament already started", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn update_tournament(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<UpdateTournamentRequest>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    // Guard: eligibility restrictions cannot be changed once registration has opened
    let wants_eligibility_change = req.eligibility_restrictions.is_some()
        || req
            .settings
            .as_ref()
            .and_then(|s| s.get("eligibility"))
            .is_some();

    if wants_eligibility_change {
        let current = state
            .tournament_service
            .get_tournament(tournament_id)
            .await?;
        if current.status != TournamentStatus::Draft
            && current.status != TournamentStatus::Published
        {
            return Err(ApiError::bad_request(
                "Eligibility restrictions cannot be changed after registration has opened",
            ));
        }
    }

    let cmd = req.try_into()?;

    let tournament = state
        .tournament_service
        .update_tournament(tournament_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Publish a tournament (make visible for registration).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/publish",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Tournament published", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Tournament cannot be published", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn publish_tournament(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state.tournament_service.publish_tournament(tournament_id).await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Open registration for a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/open-registration",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Registration opened", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Cannot open registration", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn open_registration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state
        .tournament_service
        .open_registration(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Start a tournament (generate brackets).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/start",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Tournament started", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Tournament cannot be started", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn start_tournament(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state.tournament_service.start_tournament(tournament_id).await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Close registration for a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/close-registration",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Registration closed", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Cannot close registration", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn close_registration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state
        .tournament_service
        .close_registration(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Reopen registration for a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/reopen-registration",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Registration reopened", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Cannot reopen registration", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn reopen_registration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state
        .tournament_service
        .reopen_registration(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Cancel a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/cancel",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Tournament cancelled", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Cannot cancel tournament", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn cancel_tournament(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state
        .tournament_service
        .cancel_tournament(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Complete a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/complete",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Tournament completed", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Cannot complete tournament", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn complete_tournament(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state
        .tournament_service
        .complete_tournament(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

/// Finalize a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/finalize",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Tournament finalized", body = DataResponse<TournamentResponse>),
        (status = 400, description = "Cannot finalize tournament", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn finalize_tournament(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state
        .tournament_service
        .finalize_tournament(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentResponse::from(tournament),
        request_id,
    )))
}

// =============================================================================
// TOURNAMENT STAGES
// =============================================================================

/// Create a tournament stage.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/stages",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = CreateTournamentStageRequest,
    responses(
        (status = 201, description = "Stage created", body = DataResponse<TournamentStageResponse>),
        (status = 400, description = "Validation error or tournament started", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn create_stage(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<CreateTournamentStageRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentStageResponse>>)> {
    let request_id = get_request_id(&headers);

    let cmd = req.into_command(tournament_id)?;

    let stage = state
        .tournament_service
        .create_stage(
            tournament_id,
            cmd.name,
            cmd.stage_order,
            cmd.format,
            cmd.format_settings,
            cmd.advancement_count,
            cmd.match_format,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(TournamentStageResponse::from(stage), request_id)),
    ))
}

/// Get stages for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/stages",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "List of stages", body = DataResponse<Vec<TournamentStageResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_stages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentStageResponse>>>> {
    let request_id = get_request_id(&headers);

    let stages = state.tournament_service.get_stages(tournament_id).await?;

    let data: Vec<TournamentStageResponse> = stages.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

// =============================================================================
// TOURNAMENT REGISTRATION
// =============================================================================

/// Register a team for a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/team",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = RegisterTeamRequest,
    responses(
        (status = 201, description = "Team registered", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Registration closed or validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
        (status = 409, description = "Already registered", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn register_team(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<RegisterTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentRegistrationResponse>>)> {
    let request_id = get_request_id(&headers);

    let team_season_id = req.parse_team_season_id()?;

    // Eligibility check: fetch tournament and team members, run restrictions
    let tournament = state.tournament_service.get_tournament(tournament_id).await?;
    let members = state.league_team_service.get_members(team_season_id).await?;
    let player_ids: Vec<PlayerId> = members.iter().map(|m| m.player_id).collect();
    check_eligibility_for_players(&state, &tournament, &player_ids).await?;

    let registration = state
        .tournament_service
        .register_team(
            tournament_id,
            team_season_id,
            req.participant_name,
            req.participant_logo_url,
            auth.user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TournamentRegistrationResponse::from(registration),
            request_id,
        )),
    ))
}

/// Register a player for an individual tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/player",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = RegisterPlayerRequest,
    responses(
        (status = 201, description = "Player registered", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Registration closed or validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
        (status = 409, description = "Already registered", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn register_player(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<RegisterPlayerRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentRegistrationResponse>>)> {
    let request_id = get_request_id(&headers);

    let player_id = auth.player_id;

    // Eligibility check: fetch tournament and run restrictions for this player
    let tournament = state.tournament_service.get_tournament(tournament_id).await?;
    check_eligibility_for_players(&state, &tournament, &[player_id]).await?;

    let registration = state
        .tournament_service
        .register_player(tournament_id, player_id, req.participant_name, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TournamentRegistrationResponse::from(registration),
            request_id,
        )),
    ))
}

/// Get registrations for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/registrations",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("status" = Option<String>, Query, description = "Filter by registration status"),
        ("page" = Option<u32>, Query, description = "Page number"),
        ("per_page" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of registrations", body = PaginatedResponse<TournamentRegistrationResponse>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_registrations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    Query(status_filter): Query<RegistrationStatusQuery>,
    Query(pagination): Query<PaginationParams>,
) -> ApiResult<Json<PaginatedResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let status = status_filter
        .status
        .map(|s| {
            s.parse()
                .map_err(|_| ApiError::bad_request("Invalid registration status"))
        })
        .transpose()?;

    let (registrations, total) = state
        .tournament_service
        .get_registrations(tournament_id, status, pagination.limit(), pagination.offset())
        .await?;

    let data: Vec<TournamentRegistrationResponse> =
        registrations.into_iter().map(Into::into).collect();

    Ok(Json(PaginatedResponse::new(
        data,
        &pagination,
        total as u64,
        request_id,
    )))
}

/// Path parameters for check-in.
#[derive(Debug, serde::Deserialize)]
pub struct CheckInPath {
    #[allow(dead_code)]
    tournament_id: String,
    registration_id: String,
}

/// Check in for a tournament.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/check-in",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Checked in", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Check-in not open or already checked in", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn check_in(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<CheckInPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .tournament_service
        .check_in(registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

// =============================================================================
// TOURNAMENT BRACKETS & MATCHES
// =============================================================================

/// Get brackets for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/brackets",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "List of brackets", body = DataResponse<Vec<TournamentBracketResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_brackets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentBracketResponse>>>> {
    let request_id = get_request_id(&headers);

    let brackets = state.tournament_service.get_bracket(tournament_id).await?;

    let data: Vec<TournamentBracketResponse> = brackets.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Get matches for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "List of matches", body = DataResponse<Vec<TournamentMatchResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_matches(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentMatchResponse>>>> {
    let request_id = get_request_id(&headers);

    let matches = state
        .tournament_service
        .get_tournament_matches(tournament_id)
        .await?;

    let data: Vec<TournamentMatchResponse> = matches.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Get a single match by ID.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID"),
    ),
    responses(
        (status = 200, description = "Match details", body = DataResponse<TournamentMatchResponse>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_match(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tournament_id, match_id)): Path<(TournamentId, TournamentMatchId)>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_ = state
        .tournament_service
        .get_tournament_match(tournament_id, match_id)
        .await?;

    Ok(Json(DataResponse::new(match_.into(), request_id)))
}

// =============================================================================
// REGISTRATION MANAGEMENT
// =============================================================================

/// Path parameters for registration operations.
#[derive(Debug, serde::Deserialize)]
pub struct RegistrationPath {
    #[allow(dead_code)]
    tournament_id: String,
    registration_id: String,
}

/// Withdraw from a tournament.
#[utoipa::path(
    delete,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Withdrawn successfully", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot withdraw", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .withdraw(registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Approve a pending registration (admin only).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Registration approved", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot approve", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn approve_registration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .approve_registration(registration_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Reject a pending registration (admin only).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/reject",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    request_body = RejectRegistrationRequest,
    responses(
        (status = 200, description = "Registration rejected", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot reject", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn reject_registration(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::RejectRegistrationRequest>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .reject_registration(registration_id, req.reason)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Disqualify a participant (admin only).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/disqualify",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    request_body = DisqualifyRequest,
    responses(
        (status = 200, description = "Participant disqualified", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot disqualify", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn disqualify(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::DisqualifyRequest>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .registration_service
        .disqualify(registration_id, req.reason)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Get check-in status for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/check-in-status",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Check-in status", body = DataResponse<CheckInStatusResponse>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_check_in_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<crate::dto::responses::CheckInStatusResponse>>> {
    let request_id = get_request_id(&headers);

    let status = state
        .checkin_service
        .get_check_in_status(tournament_id)
        .await?;

    Ok(Json(DataResponse::new(
        crate::dto::responses::CheckInStatusResponse {
            tournament_id: status.tournament_id.to_string(),
            check_in_required: status.check_in_required,
            check_in_open: status.check_in_open,
            checked_in_count: status.checked_in_count,
            total_eligible: status.total_eligible,
        },
        request_id,
    )))
}

/// Admin check-in a participant (bypasses check-in window).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/registrations/{registration_id}/admin-check-in",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("registration_id" = String, Path, description = "Registration ID"),
    ),
    responses(
        (status = 200, description = "Participant checked in", body = DataResponse<TournamentRegistrationResponse>),
        (status = 400, description = "Cannot check in", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn admin_check_in(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(path): Path<RegistrationPath>,
) -> ApiResult<Json<DataResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let registration_id: portal_core::TournamentRegistrationId = path
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let registration = state
        .checkin_service
        .admin_check_in(registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentRegistrationResponse::from(registration),
        request_id,
    )))
}

/// Process no-shows (mark unchecked-in participants as no-show).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/process-no-shows",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "No-shows processed", body = DataResponse<Vec<TournamentRegistrationResponse>>),
        (status = 400, description = "Cannot process no-shows", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn process_no_shows(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentRegistrationResponse>>>> {
    let request_id = get_request_id(&headers);

    let no_shows = state
        .checkin_service
        .process_no_shows(tournament_id)
        .await?;

    let data: Vec<TournamentRegistrationResponse> =
        no_shows.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

// =============================================================================
// SEEDING
// =============================================================================

/// Get current seeding for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/seeding",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Current seeding", body = DataResponse<Vec<SeededParticipantResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_seeding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<crate::dto::responses::SeededParticipantResponse>>>> {
    let request_id = get_request_id(&headers);

    let seeded = state
        .seeding_service
        .get_current_seeding(tournament_id)
        .await?;

    let data: Vec<crate::dto::responses::SeededParticipantResponse> = seeded
        .into_iter()
        .map(|p| crate::dto::responses::SeededParticipantResponse {
            registration_id: p.registration_id.to_string(),
            participant_name: p.participant_name,
            seed: p.seed,
            seed_rating: p.seed_rating,
        })
        .collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Auto-seed participants using the specified algorithm.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/seeding/auto",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = AutoSeedRequest,
    responses(
        (status = 200, description = "Seeding complete", body = DataResponse<Vec<SeededParticipantResponse>>),
        (status = 400, description = "Invalid algorithm or tournament state", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn auto_seed(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    Json(req): Json<crate::dto::requests::AutoSeedRequest>,
) -> ApiResult<Json<DataResponse<Vec<crate::dto::responses::SeededParticipantResponse>>>> {
    let request_id = get_request_id(&headers);

    let algorithm: portal_core::types::SeedingAlgorithm = req
        .algorithm
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid seeding algorithm"))?;

    let seeded = state
        .seeding_service
        .auto_seed(tournament_id, algorithm)
        .await?;

    let data: Vec<crate::dto::responses::SeededParticipantResponse> = seeded
        .into_iter()
        .map(|p| crate::dto::responses::SeededParticipantResponse {
            registration_id: p.registration_id.to_string(),
            participant_name: p.participant_name,
            seed: p.seed,
            seed_rating: p.seed_rating,
        })
        .collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Manually set seeds for participants.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/seeding/manual",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = ManualSeedRequest,
    responses(
        (status = 200, description = "Seeding complete", body = DataResponse<Vec<SeededParticipantResponse>>),
        (status = 400, description = "Invalid seeds", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament or registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn manual_seed(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<crate::dto::requests::ManualSeedRequest>,
) -> ApiResult<Json<DataResponse<Vec<crate::dto::responses::SeededParticipantResponse>>>> {
    let request_id = get_request_id(&headers);

    // Parse registration IDs
    let seeds: Vec<(portal_core::TournamentRegistrationId, i32)> = req
        .seeds
        .into_iter()
        .map(|s| {
            s.registration_id
                .parse()
                .map(|id| (id, s.seed))
                .map_err(|_| ApiError::bad_request("Invalid registration ID format"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let seeded = state
        .seeding_service
        .manual_seed(tournament_id, seeds)
        .await?;

    let data: Vec<crate::dto::responses::SeededParticipantResponse> = seeded
        .into_iter()
        .map(|p| crate::dto::responses::SeededParticipantResponse {
            registration_id: p.registration_id.to_string(),
            participant_name: p.participant_name,
            seed: p.seed,
            seed_rating: p.seed_rating,
        })
        .collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Clear all seeds for a tournament.
#[utoipa::path(
    delete,
    path = "/v1/tournaments/{tournament_id}/seeding",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 204, description = "Seeds cleared"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn clear_seeding(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<StatusCode> {

    state
        .seeding_service
        .clear_seeding(tournament_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// QUERY TYPES
// =============================================================================

/// Query parameter for filtering registrations by status.
#[derive(Debug, serde::Deserialize)]
pub struct RegistrationStatusQuery {
    #[serde(default)]
    pub status: Option<String>,
}

// =============================================================================
// MATCH LIFECYCLE
// =============================================================================

/// Get match status details.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/status",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match status details", body = DataResponse<MatchStatusDetailsResponse>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_lifecycle"
)]
pub async fn get_match_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<MatchStatusDetailsResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let details = state
        .match_lifecycle_service
        .get_match_status(match_id)
        .await?;

    Ok(Json(DataResponse::new(
        MatchStatusDetailsResponse::from(details),
        request_id,
    )))
}

/// Get match status history (transition log).
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/status-history",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match status history", body = DataResponse<Vec<MatchStatusLogResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_lifecycle"
)]
pub async fn get_match_status_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<MatchStatusLogResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let history = state
        .match_lifecycle_service
        .get_status_history(match_id)
        .await?;

    let response: Vec<MatchStatusLogResponse> = history.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Check in for a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/check-in",
    request_body = MatchCheckInRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Check-in successful", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn match_check_in(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<MatchCheckInRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let registration_id: TournamentRegistrationId = req
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let match_ = state
        .match_lifecycle_service
        .check_in(match_id, registration_id, auth.user_id)
        .await?;

    // Auto-create veto session when match transitions to PickBan
    if match_.status == TournamentMatchStatus::PickBan && match_.veto_required {
        if let Err(e) = auto_create_veto_session(&state, &match_).await {
            tracing::warn!(
                match_id = %match_id,
                error = ?e,
                "Failed to auto-create veto session on pick_ban transition"
            );
        }
    }

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Schedule a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule",
    request_body = ScheduleMatchRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match scheduled", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn schedule_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<ScheduleMatchRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let match_ = state
        .match_lifecycle_service
        .schedule(match_id, req.scheduled_at, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Forfeit a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/forfeit",
    request_body = ForfeitMatchRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Forfeit recorded", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn forfeit_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<ForfeitMatchRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let registration_id: TournamentRegistrationId = req
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    let match_ = state
        .match_lifecycle_service
        .forfeit(match_id, registration_id, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Admin force match status transition.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition",
    request_body = AdminMatchTransitionRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Transition successful", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn admin_match_transition(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminMatchTransitionRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let to_status: TournamentMatchStatus = req
        .to_status
        .parse()
        .map_err(|e| ApiError::bad_request(format!("Invalid status: {e}")))?;

    let match_ = state
        .match_lifecycle_service
        .admin_transition(match_id, to_status, auth.user_id, req.override_reason)
        .await?;

    // Auto-create veto session when admin transitions to PickBan
    if match_.status == TournamentMatchStatus::PickBan && match_.veto_required {
        if let Err(e) = auto_create_veto_session(&state, &match_).await {
            tracing::warn!(
                match_id = %match_id,
                error = ?e,
                "Failed to auto-create veto session on admin pick_ban transition"
            );
        }
    }

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

// =============================================================================
// MATCH SCHEDULING
// =============================================================================

/// Propose schedule times for a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose",
    request_body = ProposeScheduleRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Schedule proposal created", body = DataResponse<ScheduleProposalResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
        (status = 409, description = "Pending proposal already exists", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn propose_schedule(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<ProposeScheduleRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<ScheduleProposalResponse>>)> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposal = state
        .scheduling_service
        .propose_schedule(match_id, req.proposed_times, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            ScheduleProposalResponse::from(proposal),
            request_id,
        )),
    ))
}

/// Accept a schedule proposal.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept",
    request_body = AcceptScheduleProposalRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Proposal accepted, match scheduled", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn accept_schedule_proposal(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, _match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AcceptScheduleProposalRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let proposal_id: ScheduleProposalId = req
        .proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    let command = AcceptProposalCommand {
        proposal_id,
        selected_time: req.selected_time,
        accepted_by_user_id: auth.user_id,
    };

    let (_proposal, match_) = state.scheduling_service.accept_proposal(command).await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Reject a schedule proposal.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/reject",
    request_body = RejectScheduleProposalRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Proposal rejected", body = DataResponse<ScheduleProposalResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn reject_schedule_proposal(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, _match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<RejectScheduleProposalRequest>,
) -> ApiResult<Json<DataResponse<ScheduleProposalResponse>>> {
    let request_id = get_request_id(&headers);

    let proposal_id: ScheduleProposalId = req
        .proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    let command = RejectProposalCommand {
        proposal_id,
        rejected_by_user_id: auth.user_id,
    };

    let proposal = state.scheduling_service.reject_proposal(command).await?;

    Ok(Json(DataResponse::new(
        ScheduleProposalResponse::from(proposal),
        request_id,
    )))
}

/// Counter-propose with new schedule times.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/counter",
    request_body = CounterProposeRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Counter-proposal created", body = DataResponse<ScheduleProposalResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Original proposal not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn counter_propose(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<CounterProposeRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<ScheduleProposalResponse>>)> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let original_proposal_id: ScheduleProposalId = req
        .original_proposal_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid proposal ID format"))?;

    // Get the original proposal to find the user's registration
    let original_proposal = state
        .scheduling_service
        .get_active_proposal(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("No active proposal found"))?;

    // The counter-proposer must be the opponent, find their registration
    // For now, we'll need to look up the registration from the match
    let tournament_match = state
        .scheduling_service
        .get_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    // Determine which registration belongs to the counter-proposer
    let registration_id = if original_proposal.proposed_by_registration_id
        == tournament_match
            .participant1_registration_id
            .unwrap_or_default()
    {
        tournament_match
            .participant2_registration_id
            .ok_or_else(|| ApiError::bad_request("Opponent not assigned to match"))?
    } else {
        tournament_match
            .participant1_registration_id
            .ok_or_else(|| ApiError::bad_request("Opponent not assigned to match"))?
    };

    let command = CounterProposeCommand {
        original_proposal_id,
        match_id,
        proposed_by_registration_id: registration_id,
        proposed_by_user_id: auth.user_id,
        proposed_times: req.proposed_times,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(48),
    };

    let proposal = state.scheduling_service.counter_propose(command).await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            ScheduleProposalResponse::from(proposal),
            request_id,
        )),
    ))
}

/// Get active schedule proposal for a match.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/active",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Active proposal (or null if none)", body = DataResponse<Option<ScheduleProposalResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_scheduling"
)]
pub async fn get_active_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Option<ScheduleProposalResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposal = state
        .scheduling_service
        .get_active_proposal(match_id)
        .await?;

    Ok(Json(DataResponse::new(
        proposal.map(ScheduleProposalResponse::from),
        request_id,
    )))
}

/// Get schedule proposal history for a match.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/schedule/history",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Proposal history", body = DataResponse<Vec<ScheduleProposalResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "match_scheduling"
)]
pub async fn get_proposal_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<ScheduleProposalResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let proposals = state
        .scheduling_service
        .get_proposal_history(match_id)
        .await?;

    let response: Vec<ScheduleProposalResponse> =
        proposals.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Admin directly schedule a match (bypasses proposal workflow).
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule",
    request_body = AdminScheduleRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match scheduled", body = DataResponse<TournamentMatchResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_scheduling"
)]
pub async fn admin_schedule_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<AdminScheduleRequest>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let match_ = state
        .scheduling_service
        .admin_schedule(match_id, req.scheduled_at, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentMatchResponse::from(match_),
        request_id,
    )))
}

/// Get standings for a tournament bracket.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/brackets/{bracket_id}/standings",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("bracket_id" = String, Path, description = "Bracket ID")
    ),
    responses(
        (status = 200, description = "Bracket standings", body = DataResponse<Vec<TournamentStandingResponse>>),
        (status = 404, description = "Bracket not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_bracket_standings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_tournament_id, bracket_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<TournamentStandingResponse>>>> {
    let request_id = get_request_id(&headers);

    let bracket_id: portal_core::TournamentBracketId = bracket_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid bracket ID format"))?;

    let standings = state.standings_service.get_standings(bracket_id).await?;

    let response: Vec<TournamentStandingResponse> =
        standings.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Generate the next Swiss round for a tournament.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/generate-next-round",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Next round generated", body = DataResponse<Vec<TournamentMatchResponse>>),
        (status = 400, description = "Not Swiss format or current round not complete", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn admin_generate_next_swiss_round(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentMatchResponse>>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY)
        .await?;

    let new_matches = state
        .tournament_service
        .generate_next_swiss_round(tournament_id)
        .await?;

    let response: Vec<TournamentMatchResponse> =
        new_matches.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}


// =============================================================================
// INTERNAL HELPERS
// =============================================================================

/// Auto-create and start a veto session when a match transitions to PickBan.
///
/// Called after both participants check in for a veto-required match. Derives the
/// veto format from the match format and loads the map pool from tournament config.
async fn auto_create_veto_session(
    state: &AppState,
    match_: &portal_domain::entities::tournament::TournamentMatch,
) -> Result<(), ApiError> {
    // Derive veto format from match format
    let veto_format = match match_.match_format {
        MatchFormat::Bo1 => VetoFormatConfig::bo1(),
        MatchFormat::Bo3 => VetoFormatConfig::bo3(),
        MatchFormat::Bo5 | MatchFormat::Bo7 => VetoFormatConfig::bo5(),
    };

    // Resolve map pool and side selection mode
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;

    let map_pool = if let Ok(Some(pool)) = state
        .tournament_map_pool_repo
        .get_effective(match_.tournament_id, Some(match_.stage_id))
        .await
    {
        pool.maps
    } else {
        // Fall back to game's default pool
        if let Ok(Some(game)) = state
            .game_repo
            .find_by_id(tournament.game_id.as_uuid())
            .await
        {
            crate::handlers::games::extract_map_pool(&game)
        } else {
            vec![]
        }
    };

    // Resolve side selection mode: tournament settings → plugin default
    // No conversion needed — both plugin and domain use portal_core::SideSelectionMode
    let side_selection_mode = {
        use portal_core::SideSelectionMode;

        if let Some(mode) = tournament.settings
            .get("side_selection_mode")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<SideSelectionMode>().ok())
        {
            mode
        } else if let Some(plugin) = state.plugin_manager.get(&tournament.game_id.to_string()) {
            plugin.as_tournament_plugin()
                .map(|tp| tp.default_side_selection_mode())
                .unwrap_or(SideSelectionMode::Knife)
        } else {
            SideSelectionMode::Knife
        }
    };

    // Create the session
    let session = state
        .veto_service
        .create_session(match_.id, &veto_format, map_pool, None, side_selection_mode)
        .await?;

    // Auto-start the session (begins coin flip phase)
    state
        .veto_service
        .start_session(session.id)
        .await?;

    tracing::info!(
        match_id = %match_.id,
        session_id = %session.id,
        format = %veto_format.id,
        "Auto-created and started veto session on pick_ban transition"
    );

    Ok(())
}
