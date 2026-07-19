//! Tournament stage handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Stages
//! are the configuration unit for multi-phase tournaments (e.g. group
//! stage → playoffs); this module owns their create/list endpoints.

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::CreateTournamentStageRequest;
use crate::dto::responses::TournamentStageResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::TournamentState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::TournamentId;

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
    State(state): State<TournamentState>,
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
        Json(DataResponse::new(
            TournamentStageResponse::from(stage),
            request_id,
        )),
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
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentStageResponse>>>> {
    let request_id = get_request_id(&headers);

    let stages = state.tournament_service.get_stages(tournament_id).await?;

    let data: Vec<TournamentStageResponse> = stages.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}
