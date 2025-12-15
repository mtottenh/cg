//! Demo catalog handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    AssociateDemoRequest, CatalogDemoRequest, CategorizeDemoRequest, GetDemosForMatchQuery,
    LinkDemoToMatchRequest, ListDemosQuery, PendingDemosQuery, SetDemoVisibilityRequest,
};
use crate::dto::responses::{
    DemoListResponse, DemoMatchLinkResponse, DemoMatchLinkWithDemoResponse, DemoPlayerResponse,
    DemoResponse, DemoStatusCountsResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::DateTime;
use portal_core::{DemoCategory, DemoId, DemoLinkType, DemoStatus, GameId, LeagueId, TournamentId, TournamentMatchId};
use portal_domain::entities::demo::DemoFilter;
use validator::Validate;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// List demos with filtering.
#[utoipa::path(
    get,
    path = "/v1/demos",
    params(
        ("game_id" = Option<String>, Query, description = "Filter by game ID"),
        ("category" = Option<String>, Query, description = "Filter by category (uncategorized, pug, league, scrim, ignored)"),
        ("status" = Option<String>, Query, description = "Filter by status (pending, processing, ready, failed, archived)"),
        ("league_id" = Option<String>, Query, description = "Filter by league ID"),
        ("tournament_id" = Option<String>, Query, description = "Filter by tournament ID"),
        ("map_name" = Option<String>, Query, description = "Filter by map name (partial match)"),
        ("team_name" = Option<String>, Query, description = "Filter by team name (partial match)"),
        ("steam_id" = Option<String>, Query, description = "Filter by player Steam ID"),
        ("match_date_from" = Option<String>, Query, description = "Filter by match date from (ISO 8601)"),
        ("match_date_to" = Option<String>, Query, description = "Filter by match date to (ISO 8601)"),
        ("include_hidden" = Option<bool>, Query, description = "Include hidden demos (admin only)"),
        ("limit" = Option<i64>, Query, description = "Maximum results"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination"),
    ),
    responses(
        (status = 200, description = "List of demos", body = DataResponse<DemoListResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn list_demos(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<ListDemosQuery>,
) -> ApiResult<Json<DataResponse<DemoListResponse>>> {
    let request_id = get_request_id(&headers);

    // Check if user can view hidden demos
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    let include_hidden = query.include_hidden && is_admin;

    let filter = DemoFilter {
        game_id: query.game_id.map(GameId::from),
        category: query.category.as_ref().and_then(|c| c.parse::<DemoCategory>().ok()),
        status: query.status.as_ref().and_then(|s| s.parse::<DemoStatus>().ok()),
        league_id: query.league_id.map(LeagueId::from),
        tournament_id: query.tournament_id.map(TournamentId::from),
        map_name: query.map_name,
        team_name_contains: query.team_name,
        steam_id: query.steam_id,
        match_date_from: query.match_date_from.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.to_utc()),
        match_date_to: query.match_date_to.as_ref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.to_utc()),
        include_hidden,
        limit: query.limit,
        offset: query.offset,
    };

    let result = state.demo_service.list_demos(filter).await?;

    Ok(Json(DataResponse::new(
        DemoListResponse::from(result),
        request_id,
    )))
}

/// Get a demo by ID.
#[utoipa::path(
    get,
    path = "/v1/demos/{id}",
    params(
        ("id" = String, Path, description = "Demo ID"),
    ),
    responses(
        (status = 200, description = "Demo details", body = DataResponse<DemoResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn get_demo(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<DataResponse<DemoResponse>>> {
    let request_id = get_request_id(&headers);
    let demo_id = id.parse::<DemoId>().map_err(|_| ApiError::bad_request("Invalid demo ID"))?;

    let demo = state.demo_service.get_demo(demo_id).await?;

    Ok(Json(DataResponse::new(DemoResponse::from(demo), request_id)))
}

/// Get a demo with its players.
#[utoipa::path(
    get,
    path = "/v1/demos/{id}/players",
    params(
        ("id" = String, Path, description = "Demo ID"),
    ),
    responses(
        (status = 200, description = "Demo players", body = DataResponse<Vec<DemoPlayerResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn get_demo_players(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<DemoPlayerResponse>>>> {
    let request_id = get_request_id(&headers);
    let demo_id = id.parse::<DemoId>().map_err(|_| ApiError::bad_request("Invalid demo ID"))?;

    let players = state.demo_service.get_demo_players(demo_id).await?;
    let responses: Vec<DemoPlayerResponse> = players.into_iter().map(DemoPlayerResponse::from).collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Catalog a new demo from S3.
#[utoipa::path(
    post,
    path = "/v1/admin/demos",
    request_body = CatalogDemoRequest,
    responses(
        (status = 201, description = "Demo cataloged", body = DataResponse<DemoResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn catalog_demo(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Json(request): Json<CatalogDemoRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<DemoResponse>>)> {
    request.validate()?;
    let request_id = get_request_id(&headers);

    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let demo = state
        .demo_service
        .catalog_demo(
            GameId::from(request.game_id),
            request.file_name,
            request.s3_bucket,
            request.s3_key,
            request.file_size_bytes,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(DemoResponse::from(demo), request_id)),
    ))
}

/// Categorize a demo.
#[utoipa::path(
    post,
    path = "/v1/admin/demos/{id}/categorize",
    params(
        ("id" = String, Path, description = "Demo ID"),
    ),
    request_body = CategorizeDemoRequest,
    responses(
        (status = 200, description = "Demo categorized", body = DataResponse<DemoResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn categorize_demo(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<CategorizeDemoRequest>,
) -> ApiResult<Json<DataResponse<DemoResponse>>> {
    request.validate()?;
    let request_id = get_request_id(&headers);
    let demo_id = id.parse::<DemoId>().map_err(|_| ApiError::bad_request("Invalid demo ID"))?;

    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let category: DemoCategory = request
        .category
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid category"))?;

    let demo = state
        .demo_service
        .categorize_demo(demo_id, category, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(DemoResponse::from(demo), request_id)))
}

/// Hide or unhide a demo.
#[utoipa::path(
    post,
    path = "/v1/admin/demos/{id}/visibility",
    params(
        ("id" = String, Path, description = "Demo ID"),
    ),
    request_body = SetDemoVisibilityRequest,
    responses(
        (status = 200, description = "Visibility updated", body = DataResponse<DemoResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn set_demo_visibility(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<SetDemoVisibilityRequest>,
) -> ApiResult<Json<DataResponse<DemoResponse>>> {
    request.validate()?;
    let request_id = get_request_id(&headers);
    let demo_id = id.parse::<DemoId>().map_err(|_| ApiError::bad_request("Invalid demo ID"))?;

    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let demo = state
        .demo_service
        .set_demo_visibility(demo_id, request.is_hidden, auth.user_id)
        .await?;

    Ok(Json(DataResponse::new(DemoResponse::from(demo), request_id)))
}

/// Associate a demo with a league/tournament.
#[utoipa::path(
    post,
    path = "/v1/admin/demos/{id}/associate",
    params(
        ("id" = String, Path, description = "Demo ID"),
    ),
    request_body = AssociateDemoRequest,
    responses(
        (status = 200, description = "Demo associated", body = DataResponse<DemoResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn associate_demo(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<AssociateDemoRequest>,
) -> ApiResult<Json<DataResponse<DemoResponse>>> {
    request.validate()?;
    let request_id = get_request_id(&headers);
    let demo_id = id.parse::<DemoId>().map_err(|_| ApiError::bad_request("Invalid demo ID"))?;

    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let demo = state
        .demo_service
        .associate_demo(
            demo_id,
            request.league_id.map(LeagueId::from),
            request.tournament_id.map(TournamentId::from),
        )
        .await?;

    Ok(Json(DataResponse::new(DemoResponse::from(demo), request_id)))
}

/// Link a demo to a tournament match.
#[utoipa::path(
    post,
    path = "/v1/admin/demos/{id}/link",
    params(
        ("id" = String, Path, description = "Demo ID"),
    ),
    request_body = LinkDemoToMatchRequest,
    responses(
        (status = 201, description = "Demo linked to match", body = DataResponse<DemoMatchLinkResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
        (status = 409, description = "Demo already linked to this match", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn link_demo_to_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<LinkDemoToMatchRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<DemoMatchLinkResponse>>)> {
    request.validate()?;
    let request_id = get_request_id(&headers);
    let demo_id = id.parse::<DemoId>().map_err(|_| ApiError::bad_request("Invalid demo ID"))?;

    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let link_type: DemoLinkType = request
        .link_type
        .as_deref()
        .unwrap_or("manual")
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid link type"))?;

    let link = state
        .demo_service
        .link_to_match(
            demo_id,
            TournamentMatchId::from(request.match_id),
            request.game_number,
            link_type,
            Some(auth.user_id),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(DemoMatchLinkResponse::from(link), request_id)),
    ))
}

/// Get demo links for a demo.
#[utoipa::path(
    get,
    path = "/v1/demos/{id}/links",
    params(
        ("id" = String, Path, description = "Demo ID"),
    ),
    responses(
        (status = 200, description = "Demo links", body = DataResponse<Vec<DemoMatchLinkResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn get_demo_links(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<DemoMatchLinkResponse>>>> {
    let request_id = get_request_id(&headers);
    let demo_id = id.parse::<DemoId>().map_err(|_| ApiError::bad_request("Invalid demo ID"))?;

    let links = state.demo_service.get_demo_links(demo_id).await?;
    let responses: Vec<DemoMatchLinkResponse> = links.into_iter().map(DemoMatchLinkResponse::from).collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Get demo status counts for admin dashboard.
#[utoipa::path(
    get,
    path = "/v1/admin/demos/stats",
    responses(
        (status = 200, description = "Demo status counts", body = DataResponse<DemoStatusCountsResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn get_demo_stats(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<DemoStatusCountsResponse>>> {
    let request_id = get_request_id(&headers);

    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let counts = state.demo_service.get_status_counts().await?;

    let response = DemoStatusCountsResponse {
        pending: counts.iter().find(|(s, _)| *s == DemoStatus::Pending).map(|(_, c)| *c).unwrap_or(0),
        processing: counts.iter().find(|(s, _)| *s == DemoStatus::Processing).map(|(_, c)| *c).unwrap_or(0),
        ready: counts.iter().find(|(s, _)| *s == DemoStatus::Ready).map(|(_, c)| *c).unwrap_or(0),
        failed: counts.iter().find(|(s, _)| *s == DemoStatus::Failed).map(|(_, c)| *c).unwrap_or(0),
        archived: counts.iter().find(|(s, _)| *s == DemoStatus::Archived).map(|(_, c)| *c).unwrap_or(0),
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Get demos pending processing.
#[utoipa::path(
    get,
    path = "/v1/admin/demos/pending",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of demos to return"),
    ),
    responses(
        (status = 200, description = "Pending demos", body = DataResponse<Vec<DemoResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn get_pending_demos(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<PendingDemosQuery>,
) -> ApiResult<Json<DataResponse<Vec<DemoResponse>>>> {
    let request_id = get_request_id(&headers);

    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let demos = state
        .demo_service
        .get_pending_demos(query.limit.unwrap_or(50))
        .await?;

    let responses: Vec<DemoResponse> = demos.into_iter().map(DemoResponse::from).collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Get demos linked to a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/demos",
    params(
        ("match_id" = String, Path, description = "Tournament match ID"),
        ("include_stats" = Option<bool>, Query, description = "Include player stats"),
        ("game_number" = Option<i32>, Query, description = "Filter by game number"),
    ),
    responses(
        (status = 200, description = "Demos linked to the match", body = DataResponse<Vec<DemoMatchLinkWithDemoResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn get_demos_for_match(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Query(query): Query<GetDemosForMatchQuery>,
) -> ApiResult<Json<DataResponse<Vec<DemoMatchLinkWithDemoResponse>>>> {
    let request_id = get_request_id(&headers);
    let match_id = match_id
        .parse::<TournamentMatchId>()
        .map_err(|_| ApiError::bad_request("Invalid match ID"))?;

    let demos_with_data = state
        .demo_service
        .get_match_demos_with_data(match_id, query.include_stats, query.game_number)
        .await?;

    let responses: Vec<DemoMatchLinkWithDemoResponse> = demos_with_data
        .into_iter()
        .map(|d| {
            DemoMatchLinkWithDemoResponse::from_domain(
                d.link,
                d.demo,
                d.players,
                query.include_stats,
            )
        })
        .collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Unlink a demo from a match (admin only).
#[utoipa::path(
    delete,
    path = "/v1/admin/demos/{demo_id}/link/{match_id}",
    params(
        ("demo_id" = String, Path, description = "Demo ID"),
        ("match_id" = String, Path, description = "Match ID to unlink from"),
    ),
    responses(
        (status = 204, description = "Demo unlinked from match"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Admin access required", body = ApiError),
        (status = 404, description = "Link not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "admin"
)]
pub async fn unlink_demo_from_match(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((demo_id, match_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    // Check admin permission
    let is_admin = state
        .permission_service
        .is_admin(auth.user_id)
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin access required"));
    }

    let demo_id = demo_id
        .parse::<DemoId>()
        .map_err(|_| ApiError::bad_request("Invalid demo ID"))?;
    let match_id = match_id
        .parse::<TournamentMatchId>()
        .map_err(|_| ApiError::bad_request("Invalid match ID"))?;

    state
        .demo_service
        .unlink_from_match(demo_id, match_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
