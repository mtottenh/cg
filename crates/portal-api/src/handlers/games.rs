//! Game handlers.

use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{SetMapPoolRequest, UpdateGameRequest};
use crate::dto::responses::{
    GameDetailResponse, GameSummaryResponse, MapInfoResponse, MapPickBanFormatResponse,
    RankTierResponse, TeamSizeConfig,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use portal_db::entities::UpdateGame;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// ============================================================================
// PUBLIC ENDPOINTS
// ============================================================================

/// List all active games.
#[utoipa::path(
    get,
    path = "/v1/games",
    params(PaginationParams),
    responses(
        (status = 200, description = "List of active games", body = PaginatedResponse<GameSummaryResponse>),
    ),
    tag = "games"
)]
pub async fn list_games(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<PaginationParams>,
) -> ApiResult<Json<PaginatedResponse<GameSummaryResponse>>> {
    let request_id = get_request_id(&headers);

    // Fetch active games from database
    let games = state.game_repo.list_active().await?;

    // Convert to response DTOs
    let game_responses: Vec<GameSummaryResponse> = games
        .into_iter()
        .map(|g| GameSummaryResponse {
            id: g.id,
            display_name: g.display_name,
            short_name: g.short_name,
            description: g.description,
            icon_url: g.icon_url,
            team_size_default: g.team_size_default,
            status: g.status,
            is_featured: g.is_featured,
        })
        .collect();

    let total = game_responses.len() as u64;

    Ok(Json(PaginatedResponse::new(
        game_responses,
        &params,
        total,
        request_id,
    )))
}

