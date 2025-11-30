//! Tournament handlers.
//!
//! Phase 1 implementation - core tournament CRUD operations.

use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    CreateTournamentRequest, CreateTournamentStageRequest, ListTournamentsQuery,
    RegisterPlayerRequest, RegisterTeamRequest, UpdateTournamentRequest,
};
use crate::dto::responses::{
    TournamentBracketResponse, TournamentMatchResponse, TournamentRegistrationResponse,
    TournamentResponse, TournamentStageResponse, TournamentSummaryResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::TournamentId;
use portal_domain::repositories::tournament::TournamentFilters;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
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

    let cmd = req.into_command()?;

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
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
    ValidatedJson(req): ValidatedJson<UpdateTournamentRequest>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<TournamentResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

    let tournament = state.tournament_service.start_tournament(tournament_id).await?;

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
    Path(tournament_id): Path<String>,
    ValidatedJson(req): ValidatedJson<CreateTournamentStageRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentStageResponse>>)> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<TournamentStageResponse>>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
    ValidatedJson(req): ValidatedJson<RegisterTeamRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentRegistrationResponse>>)> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

    let team_season_id = req.parse_team_season_id()?;

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
    Path(tournament_id): Path<String>,
    ValidatedJson(req): ValidatedJson<RegisterPlayerRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentRegistrationResponse>>)> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

    let player_id = auth.player_id;

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
    Path(tournament_id): Path<String>,
    Query(status_filter): Query<RegistrationStatusQuery>,
    Query(pagination): Query<PaginationParams>,
) -> ApiResult<Json<PaginatedResponse<TournamentRegistrationResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<TournamentBracketResponse>>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

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
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<TournamentMatchResponse>>>> {
    let request_id = get_request_id(&headers);

    let tournament_id: TournamentId = tournament_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))?;

    let matches = state
        .tournament_service
        .get_tournament_matches(tournament_id)
        .await?;

    let data: Vec<TournamentMatchResponse> = matches.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
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
