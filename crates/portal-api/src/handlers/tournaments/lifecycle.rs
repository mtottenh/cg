//! Tournament CRUD and state-transition handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Covers
//! the tournament entity itself: create, read (by id + slug), list,
//! update, and the full lifecycle state machine
//! (publish → open-registration → start → close/reopen-registration →
//! complete → finalize, plus cancel).

use super::get_request_id;
use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    CreateTournamentRequest, ListTournamentsQuery, UpdateTournamentRequest,
};
use crate::dto::responses::{TournamentResponse, TournamentSummaryResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::types::TournamentStatus;
use portal_core::TournamentId;
use portal_domain::repositories::tournament::TournamentFilters;

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
