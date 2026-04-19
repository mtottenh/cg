//! Evidence API handlers.
//!
//! Handlers for managing match evidence (demos, screenshots, videos, links).

use crate::adapters::EvidencePluginAdapter;
use crate::dto::common::DataResponse;
use crate::dto::requests::{
    AddLinkEvidenceRequest, DiscoverEvidenceQuery, InitiateUploadRequest,
    LinkDiscoveredEvidenceRequest, ListEvidenceQuery, ValidateEvidenceRequest,
};
use crate::dto::responses::{
    AccessUrlResponse, DiscoveredEvidenceResponse, EvidenceResponse, EvidenceSummaryResponse,
    UploadInfoResponse, ValidationResultResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::EvidenceState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::Json;
use portal_core::{EvidenceId, TournamentMatchId};
use portal_domain::entities::evidence::{MatchEvidenceContext, ParticipantContext};
use portal_domain::entities::result_claim::GameResult as DomainGameResult;
use portal_domain::repositories::TournamentMatchRepository;
use std::sync::Arc;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

// =============================================================================
// EVIDENCE UPLOAD ENDPOINTS
// =============================================================================

/// Initiate a file upload for evidence.
///
/// Returns a presigned URL for uploading the file directly to S3.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/evidence/upload",
    request_body = InitiateUploadRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Upload initiated", body = DataResponse<UploadInfoResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a match participant", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn initiate_upload(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<InitiateUploadRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<UploadInfoResponse>>)> {
    let request_id = get_request_id(&headers);

    let evidence_type: portal_domain::entities::evidence::EvidenceType = req
        .evidence_type
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence type"))?;

    // Build human-readable S3 key prefix from league/tournament slugs
    let s3_key_prefix = build_evidence_key_prefix(&state, match_id, evidence_type).await;

    let upload_info = state
        .evidence_service
        .initiate_upload(
            match_id,
            s3_key_prefix,
            req.game_number,
            evidence_type,
            req.file_name,
            req.file_size_bytes,
            req.mime_type,
            auth.user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            UploadInfoResponse::from(upload_info),
            request_id,
        )),
    ))
}

/// Complete an evidence upload.
///
/// Verifies the file was uploaded and marks the evidence as active.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/evidence/{evidence_id}/complete",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("evidence_id" = String, Path, description = "Evidence ID")
    ),
    responses(
        (status = 200, description = "Upload completed", body = DataResponse<EvidenceResponse>),
        (status = 400, description = "File not uploaded", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Evidence not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn complete_upload(
    State(state): State<EvidenceState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_match_id, evidence_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<EvidenceResponse>>> {
    let request_id = get_request_id(&headers);
    let evidence_id: EvidenceId = evidence_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))?;

    let evidence = state.evidence_service.complete_upload(evidence_id).await?;

    Ok(Json(DataResponse::new(
        EvidenceResponse::from(evidence),
        request_id,
    )))
}

// =============================================================================
// LINK EVIDENCE ENDPOINTS
// =============================================================================

/// Add a link as evidence.
///
/// For video links (YouTube, Twitch) or other external evidence.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/evidence/link",
    request_body = AddLinkEvidenceRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Link evidence added", body = DataResponse<EvidenceResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a match participant", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn add_link_evidence(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<AddLinkEvidenceRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    let request_id = get_request_id(&headers);

    let evidence_type: portal_domain::entities::evidence::EvidenceType = req
        .evidence_type
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence type (must be video or link)"))?;

    let evidence = state
        .evidence_service
        .add_link(
            match_id,
            req.game_number,
            evidence_type,
            req.url,
            req.name,
            req.description,
            auth.user_id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            EvidenceResponse::from(evidence),
            request_id,
        )),
    ))
}

// =============================================================================
// EVIDENCE RETRIEVAL ENDPOINTS
// =============================================================================

