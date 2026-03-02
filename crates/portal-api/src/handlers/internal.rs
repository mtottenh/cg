//! Internal API handlers for bot/service endpoints.
//!
//! These endpoints are authenticated with API keys (`AuthenticatedService`)
//! instead of JWT tokens.

use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedService;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use portal_core::permissions::service;
use portal_core::{GameId, SteamTrackingId};
use portal_domain::entities::steam_tracking::UpdatePollResultCommand;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// =============================================================================
// Steam Tracking (internal)
// =============================================================================

/// Query params for fetching active tracking entries.
#[derive(Debug, Deserialize)]
pub struct ActiveTrackingQuery {
    /// Game slug (e.g. "cs2").
    pub game: String,
}

/// Steam tracking entry exposed to bots.
#[derive(Debug, Serialize, ToSchema)]
pub struct InternalSteamTrackingEntry {
    pub id: String,
    pub player_id: String,
    pub steam_id_64: i64,
    pub game_auth_code: String,
    pub last_known_code: Option<String>,
    pub poll_errors: i32,
}

/// Get all active tracking entries for a game.
pub async fn get_active_tracking(
    State(state): State<AppState>,
    service: AuthenticatedService,
    Query(query): Query<ActiveTrackingQuery>,
) -> ApiResult<Json<Vec<InternalSteamTrackingEntry>>> {
    service.require_permission(service::STEAM_TRACKING_READ)?;

    let game = state
        .game_repo
        .find_by_slug(&query.game)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", query.game)))?;

    let game_id = GameId::from(game.id);

    let entries = state
        .steam_tracking_service
        .get_active_for_game(game_id)
        .await
        .map_err(ApiError::from)?;

    let response: Vec<InternalSteamTrackingEntry> = entries
        .iter()
        .map(|t| InternalSteamTrackingEntry {
            id: t.id.to_string(),
            player_id: t.player_id.to_string(),
            steam_id_64: t.steam_id_64,
            game_auth_code: t.game_auth_code.clone(),
            last_known_code: t.last_known_code.clone(),
            poll_errors: t.poll_errors,
        })
        .collect();

    Ok(Json(response))
}

/// Request body for updating a poll result.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePollResultRequest {
    /// The newest share code discovered (if any).
    pub last_known_code: Option<String>,
    /// Error message if the poll failed.
    pub error: Option<String>,
}

/// Update a tracking entry's poll result.
pub async fn update_poll_result(
    State(state): State<AppState>,
    service: AuthenticatedService,
    Path(id): Path<String>,
    Json(req): Json<UpdatePollResultRequest>,
) -> ApiResult<StatusCode> {
    service.require_permission(service::STEAM_TRACKING_WRITE)?;

    let tracking_id: SteamTrackingId = id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tracking ID"))?;

    state
        .steam_tracking_service
        .update_poll_result(
            tracking_id,
            UpdatePollResultCommand {
                last_known_code: req.last_known_code,
                error: req.error,
            },
        )
        .await
        .map_err(ApiError::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// Discovered Matches (internal) — added in Stage 3
// =============================================================================

/// Batch of discovered matches from the poller.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitDiscoveredMatchesRequest {
    pub tracking_id: String,
    pub game: String,
    pub matches: Vec<DiscoveredMatchEntry>,
}

/// A single discovered match.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscoveredMatchEntry {
    pub share_code: String,
    pub match_id: i64,
    pub outcome_id: i64,
    pub token: i32,
}

/// Discovered match response.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiscoveredMatchResponse {
    pub id: String,
    pub share_code: String,
    pub status: String,
}

