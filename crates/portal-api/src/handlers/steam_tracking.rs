//! Steam tracking handlers (user-facing).

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::SteamTrackingState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use portal_domain::entities::steam_tracking::CreateSteamTrackingCommand;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// DTOs
// =============================================================================

/// Request to register for steam tracking.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct RegisterSteamTrackingRequest {
    /// The game auth code (format: XXXX-XXXXX-XXXX).
    #[validate(length(min = 8, max = 32, message = "auth code must be 8-32 characters"))]
    pub game_auth_code: String,
    /// Game slug (e.g. "cs2"). Defaults to "cs2" if omitted.
    #[serde(default = "default_game_slug")]
    #[validate(length(min = 1, max = 32, message = "game slug must be 1-32 characters"))]
    pub game_slug: String,
    /// Most recent CS2 match share code (e.g. CSGO-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx).
    /// Used as the starting cursor for the poller to discover newer matches.
    #[validate(length(max = 64, message = "share code must be at most 64 characters"))]
    pub initial_share_code: Option<String>,
}

fn default_game_slug() -> String {
    "cs2".to_string()
}

/// Request to update steam tracking auth code.
#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct UpdateSteamTrackingRequest {
    /// The new game auth code (format: XXXX-XXXXX-XXXX).
    #[validate(length(min = 8, max = 32, message = "auth code must be 8-32 characters"))]
    pub game_auth_code: String,
}

/// Steam tracking status response.
#[derive(Debug, Serialize, ToSchema)]
pub struct SteamTrackingResponse {
    pub id: String,
    pub game_id: String,
    pub steam_id_64: i64,
    pub game_auth_code_prefix: String,
    pub last_known_code: Option<String>,
    pub is_active: bool,
    pub poll_errors: i32,
    pub last_poll_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
}

impl SteamTrackingResponse {
    fn from_entity(t: &portal_domain::entities::steam_tracking::SteamTracking) -> Self {
        // Mask the auth code: show first 4 chars + masked remainder
        let prefix = if t.game_auth_code.len() >= 4 {
            format!("{}...", &t.game_auth_code[..4])
        } else {
            "****".to_string()
        };

        Self {
            id: t.id.to_string(),
            game_id: t.game_id.to_string(),
            steam_id_64: t.steam_id_64,
            game_auth_code_prefix: prefix,
            last_known_code: t.last_known_code.clone(),
            is_active: t.is_active,
            poll_errors: t.poll_errors,
            last_poll_at: t.last_poll_at.map(|dt| dt.to_rfc3339()),
            last_error: t.last_error.clone(),
            created_at: t.created_at.to_rfc3339(),
        }
    }
}

// =============================================================================
// Handlers
// =============================================================================

/// Register for steam match tracking.
#[utoipa::path(
    post,
    path = "/v1/players/me/steam-tracking",
    request_body = RegisterSteamTrackingRequest,
    responses(
        (status = 201, description = "Tracking registered", body = DataResponse<SteamTrackingResponse>),
        (status = 400, description = "Bad request", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "steam_tracking"
)]
pub async fn register_tracking(
    State(state): State<SteamTrackingState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<RegisterSteamTrackingRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<SteamTrackingResponse>>)> {
    let request_id = get_request_id(&headers);

    // Resolve game slug to game_id
    let game = state
        .game_repo
        .find_by_slug(&req.game_slug)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", req.game_slug)))?;

    let game_id = portal_core::GameId::from(game.id);

    // Get player's steam_id_64
    let player = state
        .player_service
        .get_player(user.player_id)
        .await
        .map_err(|e| ApiError::from(e))?;

    let steam_id_64 = player
        .steam_id
        .as_ref()
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| ApiError::bad_request("Player must have a linked Steam ID"))?;

    let tracking = state
        .steam_tracking_service
        .register(CreateSteamTrackingCommand {
            player_id: user.player_id,
            game_id,
            steam_id_64,
            game_auth_code: req.game_auth_code,
            initial_share_code: req.initial_share_code,
        })
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            SteamTrackingResponse::from_entity(&tracking),
            request_id,
        )),
    ))
}

/// Get current tracking status.
#[utoipa::path(
    get,
    path = "/v1/players/me/steam-tracking",
    responses(
        (status = 200, description = "Current tracking status", body = DataResponse<SteamTrackingResponse>),
        (status = 404, description = "No tracking registered", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "steam_tracking"
)]
pub async fn get_tracking(
    State(state): State<SteamTrackingState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
) -> ApiResult<Json<DataResponse<SteamTrackingResponse>>> {
    let request_id = get_request_id(&headers);

    // Default to CS2 — user can add query param for other games later
    let game = state
        .game_repo
        .find_by_slug("cs2")
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::internal("CS2 game not found in database"))?;

    let game_id = portal_core::GameId::from(game.id);

    let tracking = state
        .steam_tracking_service
        .get_for_player(user.player_id, game_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("No steam tracking registered"))?;

    Ok(Json(DataResponse::new(
        SteamTrackingResponse::from_entity(&tracking),
        request_id,
    )))
}

/// Update tracking auth code.
#[utoipa::path(
    patch,
    path = "/v1/players/me/steam-tracking",
    request_body = UpdateSteamTrackingRequest,
    responses(
        (status = 200, description = "Tracking updated", body = DataResponse<SteamTrackingResponse>),
        (status = 404, description = "No tracking registered", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "steam_tracking"
)]
pub async fn update_tracking(
    State(state): State<SteamTrackingState>,
    headers: HeaderMap,
    user: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<UpdateSteamTrackingRequest>,
) -> ApiResult<Json<DataResponse<SteamTrackingResponse>>> {
    let request_id = get_request_id(&headers);

    let game = state
        .game_repo
        .find_by_slug("cs2")
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::internal("CS2 game not found in database"))?;

    let game_id = portal_core::GameId::from(game.id);

    let tracking = state
        .steam_tracking_service
        .get_for_player(user.player_id, game_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("No steam tracking registered"))?;

    let updated = state
        .steam_tracking_service
        .update_auth_code(tracking.id, user.player_id, &req.game_auth_code)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(DataResponse::new(
        SteamTrackingResponse::from_entity(&updated),
        request_id,
    )))
}

/// Delete tracking (stop tracking).
#[utoipa::path(
    delete,
    path = "/v1/players/me/steam-tracking",
    responses(
        (status = 204, description = "Tracking deleted"),
        (status = 404, description = "No tracking registered", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "steam_tracking"
)]
pub async fn delete_tracking(
    State(state): State<SteamTrackingState>,
    user: AuthenticatedUser,
) -> ApiResult<StatusCode> {
    let game = state
        .game_repo
        .find_by_slug("cs2")
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::internal("CS2 game not found in database"))?;

    let game_id = portal_core::GameId::from(game.id);

    let tracking = state
        .steam_tracking_service
        .get_for_player(user.player_id, game_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("No steam tracking registered"))?;

    state
        .steam_tracking_service
        .delete(tracking.id, user.player_id)
        .await
        .map_err(ApiError::from)?;

    Ok(StatusCode::NO_CONTENT)
}