/// List evidence for a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/evidence",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ListEvidenceQuery
    ),
    responses(
        (status = 200, description = "Evidence list", body = DataResponse<Vec<EvidenceSummaryResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "evidence"
)]
pub async fn list_evidence(
    State(state): State<EvidenceState>,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    Query(query): Query<ListEvidenceQuery>,
) -> ApiResult<Json<DataResponse<Vec<EvidenceSummaryResponse>>>> {
    let request_id = get_request_id(&headers);

    let mut evidence = if let Some(game_number) = query.game_number {
        state
            .evidence_service
            .get_game_evidence(match_id, game_number)
            .await?
    } else {
        state.evidence_service.get_match_evidence(match_id).await?
    };

    // Apply filters
    if let Some(ref et) = query.evidence_type {
        if let Ok(parsed) = et.parse::<portal_domain::entities::evidence::EvidenceType>() {
            evidence.retain(|e| e.evidence_type == parsed);
        }
    }
    if let Some(ref st) = query.status {
        if let Ok(parsed) = st.parse::<portal_domain::entities::evidence::EvidenceStatus>() {
            evidence.retain(|e| e.status == parsed);
        }
    }
    if !query.include_discovered {
        evidence.retain(|e| {
            e.evidence_source != portal_domain::entities::evidence::EvidenceSource::PluginDiscovery
        });
    }

    let summaries: Vec<EvidenceSummaryResponse> = evidence
        .into_iter()
        .map(EvidenceSummaryResponse::from)
        .collect();

    Ok(Json(DataResponse::new(summaries, request_id)))
}

/// Get evidence details.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/evidence/{evidence_id}",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("evidence_id" = String, Path, description = "Evidence ID")
    ),
    responses(
        (status = 200, description = "Evidence details", body = DataResponse<EvidenceResponse>),
        (status = 404, description = "Evidence not found", body = ApiError),
    ),
    tag = "evidence"
)]
pub async fn get_evidence(
    State(state): State<EvidenceState>,
    headers: HeaderMap,
    Path((match_id, evidence_id)): Path<(TournamentMatchId, EvidenceId)>,
) -> ApiResult<Json<DataResponse<EvidenceResponse>>> {
    let request_id = get_request_id(&headers);

    let evidence = state.evidence_service.get_evidence(evidence_id).await?;

    // Verify the evidence belongs to this match
    if evidence.match_id != match_id {
        return Err(ApiError::not_found("Evidence not found for this match"));
    }

    Ok(Json(DataResponse::new(
        EvidenceResponse::from(evidence),
        request_id,
    )))
}

/// Get a presigned URL for accessing evidence.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/evidence/{evidence_id}/access",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("evidence_id" = String, Path, description = "Evidence ID")
    ),
    responses(
        (status = 200, description = "Access URL", body = DataResponse<AccessUrlResponse>),
        (status = 404, description = "Evidence not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn get_access_url(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_match_id, evidence_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<AccessUrlResponse>>> {
    let request_id = get_request_id(&headers);
    let evidence_id: EvidenceId = evidence_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))?;

    // Extract IP address from X-Forwarded-For or X-Real-IP header
    let ip_address = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
        });

    // Extract User-Agent header
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let access_url = state
        .evidence_service
        .get_access_url(evidence_id, auth.user_id, ip_address, user_agent)
        .await?;

    Ok(Json(DataResponse::new(
        AccessUrlResponse::from(access_url),
        request_id,
    )))
}

