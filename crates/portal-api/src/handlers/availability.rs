//! Availability handlers.
//!
//! Handles availability windows, overrides, and time suggestions for match scheduling.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::{AvailabilityExceptionId, AvailabilityWindowId, PlayerId, TournamentMatchId};
use portal_domain::entities::{
    CreateAvailabilityOverride, CreateAvailabilityWindow, OverrideType, UpdateAvailabilityWindow,
};

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    CreateAvailabilityOverrideRequest, CreateAvailabilityWindowRequest, GenerateSuggestionsRequest,
    GetAvailabilityQuery, UpdateAvailabilityWindowRequest,
};
use crate::dto::responses::{
    AvailabilityOverrideResponse, AvailabilityWindowResponse, DateAvailabilityResponse,
    SuggestedTimeResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AvailabilityState;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// AVAILABILITY WINDOWS
// =============================================================================

/// Create an availability window for the current player.
#[utoipa::path(
    post,
    path = "/v1/players/me/availability/windows",
    request_body = CreateAvailabilityWindowRequest,
    responses(
        (status = 201, description = "Availability window created", body = DataResponse<AvailabilityWindowResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Duplicate time slot", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn create_player_window(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<CreateAvailabilityWindowRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<AvailabilityWindowResponse>>)> {
    let request_id = get_request_id(&headers);

    let command = CreateAvailabilityWindow {
        player_id: Some(auth.player_id),
        registration_id: None,
        day_of_week: req.day_of_week,
        start_time: req.start_time,
        end_time: req.end_time,
        timezone: req.timezone,
        is_preferred: req.is_preferred,
        notes: req.notes,
    };

    let window = state.availability_service.create_window(command).await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            AvailabilityWindowResponse::from(window),
            request_id,
        )),
    ))
}

