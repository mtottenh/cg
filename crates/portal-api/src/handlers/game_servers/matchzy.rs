//! MatchZy-facing endpoints: config fetch + event webhook.
//!
//! NOT annotated with `#[utoipa::path]` — machine-facing, authenticated by
//! per-reservation bearer tokens (MatchZy supports exactly one custom
//! header pair), excluded from the public OpenAPI spec like
//! `handlers/internal.rs`. Design: docs/matchzy-integration.md §6.2, §6.4.

use crate::error::{ApiError, ApiResult};
use crate::game_server_flow;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use chrono::Utc;
use portal_domain::entities::ServerReservation;
use portal_domain::repositories::ServerReservationRepository;
use portal_domain::services::game_server::hash_token;

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

async fn reservation_by_matchzy_id(
    state: &AppState,
    matchzy_id: i64,
) -> Result<ServerReservation, ApiError> {
    state
        .server_reservation_repo
        .find_by_matchzy_id(matchzy_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("unknown match id"))
}

/// Serve the MatchZy match config (`matchzy_loadmatch_url` target).
///
/// Auth: the per-load-attempt config token, TTL-gated. The config JSON
/// contains the event token and passwords — treat as a secret.
pub async fn get_match_config(
    State(state): State<AppState>,
    Path(matchzy_id): Path<i64>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let token =
        bearer_token(&headers).ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
    let reservation = reservation_by_matchzy_id(&state, matchzy_id).await?;

    if hash_token(token) != reservation.config_token_hash {
        return Err(ApiError::unauthorized("invalid config token"));
    }
    if !reservation.config_fetchable(Utc::now()) {
        return Err(ApiError::forbidden(
            "config token expired or reservation is not loading",
        ));
    }
    let config = reservation
        .match_config
        .clone()
        .ok_or_else(|| ApiError::internal("reservation has no stored config"))?;

    state
        .server_reservation_repo
        .mark_config_fetched(reservation.id, Utc::now())
        .await
        .map_err(ApiError::from)?;

    tracing::info!(matchzy_id, reservation_id = %reservation.id, "match config served");
    Ok(Json(config))
}

/// MatchZy remote-log webhook (`matchzy_remote_log_url` target).
///
/// Must answer fast (MatchZy: 15s timeout, no retries): insert + process,
/// always 200 on auth success — processing errors are recorded on the
/// event row, never bounced to MatchZy.
pub async fn post_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> ApiResult<axum::http::StatusCode> {
    let token =
        bearer_token(&headers).ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;

    // Events carry the integer matchid — the reservation lookup key.
    let matchzy_id = payload
        .get("matchid")
        .and_then(|m| m.as_i64().or_else(|| m.as_str()?.parse().ok()))
        .ok_or_else(|| ApiError::bad_request("event has no matchid"))?;
    let reservation = reservation_by_matchzy_id(&state, matchzy_id).await?;

    if hash_token(token) != reservation.event_token_hash {
        return Err(ApiError::unauthorized("invalid event token"));
    }
    if reservation.status.is_terminal() {
        // Late events after completion/cancellation: acknowledge, ignore.
        tracing::debug!(matchzy_id, "event after terminal reservation ignored");
        return Ok(axum::http::StatusCode::OK);
    }

    game_server_flow::ingest_server_event(&state, &reservation, payload)
        .await
        .map_err(ApiError::from)?;
    Ok(axum::http::StatusCode::OK)
}