/// Delete evidence.
#[utoipa::path(
    delete,
    path = "/v1/matches/{match_id}/evidence/{evidence_id}",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("evidence_id" = String, Path, description = "Evidence ID")
    ),
    responses(
        (status = 204, description = "Evidence deleted"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Cannot delete this evidence", body = ApiError),
        (status = 404, description = "Evidence not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn delete_evidence(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    Path((match_id, evidence_id)): Path<(TournamentMatchId, EvidenceId)>,
) -> ApiResult<StatusCode> {

    // Before deleting, check if there's a corresponding demo_match_link to clean up.
    // Both `link_discovered` (catalog: prefix) and `link_demo` (with demo_id) store
    // `catalog_demo_id` in the evidence metadata.
    let evidence = state.evidence_service.get_evidence(evidence_id).await?;
    if let Some(demo_id_str) = evidence.plugin_metadata.get("catalog_demo_id").and_then(|v| v.as_str()) {
        if let Ok(demo_id) = demo_id_str.parse::<portal_core::DemoId>() {
            // Best-effort: ignore errors if the link was already removed
            let _ = state.demo_service.unlink_from_match(demo_id, match_id).await;
        }
    }

    state
        .evidence_service
        .delete_evidence(evidence_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// EVIDENCE DISCOVERY ENDPOINTS (PLUGIN-BASED)
// =============================================================================

/// Discover evidence for a match using plugins.
///
/// Uses game-specific plugins to find evidence (e.g., CS2 demos in S3).
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/evidence/discover",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        DiscoverEvidenceQuery
    ),
    responses(
        (status = 200, description = "Discovered evidence", body = DataResponse<Vec<DiscoveredEvidenceResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn discover_evidence(
    State(state): State<EvidenceState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    Query(query): Query<DiscoverEvidenceQuery>,
) -> ApiResult<Json<DataResponse<Vec<DiscoveredEvidenceResponse>>>> {
    let request_id = get_request_id(&headers);

    let (match_, plugin) = resolve_evidence_plugin(&state, match_id).await?;
    let context = build_evidence_context(&state, &match_).await?;
    let adapter = EvidencePluginAdapter::new(plugin)
        .ok_or_else(|| ApiError::bad_request("Game plugin does not support evidence"))?;

    let mut discovered = state
        .evidence_service
        .discover_available(match_id, &context, &adapter)
        .await?;

    // Source 2: Catalog-based discovery
    let catalog_results = state
        .demo_service
        .discover_for_match(&context)
        .await
        .unwrap_or_default();

    // Merge, dedup by external_id
    let existing_ids: std::collections::HashSet<String> =
        discovered.iter().map(|d| d.external_id.clone()).collect();
    for item in catalog_results {
        if !existing_ids.contains(&item.external_id) {
            discovered.push(item);
        }
    }
    // Re-sort by relevance
    discovered.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply query filters
    if let Some(min_relevance) = query.min_relevance {
        discovered.retain(|d| d.relevance_score >= min_relevance);
    }
    if let Some(limit) = query.limit {
        discovered.truncate(limit.max(0) as usize);
    }

    let responses: Vec<DiscoveredEvidenceResponse> =
        discovered.into_iter().map(DiscoveredEvidenceResponse::from).collect();

    Ok(Json(DataResponse::new(responses, request_id)))
}

/// Link discovered evidence to a match.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/evidence/link-discovered",
    request_body = LinkDiscoveredEvidenceRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Evidence linked", body = DataResponse<EvidenceResponse>),
        (status = 400, description = "Evidence not found or already linked", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn link_discovered_evidence(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<LinkDiscoveredEvidenceRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    let request_id = get_request_id(&headers);

    // Check if this is a catalog-based discovery (external_id starts with "catalog:")
    if let Some(demo_id_str) = req.external_id.strip_prefix("catalog:") {
        let demo_id: portal_core::DemoId = demo_id_str
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid catalog demo ID"))?;

        // Get the demo from the catalog
        let demo = state.demo_service.get_demo(demo_id).await?;

        // Create a DemoMatchLink via demo_service
        let _link = state
            .demo_service
            .link_to_match(
                demo_id,
                match_id,
                req.game_number,
                portal_core::DemoLinkType::Evidence,
                Some(auth.user_id),
            )
            .await?;

        // Build DiscoveredEvidence from the catalog demo
        use portal_domain::entities::evidence::{
            DiscoveredEvidence, EvidenceStorage, EvidenceType,
        };
        let discovered = DiscoveredEvidence {
            external_id: req.external_id,
            evidence_type: EvidenceType::Demo,
            name: demo.file_name.clone(),
            storage: EvidenceStorage::S3 {
                bucket: demo.s3_bucket.clone(),
                key: demo.s3_key.clone(),
            },
            file_size_bytes: demo.file_size_bytes,
            metadata: serde_json::json!({
                "catalog_demo_id": demo.id.to_string(),
                "map_name": demo.metadata.as_ref().map(|m| &m.map_name),
            }),
            discovered_at: chrono::Utc::now(),
            relevance_score: 1.0,
        };

        // Link via evidence service
        let evidence = state
            .evidence_service
            .link_discovered(match_id, discovered, req.game_number, auth.user_id)
            .await?;

        return Ok((
            StatusCode::CREATED,
            Json(DataResponse::new(
                EvidenceResponse::from(evidence),
                request_id,
            )),
        ));
    }

    // Non-catalog: use plugin-based discovery flow
    let (match_, plugin) = resolve_evidence_plugin(&state, match_id).await?;
    let context = build_evidence_context(&state, &match_).await?;
    let adapter = EvidencePluginAdapter::new(plugin)
        .ok_or_else(|| ApiError::bad_request("Game plugin does not support evidence"))?;

    // Discover evidence via plugin, then find the one with matching external_id
    use portal_domain::services::tournament::EvidencePluginClient;
    let discovered_list = adapter
        .discover_evidence(&context)
        .await
        .map_err(|e| ApiError::internal(format!("Evidence discovery failed: {e}")))?;

    let discovered = discovered_list
        .into_iter()
        .find(|d| d.external_id == req.external_id)
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "No discovered evidence with external_id '{}'",
                req.external_id
            ))
        })?;

    let evidence = state
        .evidence_service
        .link_discovered(match_id, discovered, req.game_number, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            EvidenceResponse::from(evidence),
            request_id,
        )),
    ))
}