/// Get all availability windows for the current player.
#[utoipa::path(
    get,
    path = "/v1/players/me/availability/windows",
    responses(
        (status = 200, description = "Availability windows", body = DataResponse<Vec<AvailabilityWindowResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn get_player_windows(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<AvailabilityWindowResponse>>>> {
    let request_id = get_request_id(&headers);

    let windows = state
        .availability_service
        .get_player_windows(auth.player_id)
        .await?;

    let response: Vec<AvailabilityWindowResponse> = windows.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Update an availability window.
#[utoipa::path(
    patch,
    path = "/v1/players/me/availability/windows/{window_id}",
    params(
        ("window_id" = String, Path, description = "Availability window ID")
    ),
    request_body = UpdateAvailabilityWindowRequest,
    responses(
        (status = 200, description = "Window updated", body = DataResponse<AvailabilityWindowResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Window not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn update_player_window(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(window_id): Path<AvailabilityWindowId>,
    ValidatedJson(req): ValidatedJson<UpdateAvailabilityWindowRequest>,
) -> ApiResult<Json<DataResponse<AvailabilityWindowResponse>>> {
    let request_id = get_request_id(&headers);

    // Verify ownership
    let existing = state
        .availability_service
        .get_window(window_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Availability window not found"))?;

    if existing.player_id != Some(auth.player_id) {
        return Err(ApiError::forbidden(
            "Cannot modify another player's availability",
        ));
    }

    let command = UpdateAvailabilityWindow {
        day_of_week: req.day_of_week,
        start_time: req.start_time,
        end_time: req.end_time,
        timezone: req.timezone.map(Some),
        is_preferred: req.is_preferred,
        notes: req.notes.map(Some),
    };

    let window = state
        .availability_service
        .update_window(window_id, command)
        .await?;

    Ok(Json(DataResponse::new(
        AvailabilityWindowResponse::from(window),
        request_id,
    )))
}

/// Delete an availability window.
#[utoipa::path(
    delete,
    path = "/v1/players/me/availability/windows/{window_id}",
    params(
        ("window_id" = String, Path, description = "Availability window ID")
    ),
    responses(
        (status = 204, description = "Window deleted"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Window not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn delete_player_window(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    Path(window_id): Path<AvailabilityWindowId>,
) -> ApiResult<StatusCode> {
    // Verify ownership
    let existing = state
        .availability_service
        .get_window(window_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Availability window not found"))?;

    if existing.player_id != Some(auth.player_id) {
        return Err(ApiError::forbidden(
            "Cannot delete another player's availability",
        ));
    }

    state.availability_service.delete_window(window_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// AVAILABILITY OVERRIDES
// =============================================================================

/// Create an availability override (blocked or extra availability date).
#[utoipa::path(
    post,
    path = "/v1/players/me/availability/overrides",
    request_body = CreateAvailabilityOverrideRequest,
    responses(
        (status = 201, description = "Override created", body = DataResponse<AvailabilityOverrideResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn create_player_override(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<CreateAvailabilityOverrideRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<AvailabilityOverrideResponse>>)> {
    let request_id = get_request_id(&headers);

    let override_type: OverrideType = req
        .override_type
        .parse()
        .map_err(|e: String| ApiError::bad_request(e))?;

    let command = CreateAvailabilityOverride {
        player_id: Some(auth.player_id),
        registration_id: None,
        override_date: req.override_date,
        start_time: req.start_time,
        end_time: req.end_time,
        override_type,
        reason: req.reason,
    };

    let override_ = state.availability_service.create_override(command).await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            AvailabilityOverrideResponse::from(override_),
            request_id,
        )),
    ))
}

/// Get all availability overrides for the current player.
#[utoipa::path(
    get,
    path = "/v1/players/me/availability/overrides",
    responses(
        (status = 200, description = "Availability overrides", body = DataResponse<Vec<AvailabilityOverrideResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn get_player_overrides(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<AvailabilityOverrideResponse>>>> {
    let request_id = get_request_id(&headers);

    let overrides = state
        .availability_service
        .get_player_overrides(auth.player_id)
        .await?;

    let response: Vec<AvailabilityOverrideResponse> =
        overrides.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Delete an availability override.
#[utoipa::path(
    delete,
    path = "/v1/players/me/availability/overrides/{override_id}",
    params(
        ("override_id" = String, Path, description = "Availability override ID")
    ),
    responses(
        (status = 204, description = "Override deleted"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Override not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn delete_player_override(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    Path(override_id): Path<AvailabilityExceptionId>,
) -> ApiResult<StatusCode> {
    // Verify ownership
    let existing = state
        .availability_service
        .get_override(override_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Availability override not found"))?;

    if existing.player_id != Some(auth.player_id) {
        return Err(ApiError::forbidden(
            "Cannot delete another player's availability",
        ));
    }

    state
        .availability_service
        .delete_override(override_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// DATE AVAILABILITY
// =============================================================================

/// Get availability for a specific date.
#[utoipa::path(
    get,
    path = "/v1/players/me/availability/date",
    params(
        ("date" = NaiveDate, Query, description = "Date to check availability for")
    ),
    responses(
        (status = 200, description = "Date availability", body = DataResponse<DateAvailabilityResponse>),
        (status = 400, description = "Invalid date", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn get_player_date_availability(
    State(state): State<AvailabilityState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<GetAvailabilityQuery>,
) -> ApiResult<Json<DataResponse<DateAvailabilityResponse>>> {
    let request_id = get_request_id(&headers);

    let availability = state
        .availability_service
        .get_availability_for_date(Some(auth.player_id), None, query.date)
        .await?;

    Ok(Json(DataResponse::new(
        DateAvailabilityResponse::from(availability),
        request_id,
    )))
}

/// Get availability for a specific player on a date (public).
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/availability/date",
    params(
        ("player_id" = String, Path, description = "Player ID"),
        ("date" = NaiveDate, Query, description = "Date to check availability for")
    ),
    responses(
        (status = 200, description = "Date availability", body = DataResponse<DateAvailabilityResponse>),
        (status = 400, description = "Invalid parameters", body = ApiError),
        (status = 404, description = "Player not found", body = ApiError),
    ),
    tag = "availability"
)]
pub async fn get_player_date_availability_public(
    State(state): State<AvailabilityState>,
    headers: HeaderMap,
    Path(player_id): Path<PlayerId>,
    Query(query): Query<GetAvailabilityQuery>,
) -> ApiResult<Json<DataResponse<DateAvailabilityResponse>>> {
    let request_id = get_request_id(&headers);

    let availability = state
        .availability_service
        .get_availability_for_date(Some(player_id), None, query.date)
        .await?;

    Ok(Json(DataResponse::new(
        DateAvailabilityResponse::from(availability),
        request_id,
    )))
}

// =============================================================================
// TIME SUGGESTIONS
// =============================================================================

/// Generate time suggestions for a match based on participant availability.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/suggestions/generate",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    request_body = GenerateSuggestionsRequest,
    responses(
        (status = 201, description = "Suggestions generated", body = DataResponse<Vec<SuggestedTimeResponse>>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "availability"
)]
pub async fn generate_suggestions(
    State(state): State<AvailabilityState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<GenerateSuggestionsRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<Vec<SuggestedTimeResponse>>>)> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let suggestions = state
        .availability_service
        .generate_suggestions(
            match_id,
            req.start_date,
            req.end_date,
            req.min_duration_minutes,
        )
        .await?;

    let response: Vec<SuggestedTimeResponse> = suggestions.into_iter().map(Into::into).collect();

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(response, request_id)),
    ))
}

/// Get active suggestions for a match.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/suggestions",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Suggestions", body = DataResponse<Vec<SuggestedTimeResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "availability"
)]
pub async fn get_suggestions(
    State(state): State<AvailabilityState>,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<SuggestedTimeResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let suggestions = state
        .availability_service
        .get_active_suggestions(match_id)
        .await?;

    let response: Vec<SuggestedTimeResponse> = suggestions.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(response, request_id)))
}
