//! Player game profile handlers.

use crate::dto::common::DataResponse;
use crate::dto::requests::SubmitRatingRequest;
use crate::dto::responses::{PlayerGameProfileResponse, PlayerRatingHistoryResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::{GameId, PlayerId};
use portal_domain::entities::PlayerGameProfile;
use portal_domain::repositories::player_rating_history::CreatePlayerRatingHistory;
use portal_domain::repositories::PlayerRatingHistoryRepository;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Resolve profiles to responses, looking up plugins for display stats.
async fn profiles_to_responses(
    state: &AppState,
    profiles: Vec<PlayerGameProfile>,
) -> Vec<PlayerGameProfileResponse> {
    let mut responses = Vec::with_capacity(profiles.len());
    for profile in profiles {
        // Look up the game to get plugin_id for display stat formatting
        let display_stats = match state.game_repo.find_by_id(profile.game_id.as_uuid()).await {
            Ok(Some(game)) => state
                .plugin_manager
                .get(&game.plugin_id)
                .map(|plugin| plugin.format_player_stats(&profile.game_specific_stats))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        responses.push(PlayerGameProfileResponse::from_profile_with_stats(profile, display_stats));
    }
    responses
}

/// List all game profiles for a player.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/games",
    params(
        ("player_id" = String, Path, description = "Player ID"),
    ),
    responses(
        (status = 200, description = "List of game profiles", body = DataResponse<Vec<PlayerGameProfileResponse>>),
        (status = 404, description = "Player not found", body = ApiError),
    ),
    tag = "players"
)]
pub async fn list_player_game_profiles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(player_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<PlayerGameProfileResponse>>>> {
    let request_id = get_request_id(&headers);

    let player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    // Verify player exists
    state.player_service.get_player(player_id).await?;

    let profiles = state
        .player_game_profile_service
        .list_profiles(player_id)
        .await?;

    let responses = profiles_to_responses(&state, profiles).await;

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Get a specific game profile for a player.
///
/// The `game_id` path parameter accepts either a game slug (e.g., "cs2") or a game UUID.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/games/{game_id}",
    params(
        ("player_id" = String, Path, description = "Player ID"),
        ("game_id" = String, Path, description = "Game slug (e.g., cs2) or UUID"),
    ),
    responses(
        (status = 200, description = "Game profile found", body = DataResponse<PlayerGameProfileResponse>),
        (status = 404, description = "Profile or player not found", body = ApiError),
    ),
    tag = "players"
)]
pub async fn get_player_game_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((player_id, game_id_or_slug)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<PlayerGameProfileResponse>>> {
    let request_id = get_request_id(&headers);

    let player_id: PlayerId = player_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    // Resolve game_id: try parsing as UUID first, fall back to slug lookup
    let (game_id, plugin_id) = if let Ok(uuid) = game_id_or_slug.parse::<uuid::Uuid>() {
        let game = state
            .game_repo
            .find_by_id(uuid)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id_or_slug}")))?;
        (GameId::from(game.id), game.plugin_id)
    } else {
        let game = state
            .game_repo
            .find_by_slug(&game_id_or_slug)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id_or_slug}")))?;
        (GameId::from(game.id), game.plugin_id)
    };

    let profile = state
        .player_game_profile_service
        .get_profile(player_id, game_id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "No game profile found for player {player_id} in game {game_id_or_slug}"
            ))
        })?;

    let display_stats = state
        .plugin_manager
        .get(&plugin_id)
        .map(|plugin| plugin.format_player_stats(&profile.game_specific_stats))
        .unwrap_or_default();

    Ok(Json(DataResponse::new(
        PlayerGameProfileResponse::from_profile_with_stats(profile, display_stats),
        request_id,
    )))
}

/// Resolve a game_id string (UUID or slug) to a `GameId` and plugin_id.
async fn resolve_game(
    state: &AppState,
    game_id_or_slug: &str,
) -> Result<(GameId, String), ApiError> {
    if let Ok(uuid) = game_id_or_slug.parse::<uuid::Uuid>() {
        let game = state
            .game_repo
            .find_by_id(uuid)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id_or_slug}")))?;
        Ok((GameId::from(game.id), game.plugin_id))
    } else {
        let game = state
            .game_repo
            .find_by_slug(game_id_or_slug)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id_or_slug}")))?;
        Ok((GameId::from(game.id), game.plugin_id))
    }
}

/// Query parameters for rating history.
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct RatingHistoryQuery {
    /// Maximum number of entries to return (default: 100).
    #[schema(example = 100)]
    pub limit: Option<i64>,
}