// =============================================================================
// EVIDENCE VALIDATION ENDPOINTS
// =============================================================================

/// Validate evidence against a claimed result.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/evidence/validate",
    request_body = ValidateEvidenceRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Validation result", body = DataResponse<ValidationResultResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 404, description = "Match or evidence not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn validate_evidence(
    State(state): State<EvidenceState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<ValidateEvidenceRequest>,
) -> ApiResult<Json<DataResponse<ValidationResultResponse>>> {
    let request_id = get_request_id(&headers);

    let (_match, plugin) = resolve_evidence_plugin(&state, match_id).await?;
    let adapter = EvidencePluginAdapter::new(plugin)
        .ok_or_else(|| ApiError::bad_request("Game plugin does not support evidence"))?;

    // Build a minimal GameResult from the request for validation
    let result = DomainGameResult {
        game_number: 1,
        map_id: String::new(),
        participant1_score: req.expected_participant1_score.unwrap_or(0),
        participant2_score: req.expected_participant2_score.unwrap_or(0),
        winner_registration_id: portal_core::TournamentRegistrationId::new(),
        started_at: None,
        completed_at: None,
        duration_seconds: None,
        evidence_ids: req
            .evidence_ids
            .iter()
            .map(|id| portal_core::EvidenceId::from(*id))
            .collect(),
        demo_link_id: None,
    };

    // Validate the first evidence item
    let evidence_id = req
        .evidence_ids
        .first()
        .ok_or_else(|| ApiError::bad_request("At least one evidence ID is required"))?;

    let evidence_id = portal_core::EvidenceId::from(*evidence_id);
    let validation = state
        .evidence_service
        .validate_against_result(evidence_id, &result, &adapter)
        .await?;

    Ok(Json(DataResponse::new(
        ValidationResultResponse::from(validation),
        request_id,
    )))
}

// =============================================================================
// EVIDENCE PLUGIN RESOLUTION HELPERS
// =============================================================================

/// Build a human-readable S3 key prefix from league/tournament slugs.
///
/// Returns `Some("league-slug/tournament-slug/evidence/demos/R1M3")` or
/// `Some("tournament-slug/evidence/screenshots/R2M1")` (no league).
/// Falls back to `None` if any lookup fails, letting the service use UUID-based keys.
async fn build_evidence_key_prefix(
    state: &EvidenceState,
    match_id: TournamentMatchId,
    evidence_type: portal_domain::entities::evidence::EvidenceType,
) -> Option<String> {
    use portal_domain::entities::evidence::EvidenceType;

    let match_ = state.tournament_match_repo.find_by_id(match_id).await.ok()??;

    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await
        .ok()?;

    let league_slug = if let Some(lid) = tournament.league_id {
        state.league_service.get_league(lid).await.ok().map(|l| l.slug)
    } else {
        None
    };

    let type_dir = match evidence_type {
        EvidenceType::Demo => "demos",
        EvidenceType::Screenshot => "screenshots",
        EvidenceType::Video => "videos",
        EvidenceType::ServerLog => "logs",
        EvidenceType::Link => "links",
    };

    let round_match = format!("R{}M{}", match_.round, match_.match_number);

    let prefix = match league_slug {
        Some(ls) => format!(
            "{}/{}/evidence/{}/{}",
            ls, tournament.slug, type_dir, round_match
        ),
        None => format!("{}/evidence/{}/{}", tournament.slug, type_dir, round_match),
    };

    Some(prefix)
}

