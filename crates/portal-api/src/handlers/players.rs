//! Player handlers.

use crate::dto::common::{DataResponse, PaginatedResponse, PaginationParams};
use crate::dto::requests::UpdatePlayerProfileRequest;
use crate::dto::responses::{PlayerResponse, PlayerSearchResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::handlers::player_game_profiles::build_stats_context;
use crate::state::PlayerState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use portal_core::{GameId, PlayerId};
use portal_domain::repositories::{PlayerSearchFilters, UpdatePlayer};
use serde::Deserialize;
use std::collections::HashMap;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Query parameters for player search.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PlayerSearchParams {
    /// Search query for display name.
    #[serde(default)]
    pub q: String,

    /// Filter by game ID (UUID string).
    pub game_id: Option<String>,

    /// Filter by team status: "has_team", "no_team", or "lft".
    pub team_status: Option<String>,

    /// Filter by ISO 3166-1 alpha-2 country code.
    pub country_code: Option<String>,

    /// Page number (1-indexed).
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page.
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

const fn default_page() -> u32 {
    1
}

const fn default_per_page() -> u32 {
    20
}

impl PlayerSearchParams {
    fn offset(&self) -> i64 {
        i64::from((self.page.saturating_sub(1)) * self.per_page)
    }

    fn limit(&self) -> i64 {
        i64::from(self.per_page.min(100))
    }

    const fn as_pagination(&self) -> PaginationParams {
        PaginationParams {
            page: self.page,
            per_page: self.per_page,
        }
    }
}

/// Search for players.
#[utoipa::path(
    get,
    path = "/v1/players",
    params(
        PlayerSearchParams,
    ),
    responses(
        (status = 200, description = "List of players", body = PaginatedResponse<PlayerSearchResponse>),
    ),
    tag = "players"
)]
pub async fn search_players(
    State(state): State<PlayerState>,
    headers: HeaderMap,
    Query(params): Query<PlayerSearchParams>,
) -> ApiResult<Json<PaginatedResponse<PlayerSearchResponse>>> {
    let request_id = get_request_id(&headers);

    let limit = params.limit();
    let offset = params.offset();
    let pagination = params.as_pagination();

    let game_id = params
        .game_id
        .map(|s| s.parse::<GameId>())
        .transpose()
        .map_err(|_| ApiError::bad_request("Invalid game_id format — expected UUID"))?;

    let filters = PlayerSearchFilters {
        query: params.q,
        game_id,
        team_status: params.team_status,
        country_code: params.country_code,
    };

    let result = state
        .player_service
        .search_players(&filters, limit, offset)
        .await?;

    // Enrich with display stats when game_id filter is provided
    let players: Vec<PlayerSearchResponse> = if let Some(game_id) = game_id {
        // Resolve game to get plugin_id
        let game = state
            .game_repo
            .find_by_id(game_id.as_uuid())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

        if let Some(game) = game {
            let plugin = state.plugin_manager.get(&game.plugin_id);

            // Batch-fetch profiles for all players in the result set
            let player_ids: Vec<PlayerId> = result.players.iter().map(|p| p.id).collect();
            let profiles = state
                .player_game_profile_service
                .find_by_players_and_game(&player_ids, game_id)
                .await
                .unwrap_or_default();

            // Build a lookup map: player_id -> profile
            let mut profile_map: HashMap<PlayerId, _> =
                profiles.into_iter().map(|p| (p.player_id, p)).collect();

            // Build responses with display stats
            let mut responses = Vec::with_capacity(result.players.len());
            for player in result.players {
                if let Some(profile) = profile_map.remove(&player.id) {
                    let context = build_stats_context(&state, &profile).await;
                    let display_stats = plugin
                        .as_ref()
                        .map(|p| p.format_player_stats(&profile.game_specific_stats, &context))
                        .unwrap_or_default();
                    responses.push(PlayerSearchResponse::with_display_stats(
                        player,
                        display_stats,
                    ));
                } else {
                    responses.push(PlayerSearchResponse::from(player));
                }
            }
            responses
        } else {
            result
                .players
                .into_iter()
                .map(PlayerSearchResponse::from)
                .collect()
        }
    } else {
        result
            .players
            .into_iter()
            .map(PlayerSearchResponse::from)
            .collect()
    };

    Ok(Json(PaginatedResponse::new(
        players,
        &pagination,
        result.total as u64,
        request_id,
    )))
}

/// Get a player by ID.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}",
    params(
        ("player_id" = String, Path, description = "Player ID")
    ),
    responses(
        (status = 200, description = "Player found", body = DataResponse<PlayerResponse>),
        (status = 404, description = "Player not found", body = ApiError),
    ),
    tag = "players"
)]
pub async fn get_player(
    State(state): State<PlayerState>,
    headers: HeaderMap,
    Path(player_id): Path<PlayerId>,
) -> ApiResult<Json<DataResponse<PlayerResponse>>> {
    let request_id = get_request_id(&headers);

    let player = state.player_service.get_player(player_id).await?;

    Ok(Json(DataResponse::new(
        PlayerResponse::from(player),
        request_id,
    )))
}

// TODO: Re-add get_player_teams handler with league team memberships
// This will be implemented as part of the league team API

/// Get the current authenticated player's profile.
#[utoipa::path(
    get,
    path = "/v1/players/me",
    responses(
        (status = 200, description = "Current player's profile", body = DataResponse<PlayerResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "players"
)]
pub async fn get_my_profile(
    State(state): State<PlayerState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<PlayerResponse>>> {
    let request_id = get_request_id(&headers);

    let player = state.player_service.get_player(auth.player_id).await?;

    Ok(Json(DataResponse::new(
        PlayerResponse::from(player),
        request_id,
    )))
}

/// Update the current authenticated player's profile.
#[utoipa::path(
    patch,
    path = "/v1/players/me",
    request_body = UpdatePlayerProfileRequest,
    responses(
        (status = 200, description = "Profile updated", body = DataResponse<PlayerResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Display name already taken", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "players"
)]
pub async fn update_my_profile(
    State(state): State<PlayerState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    ValidatedJson(request): ValidatedJson<UpdatePlayerProfileRequest>,
) -> ApiResult<Json<DataResponse<PlayerResponse>>> {
    let request_id = get_request_id(&headers);

    let update = UpdatePlayer::from(request);

    let player = state
        .player_service
        .update_profile(auth.player_id, update)
        .await?;

    Ok(Json(DataResponse::new(
        PlayerResponse::from(player),
        request_id,
    )))
}
