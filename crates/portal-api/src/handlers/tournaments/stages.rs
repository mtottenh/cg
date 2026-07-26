//! Tournament stage handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Stages
//! are the configuration unit for multi-phase tournaments (e.g. group
//! stage → playoffs); this module owns their create/list endpoints.

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::{CreateTournamentStageRequest, UpdateTournamentStageRequest};
use crate::dto::responses::TournamentStageResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::TournamentState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::{TournamentId, TournamentStageId};

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
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn create_stage(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<CreateTournamentStageRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<TournamentStageResponse>>)> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            portal_core::permissions::tournament::SETTINGS_MANAGE,
        )
        .await?;

    let cmd = req.into_command(tournament_id)?;

    let stage = state.tournament_service.create_stage(cmd).await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            TournamentStageResponse::from(stage),
            request_id,
        )),
    ))
}

/// Update a tournament stage.
///
/// Only pending stages can be edited — once a stage activates its matches
/// exist and the configuration is frozen. Editing a *pending* stage of a
/// started tournament is allowed (e.g. tuning the playoff best-of while the
/// group stage runs).
#[utoipa::path(
    patch,
    path = "/v1/tournaments/{tournament_id}/stages/{stage_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("stage_id" = String, Path, description = "Stage ID")
    ),
    request_body = UpdateTournamentStageRequest,
    responses(
        (status = 200, description = "Stage updated", body = DataResponse<TournamentStageResponse>),
        (status = 400, description = "Validation error or stage not pending", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament or stage not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn update_stage(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((tournament_id, stage_id)): Path<(TournamentId, TournamentStageId)>,
    ValidatedJson(req): ValidatedJson<UpdateTournamentStageRequest>,
) -> ApiResult<Json<DataResponse<TournamentStageResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            portal_core::permissions::tournament::SETTINGS_MANAGE,
        )
        .await?;

    let update = req.into_update()?;

    let stage = state
        .tournament_service
        .update_stage(tournament_id, stage_id, update)
        .await?;

    Ok(Json(DataResponse::new(
        TournamentStageResponse::from(stage),
        request_id,
    )))
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
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentStageResponse>>>> {
    let request_id = get_request_id(&headers);

    let stages = state.tournament_service.get_stages(tournament_id).await?;

    let data: Vec<TournamentStageResponse> = stages.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}