/// Process-wide cache of `tournament_id → plugin`.
///
/// Tournaments' `game_id` is immutable after creation (a tournament belongs
/// to exactly one game), games' `plugin_id` is immutable after seeding, and
/// plugins themselves are `Arc<dyn GamePlugin>` — cheap to clone. So once
/// we've resolved the plugin for a tournament, we can serve every
/// subsequent call from memory. This eliminates 2 of the 3 DB round-trips
/// previously paid on every `/matches/{id}/evidence/*` request (match
/// lookup still happens because callers need the full match entity).
///
/// Invalidation: none is needed while those invariants hold. If tournament
/// migration or plugin reassignment ever becomes a real operation, add an
/// invalidation hook on the write path.
fn plugin_cache() -> &'static dashmap::DashMap<
    portal_core::TournamentId,
    Arc<dyn portal_plugins::GamePlugin>,
> {
    static CACHE: std::sync::OnceLock<
        dashmap::DashMap<portal_core::TournamentId, Arc<dyn portal_plugins::GamePlugin>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(dashmap::DashMap::new)
}

/// Resolve the evidence plugin for a given match.
///
/// Follows the chain: match → tournament → game → plugin. The tournament
/// → game → plugin leg is cached per-tournament.
async fn resolve_evidence_plugin(
    state: &EvidenceState,
    match_id: TournamentMatchId,
) -> ApiResult<(
    portal_domain::entities::TournamentMatch,
    Arc<dyn portal_plugins::GamePlugin>,
)> {
    // 1. Get the match (always fresh — callers use its full state).
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load match: {e}")))?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    // 2. Cached path: tournament_id → plugin.
    if let Some(plugin) = plugin_cache().get(&match_.tournament_id) {
        return Ok((match_, Arc::clone(plugin.value())));
    }

    // 3. Cache miss — resolve via tournament → game → plugin manager.
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load tournament: {e}")))?;

    let game = state
        .game_repo
        .find_by_id(tournament.game_id.as_uuid())
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load game: {e}")))?
        .ok_or_else(|| ApiError::not_found("Game not found"))?;

    let plugin = state
        .plugin_manager
        .get(&game.plugin_id)
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "No plugin registered for game '{}' (plugin_id: '{}')",
                game.display_name, game.plugin_id
            ))
        })?;

    plugin_cache().insert(match_.tournament_id, Arc::clone(&plugin));

    Ok((match_, plugin))
}

/// Build a [`MatchEvidenceContext`] for a match.
///
/// Resolves participant registration IDs to build participant contexts
/// (currently without Steam IDs since game profiles are not yet implemented).
async fn build_evidence_context(
    state: &EvidenceState,
    match_: &portal_domain::entities::TournamentMatch,
) -> ApiResult<MatchEvidenceContext> {
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load tournament: {e}")))?;

    let mut participants = Vec::new();

    for reg_id in [
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ]
    .into_iter()
    .flatten()
    {
        let reg = state
            .registration_service
            .get_registration(reg_id)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to load registration {reg_id}: {e}"))
            })?;

        // Build participant context with Steam IDs from player profiles
        let mut player_ids = Vec::new();
        let mut steam_ids = Vec::new();

        if let Some(pid) = reg.player_id {
            player_ids.push(pid);
            if let Ok(player) = state.player_service.get_player(pid).await {
                if let Some(sid) = &player.steam_id {
                    steam_ids.push(sid.clone());
                }
            }
        }

        participants.push(ParticipantContext {
            registration_id: reg_id.as_uuid(),
            name: reg.participant_name,
            player_ids: player_ids.iter().map(|id| id.as_uuid()).collect(),
            steam_ids,
        });
    }

    Ok(MatchEvidenceContext {
        tournament_id: match_.tournament_id.as_uuid(),
        match_id: match_.id.as_uuid(),
        game_id: tournament.game_id.to_string(),
        participants,
        scheduled_at: match_.scheduled_at,
        started_at: match_.started_at,
        completed_at: match_.completed_at,
    })
}

