//! Internal API handlers for bot/service endpoints.
//!
//! These endpoints are authenticated with API keys (`AuthenticatedService`)
//! instead of JWT tokens.
//!
//! # OpenAPI exclusion
//!
//! Handlers in this module are deliberately **not** annotated with
//! `#[utoipa::path(...)]` and therefore do not appear in the public
//! `/api-docs/openapi.json` spec. Their contract is between the portal
//! server and first-party services (the Steam poller, the CS2 demo
//! scanner) — not a surface we publish for third-party consumers. If a
//! future caller (e.g. a partner integration) needs any of these, lift it
//! out of this module and annotate it like any other public endpoint.

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    BatchCatalogDemosRequest, MarkDemoFailedRequest, PendingDemosQuery, SubmitDemoStatsRequest,
};
use crate::dto::responses::{BatchCatalogErrorResponse, BatchCatalogResultResponse, DemoResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedService;
use crate::state::InternalState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::DateTime;
use portal_core::permissions::service;
use portal_core::{DemoId, GameId, SteamTrackingId};
use portal_domain::entities::demo::{DemoPlayerStats, ParsedDemoMetadata};
use portal_domain::entities::steam_tracking::UpdatePollResultCommand;
use portal_domain::services::DemoPlayerInput;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

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
    State(state): State<InternalState>,
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
    State(state): State<InternalState>,
    service: AuthenticatedService,
    Path(tracking_id): Path<SteamTrackingId>,
    Json(req): Json<UpdatePollResultRequest>,
) -> ApiResult<StatusCode> {
    service.require_permission(service::STEAM_TRACKING_WRITE)?;

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
    State(state): State<InternalState>,
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
    State(state): State<InternalState>,
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

/// Query params for fetching recent enriched matches with demo URLs.
#[derive(Debug, Deserialize)]
pub struct RecentDemoMatchesQuery {
    pub game: String,
    /// Optional: filter by SteamID64 (finds tracking entry, then matches).
    pub steam_id_64: Option<i64>,
    #[serde(default = "default_demo_limit")]
    pub limit: i64,
}

fn default_demo_limit() -> i64 {
    5
}

/// Enriched match with demo URL.
#[derive(Debug, Serialize)]
pub struct EnrichedMatchWithDemoResponse {
    pub id: String,
    pub share_code: String,
    pub demo_url: String,
    pub enriched_at: Option<String>,
}

/// Get recent enriched matches that have a demo URL.
pub async fn get_recent_demo_matches(
    State(state): State<InternalState>,
    service: AuthenticatedService,
    Query(query): Query<RecentDemoMatchesQuery>,
) -> ApiResult<Json<Vec<EnrichedMatchWithDemoResponse>>> {
    service.require_permission(service::DISCOVERED_MATCHES_READ)?;

    let game = state
        .game_repo
        .find_by_slug(&query.game)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found(format!("Game not found: {}", query.game)))?;

    let game_id = GameId::from(game.id);

    // If steam_id_64 is provided, find the tracking entry to get its ID
    let tracking_id = if let Some(steam_id_64) = query.steam_id_64 {
        let entries = state
            .steam_tracking_service
            .get_active_for_game(game_id)
            .await
            .map_err(ApiError::from)?;

        entries
            .into_iter()
            .find(|t| t.steam_id_64 == steam_id_64)
            .map(|t| t.id)
    } else {
        None
    };

    let matches = state
        .discovered_match_service
        .get_recent_with_demo_url(game_id, tracking_id, query.limit)
        .await
        .map_err(ApiError::from)?;

    let response: Vec<EnrichedMatchWithDemoResponse> = matches
        .into_iter()
        .filter_map(|m| {
            m.demo_url.map(|url| EnrichedMatchWithDemoResponse {
                id: m.id.to_string(),
                share_code: m.share_code,
                demo_url: url,
                enriched_at: m.enriched_at.map(|dt| dt.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(response))
}

/// Claim a match for enrichment (atomic, prevents double-processing).
pub async fn claim_match(
    State(state): State<InternalState>,
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

/// Per-player rank data extracted from the demo file.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DemoPlayerRating {
    /// Steam account ID (Steam32).
    pub account_id: u32,
    /// New rank value (CS Rating for Premier, 1-18 for Comp/Wingman).
    pub rank_id: i32,
    /// Rank type: 6=Competitive, 7=Wingman, 11=Premier.
    pub rank_type_id: u32,
    /// Number of competitive wins.
    pub wins: u32,
    /// Rating change from this match.
    pub rank_change: f32,
}

/// Request body for submitting enriched match data.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EnrichedMatchRequest {
    pub gc_data: serde_json::Value,
    pub demo_url: Option<String>,
    /// Per-player rank data extracted from the demo (optional, backward-compatible).
    #[serde(default)]
    pub player_ratings: Option<Vec<DemoPlayerRating>>,
    /// Map name extracted from the demo file (GC often doesn't provide it).
    #[serde(default)]
    pub map_name: Option<String>,
}

/// Submit enriched match data from GC.
pub async fn submit_enriched(
    State(state): State<InternalState>,
    service: AuthenticatedService,
    Path(id): Path<String>,
    Json(req): Json<EnrichedMatchRequest>,
) -> ApiResult<StatusCode> {
    service.require_permission(service::DISCOVERED_MATCHES_WRITE)?;

    let match_id: portal_core::DiscoveredMatchId = id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID"))?;

    // Marker-before-effect fix: apply the per-player effects (ratings +
    // aggregate stats) FIRST, and only write the `enriched` marker LAST, once
    // they have all succeeded. The claimed row already exists (status
    // 'enriching'), so we drive the effects off the SUBMITTED gc_data without
    // finalizing the discovered match. If any effect fails we mark the match
    // 'failed' — never 'enriched' — so `find_pending` retries it instead of
    // silently stranding the missing stats.
    let claimed = state
        .discovered_match_service
        .get(match_id)
        .await
        .map_err(ApiError::from)?;

    let context = portal_domain::entities::discovered_match::DiscoveredMatch {
        gc_data: Some(req.gc_data.clone()),
        demo_url: req.demo_url.clone(),
        ..claimed
    };

    // Apply the effects, short-circuiting on the first failure. Ratings first,
    // then per-player match stats.
    let effect_error: Option<String> = async {
        if let Some(ratings) = req.player_ratings.as_ref() {
            process_demo_ratings(&state, &context, ratings)
                .await
                .map_err(|e| {
                    tracing::warn!(match_id = %context.id, error = %e, "Failed to process demo ratings");
                    e.to_string()
                })?;
        }
        process_match_stats(&state, &context, req.map_name.as_deref())
            .await
            .map_err(|e| {
                tracing::warn!(match_id = %context.id, error = %e, "Failed to process match stats");
                e.to_string()
            })?;
        Ok::<(), String>(())
    }
    .await
    .err();

    if let Some(error) = effect_error {
        // Do NOT finalize: leave the match retryable for the enricher.
        state
            .discovered_match_service
            .mark_failed(match_id, &error)
            .await
            .map_err(ApiError::from)?;
        return Ok(StatusCode::OK);
    }

    // All effects applied — write the marker last.
    state
        .discovered_match_service
        .mark_enriched(match_id, req.gc_data, req.demo_url)
        .await
        .map_err(ApiError::from)?;

    Ok(StatusCode::OK)
}

/// Process per-player rank data from demo extraction.
///
/// For each Premier rating (rank_type_id == 11):
/// 1. Convert Steam32 account_id → SteamID64
/// 2. Look up registered player by steam_id_64
/// 3. Find or create PlayerGameProfile
/// 4. Update current rating + insert history entry
async fn process_demo_ratings(
    state: &InternalState,
    discovered: &portal_domain::entities::discovered_match::DiscoveredMatch,
    ratings: &[DemoPlayerRating],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use portal_domain::repositories::PlayerRatingHistoryRepository;
    use portal_domain::repositories::player_rating_history::CreatePlayerRatingHistory;

    // Only process Premier ratings (rank_type_id == 11) with non-zero rank
    let premier_ratings: Vec<&DemoPlayerRating> = ratings
        .iter()
        .filter(|r| r.rank_type_id == 11 && r.rank_id > 0)
        .collect();

    if premier_ratings.is_empty() {
        return Ok(());
    }

    let game_id = discovered.game_id;

    // Use the actual match time from GC data, falling back to enriched_at/now
    let recorded_at = discovered
        .gc_data
        .as_ref()
        .and_then(|gc| gc.get(0))
        .and_then(|m| m.get("match_time"))
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.to_utc())
        .or(discovered.enriched_at)
        .unwrap_or_else(chrono::Utc::now);

    for rating in &premier_ratings {
        // Convert Steam32 → SteamID64
        let steam_id_64 = i64::from(rating.account_id) + 76561197960265728;

        // Look up player by steam_id_64
        let player = state
            .player_service
            .find_by_steam_id_64(steam_id_64)
            .await?;

        let player_id = match player {
            Some(p) => p.id,
            None => continue, // Not a registered player, skip
        };

        // Ensure profile exists (needed for league/tournament card)
        state
            .player_game_profile_service
            .ensure_profile_exists(player_id, game_id)
            .await?;

        // Insert rating history entry — current/peak rating are derived from
        // this table at query time, so no need to update player_game_profiles
        state
            .rating_history_repo
            .create(CreatePlayerRatingHistory {
                player_id,
                game_id,
                rating: rating.rank_id,
                source: "demo_rank_update".to_string(),
                recorded_at,
                rank_type_id: 11,
                // Match-scoped idempotency key: a re-delivered enrichment
                // dedupes on (player_id, discovered_match_id, source).
                discovered_match_id: Some(discovered.id),
            })
            .await?;

        tracing::info!(
            player_id = %player_id,
            rating = rating.rank_id,
            rank_change = rating.rank_change,
            "Updated player rating from demo rank data"
        );
    }

    Ok(())
}

/// Process per-player match stats from GC data.
///
/// Extracts player stats from `gc_data[0].players`, determines win/loss
/// per player based on team position, and stores match history + aggregate stats.
async fn process_match_stats(
    state: &InternalState,
    discovered: &portal_domain::entities::discovered_match::DiscoveredMatch,
    map_name: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use portal_domain::repositories::PlayerMatchHistoryRepository;
    use portal_domain::repositories::PlayerMmStatsRepository;
    use portal_domain::repositories::player_match_history::CreatePlayerMatchHistory;
    use portal_domain::repositories::player_mm_stats::AccumulateMatchStats;

    let Some(gc_data) = &discovered.gc_data else {
        return Ok(());
    };

    // gc_data is a JSON array of MatchInfo; we want the first entry
    let Some(match_info) = gc_data.get(0) else {
        return Ok(());
    };

    let players = match match_info.get("players").and_then(|p| p.as_array()) {
        Some(p) if !p.is_empty() => p,
        _ => return Ok(()),
    };

    let team_scores: Vec<i64> = match_info
        .get("team_scores")
        .and_then(|ts| ts.as_array())
        .map(|arr| arr.iter().filter_map(serde_json::Value::as_i64).collect())
        .unwrap_or_default();

    let match_time = match_info
        .get("match_time")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.to_utc());

    let duration = match_info
        .get("match_duration_secs")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0) as i32;

    // Map: prefer demo-extracted map_name, fall back to gc_data
    let gc_map = match_info.get("map").and_then(|v| v.as_str()).unwrap_or("");
    let map = map_name
        .filter(|m| !m.is_empty())
        .unwrap_or(gc_map)
        .to_string();

    let game_id = discovered.game_id;

    // Process each player in the GC data.
    // Each player carries a `team` field (1 or 2) set by the enricher from
    // the original protobuf position (indices 0–4 → team 1, 5–9 → team 2).
    for player_val in players {
        let account_id = match player_val
            .get("account_id")
            .and_then(serde_json::Value::as_u64)
        {
            Some(id) if id > 0 => id as u32,
            _ => continue,
        };

        let steam_id_64 = i64::from(account_id) + 76561197960265728;

        let player = state
            .player_service
            .find_by_steam_id_64(steam_id_64)
            .await?;

        let player_id = match player {
            Some(p) => p.id,
            None => continue,
        };

        // Determine team from the explicit `team` field set during GC extraction.
        let team = player_val
            .get("team")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as u8;
        let is_team1 = team == 1;

        let (player_team_score, opponent_score) = if team_scores.len() >= 2 {
            if is_team1 {
                (team_scores[0], team_scores[1])
            } else {
                (team_scores[1], team_scores[0])
            }
        } else {
            (0, 0)
        };

        let match_result_str = match player_team_score.cmp(&opponent_score) {
            std::cmp::Ordering::Greater => "win",
            std::cmp::Ordering::Less => "loss",
            std::cmp::Ordering::Equal => "draw",
        };

        let kills = player_val
            .get("kills")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let deaths = player_val
            .get("deaths")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let assists = player_val
            .get("assists")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let score = player_val
            .get("score")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let headshots = player_val
            .get("headshots")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let mvps = player_val
            .get("mvps")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let entry_3k = player_val
            .get("entry_3k")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let entry_4k = player_val
            .get("entry_4k")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        let entry_5k = player_val
            .get("entry_5k")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;

        // Insert match history (deduped on (player_id, discovered_match_id)).
        // The returned `is_new` flag is our match-scoped idempotency ledger:
        // the aggregate accumulate below is +1/+delta with no match key, so it
        // must run ONLY when this match's history row was newly inserted —
        // otherwise a re-delivered enrichment double-counts the aggregate.
        let (_history, is_new) = state
            .match_history_repo
            .create(CreatePlayerMatchHistory {
                player_id,
                game_id,
                discovered_match_id: discovered.id,
                map: map.clone(),
                match_time,
                team_scores: team_scores.iter().map(|&s| s as i32).collect(),
                match_duration_secs: duration,
                match_result: match_result_str.to_string(),
                kills,
                deaths,
                assists,
                score,
                headshots,
                mvps,
                entry_3k,
                entry_4k,
                entry_5k,
            })
            .await?;

        // Accumulate aggregate stats (upsert) — gated on the ledger so it is
        // applied exactly once per (player, match).
        if is_new {
            state
                .mm_stats_repo
                .accumulate_match_stats(
                    player_id,
                    game_id,
                    &AccumulateMatchStats {
                        is_win: match_result_str == "win",
                        is_loss: match_result_str == "loss",
                        is_draw: match_result_str == "draw",
                        kills,
                        deaths,
                        assists,
                        headshots,
                        mvps,
                        score,
                        entry_3k,
                        entry_4k,
                        entry_5k,
                        duration_secs: duration,
                        match_time,
                    },
                )
                .await?;
        }

        tracing::info!(
            player_id = %player_id,
            team,
            result = match_result_str,
            kills,
            deaths,
            assists,
            "Recorded public MM match stats"
        );
    }

    Ok(())
}

