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

/// Constant-time equality over the hex digests (§9). Digest comparison is
/// already preimage-hard, but there is no reason to leak length/prefix
/// timing either.
fn token_matches(provided_raw: &str, stored_hash: &str) -> bool {
    let provided = hash_token(provided_raw);
    let a = provided.as_bytes();
    let b = stored_hash.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

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

    if !token_matches(token, &reservation.config_token_hash) {
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

    if !token_matches(token, &reservation.event_token_hash) {
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

// =============================================================================
// Demo upload (`matchzy_demo_upload_url` target, §6.5)
// =============================================================================

/// Receive a raw `.dem` upload from MatchZy.
///
/// Auth: the SERVER-scoped demo token (minted at enrollment; lives in the
/// permanent MatchZy config — per-match cvars would race the series-end
/// cvar restore, §6.3). Correlation headers: `MatchZy-FileName`,
/// `MatchZy-MatchId`, `MatchZy-MapNumber` (0-indexed).
///
/// The body is buffered (route-level 1 GiB cap): CS2 demos run
/// 100–300 MB and this is a single-box deployment; revisit with S3
/// multipart streaming if memory pressure ever shows.
pub async fn post_demo(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<axum::http::StatusCode> {
    let token =
        bearer_token(&headers).ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
    let server = state
        .game_server_registry
        .find_by_demo_token(&hash_token(token))
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::unauthorized("invalid demo token"))?;

    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let file_name = header("matchzy-filename")
        .map(sanitize_demo_filename)
        .ok_or_else(|| ApiError::bad_request("missing MatchZy-FileName header"))?;
    let matchzy_id: Option<i64> = header("matchzy-matchid").and_then(|v| v.parse().ok());
    let map_number: Option<i32> = header("matchzy-mapnumber").and_then(|v| v.parse().ok());

    if body.is_empty() {
        return Err(ApiError::bad_request("empty demo upload"));
    }
    let size = i64::try_from(body.len()).unwrap_or(i64::MAX);

    let stored = crate::observability::track_s3(
        "demos",
        "put",
        state
            .demo_upload_storage
            .store(portal_storage::StoreRequest {
                data: body,
                filename: file_name.clone(),
                content_type: "application/octet-stream".to_string(),
                prefix: match matchzy_id {
                    Some(id) => format!("matchzy/{id}"),
                    None => "matchzy/unmatched".to_string(),
                },
                owner_id: Some(server.id.to_string()),
            }),
    )
    .await
    .map_err(|e| ApiError::internal(format!("demo store failed: {e}")))?;

    // Catalog + auto-link into the existing demo pipeline (§6.5).
    let demo = match state
        .demo_service
        .catalog_demo(
            server.game_id,
            file_name.clone(),
            state.demo_upload_bucket.clone(),
            stored.key.clone(),
            Some(size),
        )
        .await
        .map_err(ApiError::from)?
    {
        portal_domain::services::CatalogResult::Created(demo)
        | portal_domain::services::CatalogResult::AlreadyExists(demo) => demo,
    };

    if let Some(matchzy_id) = matchzy_id
        && let Some(reservation) = state
            .server_reservation_repo
            .find_by_matchzy_id(matchzy_id)
            .await
            .map_err(ApiError::from)?
        // §9 blast-radius: a server may only attach demos to ITS OWN
        // match (M4) — the MatchZy-MatchId header is attacker-settable.
        && reservation.server_id == Some(server.id)
    {
        let link = state
            .demo_service
            .link_to_match(
                demo.id,
                reservation.match_id,
                map_number.map(|n| n + 1),
                portal_core::types::DemoLinkType::AutoMatched,
                None,
            )
            .await;
        if let Err(e) = link {
            // Already linked (replayed upload) is fine; anything else is
            // logged but never bounced to MatchZy (no retries anyway).
            tracing::debug!(demo_id = %demo.id, error = %e, "demo link skipped");
        }
        // PUG demos are categorized 'pug' so the tournament stats surfaces
        // never see them (stats separation).
        crate::pug_flow::tag_demo_if_pug(&state, demo.id, reservation.match_id).await;
    }

    tracing::info!(server = %server.name, file = %file_name, size, "demo uploaded");
    Ok(axum::http::StatusCode::OK)
}

/// Keep only the basename, restricted to a safe charset.
fn sanitize_demo_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let safe: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if safe.is_empty() {
        "upload.dem".to_string()
    } else {
        safe
    }
}

// =============================================================================
// Round backups (`matchzy_remote_backup_url` target, §2.3 / Phase 4)
// =============================================================================

/// Receive a MatchZy round-backup JSON (posted after every live round).
///
/// Auth: the per-reservation event token (backups flow DURING the series,
/// so per-match cvars don't race the series-end restore). Stored via the
/// demo storage backend and indexed as a `backup_uploaded` server event.
pub async fn post_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<axum::http::StatusCode> {
    use portal_domain::repositories::ServerEventRepository;

    let token =
        bearer_token(&headers).ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let matchzy_id: i64 = header("matchzy-matchid")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::bad_request("missing MatchZy-MatchId header"))?;
    let reservation = reservation_by_matchzy_id(&state, matchzy_id).await?;
    if !token_matches(token, &reservation.event_token_hash) {
        return Err(ApiError::unauthorized("invalid token"));
    }

    let file_name = header("matchzy-filename")
        .map(sanitize_demo_filename)
        .ok_or_else(|| ApiError::bad_request("missing MatchZy-FileName header"))?;
    let map_number: Option<i32> = header("matchzy-mapnumber").and_then(|v| v.parse().ok());
    let round_number: Option<i32> = header("matchzy-roundnumber").and_then(|v| v.parse().ok());

    let stored = crate::observability::track_s3(
        "demos",
        "put",
        state
            .demo_upload_storage
            .store(portal_storage::StoreRequest {
                data: body,
                filename: file_name.clone(),
                content_type: "application/json".to_string(),
                prefix: format!("matchzy-backups/{matchzy_id}"),
                owner_id: None,
            }),
    )
    .await
    .map_err(|e| ApiError::internal(format!("backup store failed: {e}")))?;

    let Some(server_id) = reservation.server_id else {
        return Err(ApiError::bad_request("reservation has no server"));
    };
    // Indexed as a server event; the dedupe key (type+map+round) keeps one
    // row per round, and the restore endpoint reads the latest.
    let inserted = state
        .server_event_repo
        .insert(portal_domain::repositories::CreateServerEvent {
            reservation_id: Some(reservation.id),
            server_id,
            event_type: "backup_uploaded".to_string(),
            map_number,
            round_number,
            payload: serde_json::json!({
                "filename": file_name,
                "storage_key": stored.key,
            }),
        })
        .await
        .map_err(ApiError::from)?;
    if let Some(event) = inserted {
        // Storage-indexing rows are terminal on arrival.
        let _ = state.server_event_repo.mark_processed(event.id, None).await;
    }
    Ok(axum::http::StatusCode::OK)
}

/// Serve a stored round backup for `matchzy_loadbackup_url`.
///
/// Auth: the reservation's config token (freshly minted by the restore
/// endpoint). Local-storage keys resolve through the uploads dir; S3 keys
/// are proxied via a presigned-equivalent read (the storage backend's
/// public URL is not used — backups contain the full match config).
pub async fn get_backup(
    State(state): State<AppState>,
    Path((matchzy_id, filename)): Path<(i64, String)>,
    headers: HeaderMap,
) -> ApiResult<axum::response::Response> {
    use portal_domain::repositories::ServerEventRepository as _;

    let token =
        bearer_token(&headers).ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
    let reservation = reservation_by_matchzy_id(&state, matchzy_id).await?;
    if !token_matches(token, &reservation.config_token_hash)
        || reservation.config_token_expires_at <= Utc::now()
    {
        return Err(ApiError::unauthorized("invalid or expired token"));
    }
    let wanted = sanitize_demo_filename(&filename);

    // Resolve the storage key from the indexed backup events.
    let key = state
        .server_event_repo
        .find_backup_key(reservation.id, &wanted)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("backup not found"))?;

    let bytes =
        crate::observability::track_s3("demos", "get", state.demo_upload_storage.read(&key))
            .await
            .map_err(|e| ApiError::internal(format!("backup read failed: {e}")))?;
    Ok(axum::response::Response::builder()
        .header("content-type", "application/json")
        .body(axum::body::Body::from(bytes))
        .unwrap_or_default())
}