// =============================================================================
// CS2 DEMO VALIDATION ENDPOINTS
// =============================================================================

use crate::dto::requests::{GetDemoStatsQuery, LinkDemoRequest, ValidateDemoRequest};
use crate::dto::responses::{DemoPlayerStatsResponse, DemoStatsResponse, DemoValidationResponse};
use portal_plugins::{Cs2PluginWithEvidence, GameResult};

/// Validate a CS2 demo against claimed match result.
///
/// Fetches demo stats from the external demo service and validates against
/// the claimed scores and map.
///
/// Note: Team-to-participant mapping requires Steam IDs to be provided via query parameters
/// since automatic lookup would require game profile data which may not be available.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/evidence/validate-demo",
    request_body = ValidateDemoRequest,
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("participant1_steam_ids" = Option<String>, Query, description = "Steam IDs for participant 1 (comma-separated)"),
        ("participant2_steam_ids" = Option<String>, Query, description = "Steam IDs for participant 2 (comma-separated)")
    ),
    responses(
        (status = 200, description = "Validation result", body = DataResponse<DemoValidationResponse>),
        (status = 400, description = "Invalid request or demo not found", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn validate_demo(
    State(state): State<EvidenceState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(_match_id): Path<String>,
    Query(query): Query<GetDemoStatsQuery>,
    ValidatedJson(req): ValidatedJson<ValidateDemoRequest>,
) -> ApiResult<Json<DataResponse<DemoValidationResponse>>> {
    let request_id = get_request_id(&headers);

    // Parse Steam IDs from query parameters
    let team1_steam_ids: Vec<String> = query
        .participant1_steam_ids
        .map(|s| s.split(',').map(|id| id.trim().to_string()).collect())
        .unwrap_or_default();

    let team2_steam_ids: Vec<String> = query
        .participant2_steam_ids
        .map(|s| s.split(',').map(|id| id.trim().to_string()).collect())
        .unwrap_or_default();

    // Build claimed result from request
    let claimed_result = GameResult {
        game_number: req.game_number.unwrap_or(1),
        map_id: req.map_id,
        participant1_score: req.participant1_score,
        participant2_score: req.participant2_score,
    };

    // Validate using CS2 plugin
    let cs2_plugin = create_cs2_plugin(&state);
    let validation = cs2_plugin
        .validate_demo(
            &req.demo_name,
            &claimed_result,
            &team1_steam_ids,
            &team2_steam_ids,
        )
        .await
        .map_err(|e| ApiError::bad_request(format!("Validation failed: {e}")))?;

    Ok(Json(DataResponse::new(
        DemoValidationResponse {
            is_valid: validation.is_valid,
            confidence: validation.confidence,
            extracted_result: validation.extracted_result.map(|r| {
                crate::dto::responses::ExtractedResultResponse {
                    map_id: r.map_id,
                    participant1_score: r.participant1_score,
                    participant2_score: r.participant2_score,
                    duration_seconds: r.duration_seconds,
                }
            }),
            warnings: validation.warnings,
            errors: validation.errors,
            demo_url: cs2_plugin.get_demo_url(&req.demo_name),
            stats_url: cs2_plugin.get_stats_url(&req.demo_name),
        },
        request_id,
    )))
}

/// Get CS2 demo stats without validation.
///
/// Fetches pre-parsed demo stats from the external demo service.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/evidence/demo-stats/{demo_name}",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("demo_name" = String, Path, description = "Demo file name"),
        GetDemoStatsQuery
    ),
    responses(
        (status = 200, description = "Demo stats", body = DataResponse<DemoStatsResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn get_demo_stats(
    State(state): State<EvidenceState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_match_id, demo_name)): Path<(String, String)>,
    Query(_query): Query<GetDemoStatsQuery>,
) -> ApiResult<Json<DataResponse<DemoStatsResponse>>> {
    let request_id = get_request_id(&headers);

    let cs2_plugin = create_cs2_plugin(&state);

    let stats = cs2_plugin
        .get_demo_stats(&demo_name)
        .await
        .map_err(|e| match e {
            portal_plugins::PluginError::NotFound(_) => {
                ApiError::not_found(format!("Demo not found: {demo_name}"))
            }
            _ => ApiError::internal(format!("Failed to fetch demo stats: {e}")),
        })?;

    // Get team names for response
    let team_names: Vec<String> = stats.team_names();
    let (team1_name, team2_name) = if team_names.len() >= 2 {
        (team_names[0].clone(), team_names[1].clone())
    } else {
        ("Team 1".to_string(), "Team 2".to_string())
    };

    // Build player stats response
    let players: Vec<DemoPlayerStatsResponse> = stats
        .all_player_summaries()
        .into_iter()
        .map(|p| DemoPlayerStatsResponse {
            steam_id: p.player_id.to_string(),
            name: p.player_name,
            team: p.team.map(|t| t.team_name).unwrap_or_default(),
            kills: p.kills,
            deaths: p.deaths,
            assists: p.assists,
            damage: p.damage_dealt,
            adr: p.adr,
        })
        .collect();

    Ok(Json(DataResponse::new(
        DemoStatsResponse {
            demo_name: stats.demo_file.clone(),
            map_name: stats.map.clone(),
            match_date: stats.match_date.clone(),
            match_id: stats.match_id.clone(),
            team1_score: stats.score_for_team(&team1_name).unwrap_or(0),
            team2_score: stats.score_for_team(&team2_name).unwrap_or(0),
            team1_name,
            team2_name,
            total_rounds: stats.total_rounds(),
            players,
            demo_url: cs2_plugin.get_demo_url(&demo_name),
            stats_url: cs2_plugin.get_stats_url(&demo_name),
        },
        request_id,
    )))
}

/// Link a CS2 demo to a match as evidence.
///
/// Creates an evidence record linking the demo to the specified match.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/evidence/link-demo",
    request_body = LinkDemoRequest,
    params(
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 201, description = "Demo linked", body = DataResponse<EvidenceResponse>),
        (status = 400, description = "Invalid request or demo not found", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn link_demo(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<LinkDemoRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    let request_id = get_request_id(&headers);

    let cs2_plugin = create_cs2_plugin(&state);

    // Verify demo exists by fetching stats
    let stats = cs2_plugin
        .get_demo_stats(&req.demo_name)
        .await
        .map_err(|_| ApiError::not_found(format!("Demo not found: {}", req.demo_name)))?;

    // Build DiscoveredEvidence with proper Demo type
    use portal_domain::entities::evidence::{
        DiscoveredEvidence, EvidenceStorage, EvidenceType,
    };
    let discovered = DiscoveredEvidence {
        external_id: format!("demo:{}", req.demo_name),
        evidence_type: EvidenceType::Demo,
        name: format!("CS2 Demo: {}", stats.map),
        storage: EvidenceStorage::Url {
            url: cs2_plugin.get_demo_url(&req.demo_name),
        },
        file_size_bytes: None,
        metadata: serde_json::json!({
            "demo_name": req.demo_name,
            "map": stats.map,
            "description": req.description,
            "catalog_demo_id": req.demo_id,
        }),
        discovered_at: chrono::Utc::now(),
        relevance_score: 1.0,
    };

    // If a catalog demo_id was provided, also create a demo_match_link so the
    // demo is visible via GET /v1/matches/{match_id}/demos.
    if let Some(ref demo_id_str) = req.demo_id {
        let demo_id: portal_core::DemoId = demo_id_str
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid demo_id format"))?;

        // Verify the catalog demo exists
        let _demo = state.demo_service.get_demo(demo_id).await?;

        let _link = state
            .demo_service
            .link_to_match(
                demo_id,
                match_id,
                req.game_number,
                portal_core::DemoLinkType::Evidence,
                Some(auth.user_id),
            )
            .await?;
    }

    let evidence = state
        .evidence_service
        .link_discovered(match_id, discovered, req.game_number, auth.user_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            EvidenceResponse::from(evidence),
            request_id,
        )),
    ))
}