/// List game profiles for the authenticated player.
#[utoipa::path(
    get,
    path = "/v1/players/me/games",
    responses(
        (status = 200, description = "List of game profiles", body = DataResponse<Vec<PlayerGameProfileResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "players"
)]
pub async fn get_my_game_profiles(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<Vec<PlayerGameProfileResponse>>>> {
    let request_id = get_request_id(&headers);

    let profiles = state
        .player_game_profile_service
        .list_profiles(auth.player_id)
        .await?;

    let responses = profiles_to_responses(&state, profiles).await;

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Submit a rating update for a player's game profile.
///
/// Used by external services (e.g., steam bot) to update a player's
/// in-game rating. Requires admin permission.
#[utoipa::path(
    post,
    path = "/v1/players/{player_id}/games/{game_id}/rating",
    params(
        ("player_id" = String, Path, description = "Player ID"),
        ("game_id" = String, Path, description = "Game slug (e.g., cs2) or UUID"),
    ),
    request_body = SubmitRatingRequest,
    responses(
        (status = 201, description = "Rating updated", body = DataResponse<PlayerGameProfileResponse>),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Player or game not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "players"
)]
pub async fn submit_player_rating(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((player_id_str, game_id_or_slug)): Path<(String, String)>,
    Json(body): Json<SubmitRatingRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<PlayerGameProfileResponse>>)> {
    let request_id = get_request_id(&headers);

    // Admin-only endpoint (for bot/service accounts)
    perm_checker
        .require_permission(&auth, portal_core::permissions::admin::SYSTEM_MANAGE)
        .await?;

    let player_id: PlayerId = player_id_str
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    let (game_id, plugin_id) = resolve_game(&state, &game_id_or_slug).await?;

    // Ensure player exists
    state.player_service.get_player(player_id).await?;

    // Compute rank tier from plugin
    let rank_tier = state
        .plugin_manager
        .get(&plugin_id)
        .and_then(|plugin| plugin.rating_to_rank_tier(body.rating))
        .map(|tier| tier.id);

    // Ensure profile exists (find_or_create)
    {
        let profile_repo = portal_db::PgPlayerGameProfileRepository::new(state.db_pool.clone());
        use portal_domain::repositories::PlayerGameProfileRepository;
        profile_repo.find_or_create(player_id, game_id).await?;
    }

    // Update rating + rank tier on the profile
    state
        .player_game_profile_service
        .update_rating(player_id, game_id, body.rating, 0, 0.0, rank_tier)
        .await?;

    // Insert history entry
    state
        .rating_history_repo
        .create(CreatePlayerRatingHistory {
            player_id,
            game_id,
            rating: body.rating,
            source: body.source,
            recorded_at: body.recorded_at,
        })
        .await?;

    // Return updated profile
    let profile = state
        .player_game_profile_service
        .get_profile(player_id, game_id)
        .await?
        .ok_or_else(|| ApiError::internal("Profile not found after update"))?;

    let display_stats = state
        .plugin_manager
        .get(&plugin_id)
        .map(|plugin| plugin.format_player_stats(&profile.game_specific_stats))
        .unwrap_or_default();

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            PlayerGameProfileResponse::from_profile_with_stats(profile, display_stats),
            request_id,
        )),
    ))
}

/// Get rating history for a player in a specific game.
///
/// Returns a chronological (newest-first) list of rating history entries.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/games/{game_id}/rating-history",
    params(
        ("player_id" = String, Path, description = "Player ID"),
        ("game_id" = String, Path, description = "Game slug (e.g., cs2) or UUID"),
        RatingHistoryQuery,
    ),
    responses(
        (status = 200, description = "Rating history", body = DataResponse<Vec<PlayerRatingHistoryResponse>>),
        (status = 404, description = "Player or game not found", body = ApiError),
    ),
    tag = "players"
)]
pub async fn get_player_rating_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((player_id_str, game_id_or_slug)): Path<(String, String)>,
    Query(query): Query<RatingHistoryQuery>,
) -> ApiResult<Json<DataResponse<Vec<PlayerRatingHistoryResponse>>>> {
    let request_id = get_request_id(&headers);

    let player_id: PlayerId = player_id_str
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    let (game_id, _plugin_id) = resolve_game(&state, &game_id_or_slug).await?;

    // Verify player exists
    state.player_service.get_player(player_id).await?;

    let limit = query.limit.or(Some(100));
    let entries = state
        .rating_history_repo
        .list_by_player_and_game(player_id, game_id, limit)
        .await?;

    let responses: Vec<PlayerRatingHistoryResponse> =
        entries.into_iter().map(PlayerRatingHistoryResponse::from).collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}
