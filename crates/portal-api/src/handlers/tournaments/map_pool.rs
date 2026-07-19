//! Tournament map pool handlers.
//!
//! Split out of `tournaments.rs` as the first stage of breaking up the
//! 2400-line god-module (audit item N1). The three endpoints here form a
//! self-contained subsystem — read/set/delete the effective map pool for
//! a tournament, with a game-default fallback.

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::SetTournamentMapPoolRequest;
use crate::dto::responses::TournamentMapPoolResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::TournamentState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::TournamentId;
use portal_domain::repositories::tournament::{
    TournamentMapPoolRepository, UpsertTournamentMapPool,
};

/// Get effective map pool for a tournament (tournament override → game default fallback).
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/map-pool",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Effective map pool", body = DataResponse<TournamentMapPoolResponse>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_tournament_map_pool(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<TournamentMapPoolResponse>>> {
    let request_id = get_request_id(&headers);

    let tournament = state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;

    // Try tournament-specific pool first
    if let Some(pool) = state
        .tournament_map_pool_repo
        .find_by_tournament(tournament_id)
        .await?
    {
        return Ok(Json(DataResponse::new(
            TournamentMapPoolResponse {
                maps: pool.maps,
                source: "tournament".to_string(),
            },
            request_id,
        )));
    }

    // Fall back to game's default pool
    let game = state
        .game_repo
        .find_by_id(tournament.game_id.as_uuid())
        .await?
        .ok_or_else(|| ApiError::not_found("Game not found"))?;

    let maps: Vec<String> = crate::handlers::games::extract_map_pool(&game);

    Ok(Json(DataResponse::new(
        TournamentMapPoolResponse {
            maps,
            source: "game".to_string(),
        },
        request_id,
    )))
}

/// Set a tournament-specific map pool (admin only).
#[utoipa::path(
    put,
    path = "/v1/tournaments/{tournament_id}/map-pool",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = SetTournamentMapPoolRequest,
    responses(
        (status = 200, description = "Map pool updated", body = DataResponse<TournamentMapPoolResponse>),
        (status = 400, description = "Invalid map IDs", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn set_tournament_map_pool(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<SetTournamentMapPoolRequest>,
) -> ApiResult<Json<DataResponse<TournamentMapPoolResponse>>> {
    let request_id = get_request_id(&headers);

    // Check admin permission
    perm_checker
        .require_permission(
            &auth,
            portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY,
        )
        .await?;

    let tournament = state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;

    // Validate all map IDs exist in the game's catalog
    let game = state
        .game_repo
        .find_by_id(tournament.game_id.as_uuid())
        .await?
        .ok_or_else(|| ApiError::not_found("Game not found"))?;

    let plugin = state.plugin_manager.get(&game.plugin_id);
    let catalog = crate::handlers::games::load_available_maps(&game, &plugin);

    for map_id in &req.map_ids {
        if !catalog.iter().any(|m| m.id == *map_id) {
            return Err(ApiError::bad_request(format!("Unknown map: {map_id}")));
        }
    }

    let pool = state
        .tournament_map_pool_repo
        .upsert(UpsertTournamentMapPool {
            tournament_id,
            stage_id: None,
            maps: req.map_ids,
            veto_format_id: None,
        })
        .await?;

    Ok(Json(DataResponse::new(
        TournamentMapPoolResponse {
            maps: pool.maps,
            source: "tournament".to_string(),
        },
        request_id,
    )))
}

/// Remove a tournament-specific map pool override (reverts to game default).
#[utoipa::path(
    delete,
    path = "/v1/tournaments/{tournament_id}/map-pool",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 204, description = "Map pool override removed"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament or map pool not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn delete_tournament_map_pool(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<StatusCode> {
    perm_checker
        .require_permission(
            &auth,
            portal_core::permissions::admin::TOURNAMENTS_MANAGE_ANY,
        )
        .await?;

    // Find and delete the tournament-level pool
    let pool = state
        .tournament_map_pool_repo
        .find_by_tournament(tournament_id)
        .await?
        .ok_or_else(|| ApiError::not_found("No tournament map pool override found"))?;

    state.tournament_map_pool_repo.delete(pool.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
