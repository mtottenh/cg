//! Player game profile handlers.

use crate::dto::common::DataResponse;
use crate::dto::responses::PlayerGameProfileResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use portal_core::{GameId, PlayerId};
use portal_domain::entities::PlayerGameProfile;

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
