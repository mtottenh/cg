//! Game handlers.

use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::{
    AddMapRequest, SetMapPoolRequest, SetRankTiersRequest, UpdateGameRequest, UpdateMapRequest,
    UpdateTeamSizeRequest,
};
use crate::dto::responses::{
    GameDetailResponse, GameSummaryResponse, MapInfoResponse, RankTierResponse, TeamSizeConfig,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_db::entities::{GameRow, UpdateGame};

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
            id: g.id.to_string(),
            slug: g.slug,
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

    // Fetch from database by slug
    let game = state
        .game_repo
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

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
                .map(std::string::ToString::to_string)
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
        id: game.id.to_string(),
        slug: game.slug.clone(),
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
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

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
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

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
                .map(std::string::ToString::to_string)
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
        id: game.id.to_string(),
        slug: game.slug.clone(),
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
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

    // Get plugin
    let plugin = state.plugin_manager.get(&game.plugin_id);

    if let Some(p) = &plugin {
        if !p.supports_custom_map_pool() {
            return Err(ApiError::bad_request(
                "This game does not support custom map pools",
            ));
        }
    }

    // Load available maps from DB first, fall back to plugin defaults
    let all_maps = load_available_maps(&game, &plugin);

    // Validate all requested map IDs exist in the available catalog
    for map_id in &req.map_ids {
        if !all_maps.iter().any(|m| m.id == *map_id) {
            return Err(ApiError::bad_request(format!("Unknown map: {map_id}")));
        }
    }

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
        id: game.id.to_string(),
        slug: game.slug.clone(),
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
        id: game.id.to_string(),
        slug: game.slug.clone(),
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

// ============================================================================
// HELPERS
// ============================================================================

/// Load available maps from DB (if non-empty) or fall back to plugin defaults.
fn load_available_maps(
    game: &GameRow,
    plugin: &Option<std::sync::Arc<dyn portal_plugins::GamePlugin>>,
) -> Vec<MapInfoResponse> {
    if let Some(arr) = game.available_maps.as_array() {
        if !arr.is_empty() {
            return serde_json::from_value(game.available_maps.clone()).unwrap_or_default();
        }
    }
    if let Some(p) = plugin {
        p.available_maps().into_iter().map(Into::into).collect()
    } else {
        vec![]
    }
}

/// Check admin.games.manage permission.
async fn require_games_admin(state: &AppState, auth: &AuthenticatedUser) -> ApiResult<()> {
    let is_admin = state
        .permission_repo
        .user_has_permission(auth.user_id, "admin.games.manage")
        .await
        .unwrap_or(false);

    if !is_admin {
        return Err(ApiError::forbidden(
            "Admin permission required to manage games",
        ));
    }
    Ok(())
}

// ============================================================================
// MAP CATALOG ENDPOINTS
// ============================================================================

/// Add a new map to a game's available maps catalog (admin only).
#[utoipa::path(
    post,
    path = "/v1/games/{game_id}/maps/catalog",
    params(
        ("game_id" = String, Path, description = "Game ID (slug)")
    ),
    request_body = AddMapRequest,
    responses(
        (status = 200, description = "Map added to catalog", body = DataResponse<Vec<MapInfoResponse>>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game not found", body = ApiError),
        (status = 409, description = "Map ID already exists", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn add_map(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(game_id): Path<String>,
    ValidatedJson(req): ValidatedJson<AddMapRequest>,
) -> ApiResult<Json<DataResponse<Vec<MapInfoResponse>>>> {
    let request_id = get_request_id(&headers);
    require_games_admin(&state, &auth).await?;

    let game = state
        .game_repo
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

    let plugin = state.plugin_manager.get(&game.plugin_id);
    let mut maps = load_available_maps(&game, &plugin);

    // Check map ID doesn't already exist
    if maps.iter().any(|m| m.id == req.id) {
        return Err(ApiError::conflict(format!(
            "Map with ID '{}' already exists",
            req.id
        )));
    }

    // Append new map
    maps.push(MapInfoResponse {
        id: req.id,
        display_name: req.display_name,
        image_url: req.image_url,
        game_modes: req.game_modes,
        external_id: req.external_id,
        external_url: req.external_url,
    });

    // Persist to DB
    let maps_json = serde_json::to_value(&maps).unwrap_or_default();
    let update = UpdateGame {
        available_maps: Some(maps_json),
        ..Default::default()
    };
    let _ = state.game_repo.update(&game_id, update).await?;

    Ok(Json(DataResponse::new(maps, request_id)))
}

/// Update an existing map's metadata (admin only).
#[utoipa::path(
    patch,
    path = "/v1/games/{game_id}/maps/catalog/{map_id}",
    params(
        ("game_id" = String, Path, description = "Game ID (slug)"),
        ("map_id" = String, Path, description = "Map ID")
    ),
    request_body = UpdateMapRequest,
    responses(
        (status = 200, description = "Map updated", body = DataResponse<MapInfoResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game or map not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn update_map(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((game_id, map_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<UpdateMapRequest>,
) -> ApiResult<Json<DataResponse<MapInfoResponse>>> {
    let request_id = get_request_id(&headers);
    require_games_admin(&state, &auth).await?;

    let game = state
        .game_repo
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

    let plugin = state.plugin_manager.get(&game.plugin_id);
    let mut maps = load_available_maps(&game, &plugin);

    // Find and update the map
    let map = maps
        .iter_mut()
        .find(|m| m.id == map_id)
        .ok_or_else(|| ApiError::not_found(format!("Map not found: {map_id}")))?;

    if let Some(display_name) = req.display_name {
        map.display_name = display_name;
    }
    if let Some(image_url) = req.image_url {
        map.image_url = Some(image_url);
    }
    if let Some(game_modes) = req.game_modes {
        map.game_modes = game_modes;
    }
    if let Some(external_id) = req.external_id {
        map.external_id = Some(external_id);
    }
    if let Some(external_url) = req.external_url {
        map.external_url = Some(external_url);
    }

    let updated_map = map.clone();

    // Persist to DB
    let maps_json = serde_json::to_value(&maps).unwrap_or_default();
    let update = UpdateGame {
        available_maps: Some(maps_json),
        ..Default::default()
    };
    let _ = state.game_repo.update(&game_id, update).await?;

    Ok(Json(DataResponse::new(updated_map, request_id)))
}

/// Remove a map from a game's available maps catalog (admin only).
#[utoipa::path(
    delete,
    path = "/v1/games/{game_id}/maps/catalog/{map_id}",
    params(
        ("game_id" = String, Path, description = "Game ID (slug)"),
        ("map_id" = String, Path, description = "Map ID")
    ),
    responses(
        (status = 204, description = "Map removed"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game or map not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn remove_map(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((game_id, map_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    require_games_admin(&state, &auth).await?;

    let game = state
        .game_repo
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

    let plugin = state.plugin_manager.get(&game.plugin_id);
    let mut maps = load_available_maps(&game, &plugin);

    let original_len = maps.len();
    maps.retain(|m| m.id != map_id);

    if maps.len() == original_len {
        return Err(ApiError::not_found(format!("Map not found: {map_id}")));
    }

    // Persist to DB
    let maps_json = serde_json::to_value(&maps).unwrap_or_default();
    let update = UpdateGame {
        available_maps: Some(maps_json),
        ..Default::default()
    };
    let _ = state.game_repo.update(&game_id, update).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// RANK TIERS ENDPOINT
// ============================================================================

/// Replace the full set of rank tiers for a game (admin only).
#[utoipa::path(
    put,
    path = "/v1/games/{game_id}/rank-tiers",
    params(
        ("game_id" = String, Path, description = "Game ID (slug)")
    ),
    request_body = SetRankTiersRequest,
    responses(
        (status = 200, description = "Rank tiers updated", body = DataResponse<Vec<RankTierResponse>>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn set_rank_tiers(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(game_id): Path<String>,
    ValidatedJson(req): ValidatedJson<SetRankTiersRequest>,
) -> ApiResult<Json<DataResponse<Vec<RankTierResponse>>>> {
    let request_id = get_request_id(&headers);
    require_games_admin(&state, &auth).await?;

    // Verify game exists
    let _ = state
        .game_repo
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

    // Validate tier ordering and boundaries
    let mut prev_max: Option<i32> = None;
    for tier in &req.rank_tiers {
        if let Some(pm) = prev_max {
            if tier.min_rating <= pm {
                return Err(ApiError::bad_request(format!(
                    "Tier '{}' min_rating ({}) overlaps with previous tier max_rating ({})",
                    tier.id, tier.min_rating, pm
                )));
            }
        }
        if let Some(max) = tier.max_rating {
            if max < tier.min_rating {
                return Err(ApiError::bad_request(format!(
                    "Tier '{}' max_rating ({}) must be >= min_rating ({})",
                    tier.id, max, tier.min_rating
                )));
            }
        }
        prev_max = tier.max_rating;
    }

    // Convert to response DTOs
    let tiers: Vec<RankTierResponse> = req
        .rank_tiers
        .iter()
        .map(|t| RankTierResponse {
            id: t.id.clone(),
            display_name: t.display_name.clone(),
            min_rating: t.min_rating,
            max_rating: t.max_rating,
            color: t.color.clone(),
            icon_url: t.icon_url.clone(),
            order: t.order,
        })
        .collect();

    // Persist to DB
    let tiers_json = serde_json::to_value(&tiers).unwrap_or_default();
    let update = UpdateGame {
        rank_tiers: Some(tiers_json),
        ..Default::default()
    };
    let _ = state.game_repo.update(&game_id, update).await?;

    Ok(Json(DataResponse::new(tiers, request_id)))
}

// ============================================================================
// TEAM SIZE ENDPOINT
// ============================================================================

/// Update team size constraints for a game (admin only).
#[utoipa::path(
    patch,
    path = "/v1/games/{game_id}/team-size",
    params(
        ("game_id" = String, Path, description = "Game ID (slug)")
    ),
    request_body = UpdateTeamSizeRequest,
    responses(
        (status = 200, description = "Team size updated", body = DataResponse<TeamSizeConfig>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - admin role required", body = ApiError),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "games"
)]
pub async fn update_team_size(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(game_id): Path<String>,
    ValidatedJson(req): ValidatedJson<UpdateTeamSizeRequest>,
) -> ApiResult<Json<DataResponse<TeamSizeConfig>>> {
    let request_id = get_request_id(&headers);
    require_games_admin(&state, &auth).await?;

    // Fetch current game to merge with existing values
    let game = state
        .game_repo
        .find_by_slug(&game_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id}")))?;

    let new_min = req.min.unwrap_or(game.team_size_min);
    let new_max = req.max.unwrap_or(game.team_size_max);
    let new_default = req.default.unwrap_or(game.team_size_default);

    // Validate constraints
    if new_min > new_default {
        return Err(ApiError::bad_request(format!(
            "min ({new_min}) must be <= default ({new_default})"
        )));
    }
    if new_default > new_max {
        return Err(ApiError::bad_request(format!(
            "default ({new_default}) must be <= max ({new_max})"
        )));
    }

    let update = UpdateGame {
        team_size_min: req.min,
        team_size_max: req.max,
        team_size_default: req.default,
        ..Default::default()
    };
    let updated = state.game_repo.update(&game_id, update).await?;

    let config = TeamSizeConfig {
        min: updated.team_size_min,
        max: updated.team_size_max,
        default: updated.team_size_default,
    };

    Ok(Json(DataResponse::new(config, request_id)))
}