// =============================================================================
// LOCAL EVIDENCE UPLOAD HANDLER
// =============================================================================

/// Maximum size of a single local evidence upload (64 MiB).
///
/// This is generous for replays and screenshots but small enough that an
/// abusive caller can't exhaust memory or fill the disk in one request.
const LOCAL_EVIDENCE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Handle direct file upload for local development.
///
/// In production, uploads go directly to S3 via presigned URLs and this
/// endpoint is unreachable: it returns 404 unless `EVIDENCE_STORAGE` is unset
/// or set to `local`.
///
/// Defenses:
/// * **Authentication required** — the caller must hold a valid JWT. The
///   prior implementation accepted any unauthenticated `PUT /uploads/*path`,
///   which let anyone write arbitrary bytes to the server's filesystem.
/// * **Path traversal rejected** — absolute paths, `..` components, and
///   non-UTF-8 paths are refused outright. After joining we canonicalize the
///   parent directory and verify it stays inside `state.uploads_path`.
/// * **Size capped** — bodies above [`LOCAL_EVIDENCE_MAX_BYTES`] are rejected.
pub async fn local_evidence_upload(
    State(state): State<EvidenceState>,
    _auth: AuthenticatedUser,
    axum::extract::Path(path): axum::extract::Path<String>,
    body: axum::body::Bytes,
) -> Result<StatusCode, ApiError> {
    // S3 mode: this endpoint should not be used. Refuse rather than silently
    // writing files that nothing will ever read.
    if std::env::var("EVIDENCE_STORAGE")
        .ok()
        .is_some_and(|v| v.eq_ignore_ascii_case("s3"))
    {
        return Err(ApiError::not_found("Local upload endpoint disabled in S3 mode"));
    }

    if body.len() > LOCAL_EVIDENCE_MAX_BYTES {
        return Err(ApiError::bad_request(format!(
            "Upload too large ({} bytes; max {})",
            body.len(),
            LOCAL_EVIDENCE_MAX_BYTES
        )));
    }

    let rel = std::path::Path::new(&path);
    if rel.is_absolute() {
        return Err(ApiError::bad_request("Absolute paths not allowed"));
    }
    if rel
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ApiError::bad_request("Parent directory components not allowed"));
    }
    if path.is_empty() {
        return Err(ApiError::bad_request("Empty path"));
    }

    let base = std::path::Path::new(&state.uploads_path);
    // Canonicalize the base once. If it doesn't exist yet, create it.
    tokio::fs::create_dir_all(base)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to prepare uploads dir: {e}")))?;
    let canon_base = tokio::fs::canonicalize(base)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to canonicalize uploads dir: {e}")))?;

    let file_path = canon_base.join(rel);

    // Create parent dirs (relative to the safe base) and re-check containment
    // after canonicalization, in case symlinks point outside the tree.
    if let Some(parent) = file_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create directory: {e}")))?;
        let canon_parent = tokio::fs::canonicalize(parent)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to canonicalize parent: {e}")))?;
        if !canon_parent.starts_with(&canon_base) {
            return Err(ApiError::forbidden("Path escapes uploads directory"));
        }
    }

    tokio::fs::write(&file_path, &body)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to write file: {e}")))?;

    Ok(StatusCode::OK)
}

/// Create a CS2 plugin with evidence support, using the configured demo service URL.
fn create_cs2_plugin(state: &EvidenceState) -> Cs2PluginWithEvidence {
    match &state.cs2_demo_base_url {
        Some(url) => Cs2PluginWithEvidence::with_demo_url(url.clone()),
        None => Cs2PluginWithEvidence::new(),
    }
}