/// Request body for marking a match as failed.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FailedMatchRequest {
    pub error: String,
}

/// Mark a match enrichment as failed.
pub async fn mark_failed(
    State(state): State<InternalState>,
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

// =============================================================================
// Demo endpoints (internal) — for portal-scanner
// =============================================================================

/// Batch catalog demos (service auth).
pub async fn internal_batch_catalog_demos(
    State(state): State<InternalState>,
    service_auth: AuthenticatedService,
    Json(request): Json<BatchCatalogDemosRequest>,
) -> ApiResult<Json<DataResponse<BatchCatalogResultResponse>>> {
    service_auth.require_permission(service::DEMOS_CATALOG)?;
    request.validate()?;

    let game_id = GameId::from(request.game_id);
    let mut created = Vec::new();
    let mut existing = Vec::new();
    let mut errors = Vec::new();

    for entry in request.demos {
        match state
            .demo_service
            .catalog_demo(
                game_id,
                entry.file_name,
                entry.s3_bucket,
                entry.s3_key.clone(),
                entry.file_size_bytes,
            )
            .await
        {
            Ok(result) => {
                if result.is_created() {
                    created.push(DemoResponse::from(result.into_demo()));
                } else {
                    existing.push(DemoResponse::from(result.into_demo()));
                }
            }
            Err(e) => {
                errors.push(BatchCatalogErrorResponse {
                    s3_key: entry.s3_key,
                    error: e.to_string(),
                });
            }
        }
    }

    Ok(Json(DataResponse::new(
        BatchCatalogResultResponse {
            created,
            existing,
            errors,
        },
        "internal",
    )))
}

/// Get pending demos (service auth).
pub async fn internal_get_pending_demos(
    State(state): State<InternalState>,
    service_auth: AuthenticatedService,
    Query(query): Query<PendingDemosQuery>,
) -> ApiResult<Json<DataResponse<Vec<DemoResponse>>>> {
    service_auth.require_permission(service::DEMOS_READ)?;

    let demos = state
        .demo_service
        .get_pending_demos(query.limit.unwrap_or(50))
        .await?;

    let responses: Vec<DemoResponse> = demos.into_iter().map(DemoResponse::from).collect();

    Ok(Json(DataResponse::new(responses, "internal")))
}

/// Submit parsed stats for a demo (service auth).
pub async fn internal_submit_demo_stats(
    State(state): State<InternalState>,
    service_auth: AuthenticatedService,
    Path(demo_id): Path<DemoId>,
    Json(request): Json<SubmitDemoStatsRequest>,
) -> ApiResult<Json<DataResponse<DemoResponse>>> {
    service_auth.require_permission(service::DEMOS_STATS)?;
    request.validate()?;

    // Look up the demo to get its game_id
    let demo = state.demo_service.get_demo(demo_id).await?;

    // Check if this is a CS2 game (by looking up plugin_id)
    let is_cs2 = state
        .game_repo
        .find_by_id(demo.game_id.as_uuid())
        .await
        .ok()
        .flatten()
        .is_some_and(|g| g.plugin_id == "cs2");

    // Parse match_date
    let match_date = request
        .match_date
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.to_utc());

    // Build domain metadata
    let metadata = ParsedDemoMetadata {
        map_name: request.map_name.unwrap_or_default(),
        match_date,
        team1_name: request.team1_name.unwrap_or_default(),
        team2_name: request.team2_name.unwrap_or_default(),
        team1_score: request.team1_score.unwrap_or(0),
        team2_score: request.team2_score.unwrap_or(0),
        total_rounds: request.total_rounds.unwrap_or(0),
        duration_seconds: request.duration_seconds,
    };

    // Convert players
    let players: Vec<DemoPlayerInput> = request
        .players
        .into_iter()
        .map(|p| {
            let stats = if is_cs2 {
                super::demos::extract_cs2_player_stats(&p.stats)
            } else {
                DemoPlayerStats::default()
            };
            DemoPlayerInput {
                steam_id: p.steam_id,
                player_name: p.player_name,
                team_name: p.team_name,
                stats,
            }
        })
        .collect();

    let raw_stats = request.raw_stats.clone();
    // Auto-link kill-switch: a settings read failure must not block
    // ingestion, so fall back to enabled.
    let auto_link = state
        .system_settings_service
        .get_bool(
            portal_domain::services::system_settings::DEMO_AUTO_LINK_ENABLED,
            true,
        )
        .await
        .unwrap_or(true);
    let demo = state
        .demo_service
        .save_demo_stats(demo_id, metadata, request.raw_stats, players, auto_link)
        .await?;

    // Project EAV stat facts from the raw stats via the game plugin.
    // Non-fatal: the canonical stats are already persisted.
    super::demos::extract_and_store_stat_facts(
        &state.game_repo,
        &state.plugin_manager,
        &state.demo_stats_repo,
        demo_id,
        demo.game_id,
        &raw_stats,
    )
    .await;

    Ok(Json(DataResponse::new(
        DemoResponse::from(demo),
        "internal",
    )))
}

/// Mark a demo's stats processing as failed (service auth).
pub async fn internal_mark_demo_stats_failed(
    State(state): State<InternalState>,
    service_auth: AuthenticatedService,
    Path(demo_id): Path<DemoId>,
    Json(request): Json<MarkDemoFailedRequest>,
) -> ApiResult<Json<DataResponse<DemoResponse>>> {
    service_auth.require_permission(service::DEMOS_STATS)?;
    request.validate()?;

    let demo = state
        .demo_service
        .mark_stats_failed(demo_id, &request.error)
        .await?;

    Ok(Json(DataResponse::new(
        DemoResponse::from(demo),
        "internal",
    )))
}