/// Get a game by ID with full details.
#[utoipa::path(
    get,
    path = "/v1/games/{game_id}",
    params(
        ("game_id" = String, Path, description = "Game ID (e.g., cs2, aoe4)")
    ),
    responses(
        (status = 200, description = "Game details", body = DataResponse<GameDetailResponse>),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    tag = "games"
)]
pub async fn get_game(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(game_id): Path<String>,
) -> ApiResult<Json<DataResponse<GameDetailResponse>>> {
    let request_id = get_request_id(&headers);

    // Fetch from database
    let game = state
        .game_repo
        .find_by_id(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", game_id)))?;

    // Get plugin for additional metadata
    let plugin = state.plugin_manager.get(&game.plugin_id);

    // Build maps from DB or plugin defaults
    let maps: Vec<MapInfoResponse> = if let Some(arr) = game.available_maps.as_array() {
        if !arr.is_empty() {
            serde_json::from_value(game.available_maps.clone()).unwrap_or_default()
        } else if let Some(p) = &plugin {
            p.available_maps().into_iter().map(Into::into).collect()
        } else {
            vec![]
        }
    } else if let Some(p) = &plugin {
        p.available_maps().into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    // Build rank tiers from DB or plugin defaults
    let rank_tiers: Vec<RankTierResponse> = if let Some(arr) = game.rank_tiers.as_array() {
        if !arr.is_empty() {
            serde_json::from_value(game.rank_tiers.clone()).unwrap_or_default()
        } else if let Some(p) = &plugin {
            p.rank_tiers().into_iter().map(Into::into).collect()
        } else {
            vec![]
        }
    } else if let Some(p) = &plugin {
        p.rank_tiers().into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    // Get match formats and pick/ban formats from plugin
    let (supported_match_formats, default_match_format, map_pick_ban_formats) = if let Some(p) =
        &plugin
    {
        (
            p.supported_match_formats()
                .iter()
                .map(|f| f.to_string())
                .collect(),
            p.default_match_format().to_string(),
            p.map_pick_ban_formats()
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    } else {
        (
            vec!["bo1".to_string(), "bo3".to_string(), "bo5".to_string()],
            "bo1".to_string(),
            vec![],
        )
    };

    let response = GameDetailResponse {
        id: game.id,
        display_name: game.display_name,
        short_name: game.short_name,
        description: game.description,
        icon_url: game.icon_url,
        logo_url: game.logo_url,
        banner_url: game.banner_url,
        team_size: TeamSizeConfig {
            min: game.team_size_min,
            max: game.team_size_max,
            default: game.team_size_default,
        },
        maps,
        rank_tiers,
        supported_match_formats,
        default_match_format,
        map_pick_ban_formats,
        status: game.status,
        is_featured: game.is_featured,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Get available maps for a game.
#[utoipa::path(
    get,
    path = "/v1/games/{game_id}/maps",
    params(
        ("game_id" = String, Path, description = "Game ID")
    ),
    responses(
        (status = 200, description = "Available maps", body = DataResponse<Vec<MapInfoResponse>>),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    tag = "games"
)]
pub async fn get_maps(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(game_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<MapInfoResponse>>>> {
    let request_id = get_request_id(&headers);

    // Fetch from database
    let game = state
        .game_repo
        .find_by_id(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", game_id)))?;

    // Get plugin for defaults
    let plugin = state.plugin_manager.get(&game.plugin_id);

    // Build maps from DB or plugin defaults
    let maps: Vec<MapInfoResponse> = if let Some(arr) = game.available_maps.as_array() {
        if !arr.is_empty() {
            serde_json::from_value(game.available_maps.clone()).unwrap_or_default()
        } else if let Some(p) = &plugin {
            p.available_maps().into_iter().map(Into::into).collect()
        } else {
            vec![]
        }
    } else if let Some(p) = &plugin {
        p.available_maps().into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    Ok(Json(DataResponse::new(maps, request_id)))
}

/// Get rank tiers for a game.
#[utoipa::path(
    get,
    path = "/v1/games/{game_id}/rank-tiers",
    params(
        ("game_id" = String, Path, description = "Game ID")
    ),
    responses(
        (status = 200, description = "Rank tier definitions", body = DataResponse<Vec<RankTierResponse>>),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    tag = "games"
)]
pub async fn get_rank_tiers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(game_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<RankTierResponse>>>> {
    let request_id = get_request_id(&headers);

    // Fetch from database
    let game = state
        .game_repo
        .find_by_id(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", game_id)))?;

    // Get plugin for defaults
    let plugin = state.plugin_manager.get(&game.plugin_id);

    // Build rank tiers from DB or plugin defaults
    let rank_tiers: Vec<RankTierResponse> = if let Some(arr) = game.rank_tiers.as_array() {
        if !arr.is_empty() {
            serde_json::from_value(game.rank_tiers.clone()).unwrap_or_default()
        } else if let Some(p) = &plugin {
            p.rank_tiers().into_iter().map(Into::into).collect()
        } else {
            vec![]
        }
    } else if let Some(p) = &plugin {
        p.rank_tiers().into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    Ok(Json(DataResponse::new(rank_tiers, request_id)))
}

// ============================================================================
// ADMIN ENDPOINTS
// ============================================================================

/// Update a game's settings (admin only).
#[utoipa::path(
    patch,
    path = "/v1/games/{game_id}",
    params(
        ("game_id" = String, Path, description = "Game ID")
    ),
    request_body = UpdateGameRequest,
    responses(
        (status = 200, description = "Game updated", body = DataResponse<GameDetailResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn update_game(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(game_id): Path<String>,
    ValidatedJson(req): ValidatedJson<UpdateGameRequest>,
) -> ApiResult<Json<DataResponse<GameDetailResponse>>> {
    let request_id = get_request_id(&headers);

    // Check admin permission
    let is_admin = state
        .permission_repo
        .user_has_permission(auth.user_id, "admin.games.manage")
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden(
            "Admin permission required to update games",
        ));
    }

    // Build update struct
    let update = UpdateGame {
        display_name: req.display_name,
        short_name: req.short_name,
        description: req.description,
        icon_url: req.icon_url,
        is_featured: req.is_featured,
        sort_order: req.sort_order,
        ..Default::default()
    };

    // Update in database
    let game = state.game_repo.update(&game_id, update).await?;

    // Get plugin for additional metadata
    let plugin = state.plugin_manager.get(&game.plugin_id);

    // Build response (same logic as get_game)
    let maps: Vec<MapInfoResponse> = if let Some(arr) = game.available_maps.as_array() {
        if !arr.is_empty() {
            serde_json::from_value(game.available_maps.clone()).unwrap_or_default()
        } else if let Some(p) = &plugin {
            p.available_maps().into_iter().map(Into::into).collect()
        } else {
            vec![]
        }
    } else if let Some(p) = &plugin {
        p.available_maps().into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    let rank_tiers: Vec<RankTierResponse> = if let Some(arr) = game.rank_tiers.as_array() {
        if !arr.is_empty() {
            serde_json::from_value(game.rank_tiers.clone()).unwrap_or_default()
        } else if let Some(p) = &plugin {
            p.rank_tiers().into_iter().map(Into::into).collect()
        } else {
            vec![]
        }
    } else if let Some(p) = &plugin {
        p.rank_tiers().into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    let (supported_match_formats, default_match_format, map_pick_ban_formats) = if let Some(p) =
        &plugin
    {
        (
            p.supported_match_formats()
                .iter()
                .map(|f| f.to_string())
                .collect(),
            p.default_match_format().to_string(),
            p.map_pick_ban_formats()
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    } else {
        (
            vec!["bo1".to_string(), "bo3".to_string(), "bo5".to_string()],
            "bo1".to_string(),
            vec![],
        )
    };

    let response = GameDetailResponse {
        id: game.id,
        display_name: game.display_name,
        short_name: game.short_name,
        description: game.description,
        icon_url: game.icon_url,
        logo_url: game.logo_url,
        banner_url: game.banner_url,
        team_size: TeamSizeConfig {
            min: game.team_size_min,
            max: game.team_size_max,
            default: game.team_size_default,
        },
        maps,
        rank_tiers,
        supported_match_formats,
        default_match_format,
        map_pick_ban_formats,
        status: game.status,
        is_featured: game.is_featured,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Set a game's map pool (admin only).
#[utoipa::path(
    put,
    path = "/v1/games/{game_id}/maps",
    params(
        ("game_id" = String, Path, description = "Game ID")
    ),
    request_body = SetMapPoolRequest,
    responses(
        (status = 200, description = "Map pool updated", body = DataResponse<Vec<MapInfoResponse>>),
        (status = 400, description = "Invalid map IDs", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn set_map_pool(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(game_id): Path<String>,
    ValidatedJson(req): ValidatedJson<SetMapPoolRequest>,
) -> ApiResult<Json<DataResponse<Vec<MapInfoResponse>>>> {
    let request_id = get_request_id(&headers);

    // Check admin permission
    let is_admin = state
        .permission_repo
        .user_has_permission(auth.user_id, "admin.games.manage")
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden(
            "Admin permission required to update map pool",
        ));
    }

    // Fetch game to verify it exists and get plugin
    let game = state
        .game_repo
        .find_by_id(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", game_id)))?;

    // Get plugin to validate map IDs
    let plugin = state.plugin_manager.get(&game.plugin_id);

    if let Some(p) = &plugin {
        // Validate all map IDs exist
        if let Err(e) = p.validate_map_pool(&req.map_ids) {
            return Err(ApiError::bad_request(e));
        }

        // Check if custom map pools are supported
        if !p.supports_custom_map_pool() {
            return Err(ApiError::bad_request(
                "This game does not support custom map pools",
            ));
        }
    }

    // Get available maps and filter to the requested pool
    let all_maps: Vec<MapInfoResponse> = if let Some(p) = &plugin {
        p.available_maps().into_iter().map(Into::into).collect()
    } else {
        vec![]
    };

    // Build new map pool as JSONB
    let pool_maps: Vec<MapInfoResponse> = all_maps
        .into_iter()
        .filter(|m| req.map_ids.contains(&m.id))
        .collect();

    let pool_json = serde_json::to_value(&pool_maps).unwrap_or_default();
    let pool_ids_json = serde_json::to_value(&req.map_ids).unwrap_or_default();

    // Update database
    let update = UpdateGame {
        available_maps: Some(pool_json),
        default_map_pool: Some(pool_ids_json),
        ..Default::default()
    };

    let _ = state.game_repo.update(&game_id, update).await?;

    Ok(Json(DataResponse::new(pool_maps, request_id)))
}

/// Enable a game (admin only).
#[utoipa::path(
    post,
    path = "/v1/games/{game_id}/enable",
    params(
        ("game_id" = String, Path, description = "Game ID")
    ),
    responses(
        (status = 200, description = "Game enabled", body = DataResponse<GameSummaryResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn enable_game(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(game_id): Path<String>,
) -> ApiResult<Json<DataResponse<GameSummaryResponse>>> {
    let request_id = get_request_id(&headers);

    // Check admin permission
    let is_admin = state
        .permission_repo
        .user_has_permission(auth.user_id, "admin.games.manage")
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden("Admin permission required to enable games"));
    }

    // Enable the game
    let game = state.game_repo.enable(&game_id).await?;

    let response = GameSummaryResponse {
        id: game.id,
        display_name: game.display_name,
        short_name: game.short_name,
        description: game.description,
        icon_url: game.icon_url,
        team_size_default: game.team_size_default,
        status: game.status,
        is_featured: game.is_featured,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}

/// Disable a game (admin only).
#[utoipa::path(
    post,
    path = "/v1/games/{game_id}/disable",
    params(
        ("game_id" = String, Path, description = "Game ID")
    ),
    responses(
        (status = 200, description = "Game disabled", body = DataResponse<GameSummaryResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn disable_game(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(game_id): Path<String>,
) -> ApiResult<Json<DataResponse<GameSummaryResponse>>> {
    let request_id = get_request_id(&headers);

    // Check admin permission
    let is_admin = state
        .permission_repo
        .user_has_permission(auth.user_id, "admin.games.manage")
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden(
            "Admin permission required to disable games",
        ));
    }

    // Disable the game
    let game = state.game_repo.disable(&game_id).await?;

    let response = GameSummaryResponse {
        id: game.id,
        display_name: game.display_name,
        short_name: game.short_name,
        description: game.description,
        icon_url: game.icon_url,
        team_size_default: game.team_size_default,
        status: game.status,
        is_featured: game.is_featured,
    };

    Ok(Json(DataResponse::new(response, request_id)))
}