/// Submit discovered matches (idempotent on share_code).
pub async fn submit_discovered_matches(
    State(state): State<AppState>,
    service: AuthenticatedService,
    Json(req): Json<SubmitDiscoveredMatchesRequest>,
) -> ApiResult<(StatusCode, Json<Vec<DiscoveredMatchResponse>>)> {
    service.require_permission(service::DISCOVERED_MATCHES_WRITE)?;

    let tracking_id: SteamTrackingId = req
        .tracking_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid tracking_id"))?;

    let game = state
        .game_repo
        .find_by_slug(&req.game)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", req.game)))?;

    let game_id = GameId::from(game.id);

    let mut results = Vec::new();
    for entry in &req.matches {
        let result = state
            .discovered_match_service
            .submit(
                tracking_id,
                game_id,
                &entry.share_code,
                entry.match_id,
                entry.outcome_id,
                entry.token,
            )
            .await
            .map_err(ApiError::from)?;

        results.push(DiscoveredMatchResponse {
            id: result.id.to_string(),
            share_code: result.share_code.clone(),
            status: result.status.clone(),
        });
    }

    Ok((StatusCode::CREATED, Json(results)))
}

// =============================================================================
// Enricher endpoints — added in Stage 4
// =============================================================================

/// Query params for fetching pending matches.
#[derive(Debug, Deserialize)]
pub struct PendingMatchesQuery {
    pub game: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

/// Pending match for the enricher.
#[derive(Debug, Serialize, ToSchema)]
pub struct PendingMatchResponse {
    pub id: String,
    pub share_code: String,
    pub match_id: i64,
    pub outcome_id: i64,
    pub token: i32,
    pub retry_count: i32,
}

/// Get pending matches for enrichment.
pub async fn get_pending_matches(
    State(state): State<AppState>,
    service: AuthenticatedService,
    Query(query): Query<PendingMatchesQuery>,
) -> ApiResult<Json<Vec<PendingMatchResponse>>> {
    service.require_permission(service::DISCOVERED_MATCHES_READ)?;

    let game = state
        .game_repo
        .find_by_slug(&query.game)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", query.game)))?;

    let game_id = GameId::from(game.id);

    let matches = state
        .discovered_match_service
        .get_pending(game_id, query.limit)
        .await
        .map_err(ApiError::from)?;

    let response: Vec<PendingMatchResponse> = matches
        .iter()
        .map(|m| PendingMatchResponse {
            id: m.id.to_string(),
            share_code: m.share_code.clone(),
            match_id: m.match_id,
            outcome_id: m.outcome_id,
            token: m.token,
            retry_count: m.retry_count,
        })
        .collect();

    Ok(Json(response))
}

/// Claim a match for enrichment (atomic, prevents double-processing).
pub async fn claim_match(
    State(state): State<AppState>,
    service: AuthenticatedService,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    service.require_permission(service::DISCOVERED_MATCHES_WRITE)?;

    let match_id: portal_core::DiscoveredMatchId = id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID"))?;

    let claimed = state
        .discovered_match_service
        .claim(match_id)
        .await
        .map_err(ApiError::from)?;

    if claimed {
        Ok(StatusCode::OK)
    } else {
        Ok(StatusCode::CONFLICT)
    }
}

/// Request body for submitting enriched match data.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EnrichedMatchRequest {
    pub gc_data: serde_json::Value,
    pub demo_url: Option<String>,
}

/// Submit enriched match data from GC.
pub async fn submit_enriched(
    State(state): State<AppState>,
    service: AuthenticatedService,
    Path(id): Path<String>,
    Json(req): Json<EnrichedMatchRequest>,
) -> ApiResult<StatusCode> {
    service.require_permission(service::DISCOVERED_MATCHES_WRITE)?;

    let match_id: portal_core::DiscoveredMatchId = id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID"))?;

    state
        .discovered_match_service
        .mark_enriched(match_id, req.gc_data, req.demo_url)
        .await
        .map_err(ApiError::from)?;

    Ok(StatusCode::OK)
}

/// Request body for marking a match as failed.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FailedMatchRequest {
    pub error: String,
}

/// Mark a match enrichment as failed.
pub async fn mark_failed(
    State(state): State<AppState>,
    service: AuthenticatedService,
    Path(id): Path<String>,
    Json(req): Json<FailedMatchRequest>,
) -> ApiResult<StatusCode> {
    service.require_permission(service::DISCOVERED_MATCHES_WRITE)?;

    let match_id: portal_core::DiscoveredMatchId = id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID"))?;

    state
        .discovered_match_service
        .mark_failed(match_id, &req.error)
        .await
        .map_err(ApiError::from)?;

    Ok(StatusCode::OK)
}
