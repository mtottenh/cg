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
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::EvidenceState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use portal_core::{EvidenceId, ScopeType, TournamentMatchId};
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

/// Participant-or-admin gate for evidence endpoints that mutate or persist
/// match state.
///
/// Mirrors the binding `add_link_evidence` / `initiate_upload` get from
/// `EvidenceService::resolve_uploader_registration`: the caller must be the
/// registrant of one of the match's two participant registrations, or hold
/// the tournament admin override. Handlers that never reach the evidence
/// service's own check (validation endpoints) must call this explicitly —
/// otherwise any authenticated user can stamp results onto any match.
async fn require_match_participant_or_admin(
    state: &EvidenceState,
    perm_checker: &PermissionChecker,
    auth: &AuthenticatedUser,
    match_id: TournamentMatchId,
) -> ApiResult<()> {
    if perm_checker
        .has_admin_override(auth, ScopeType::Tournament)
        .await
    {
        return Ok(());
    }

    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load match: {e}")))?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    for reg_id in [
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(reg) = state.registration_service.get_registration(reg_id).await
            && reg.registered_by == auth.user_id
        {
            return Ok(());
        }
    }

    Err(ApiError::forbidden(
        "User is not a participant in this match",
    ))
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
    perm_checker: PermissionChecker,
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

    // Admins may attach evidence on behalf of a match without being a
    // participant; everyone else must belong to one of the match's
    // registrations (enforced by the service).
    let acting_as_admin = perm_checker
        .has_admin_override(&auth, ScopeType::Tournament)
        .await;

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
            acting_as_admin,
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
        (status = 403, description = "Not the uploader of this evidence", body = ApiError),
        (status = 404, description = "Evidence not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn complete_upload(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_match_id, evidence_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<EvidenceResponse>>> {
    let request_id = get_request_id(&headers);
    let evidence_id: EvidenceId = evidence_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))?;

    // Bind to the original uploader: `initiate_upload` records
    // `uploaded_by_user_id`, so only the user who started this upload (or a
    // tournament admin) may flip it Pending → Active.
    let pending = state.evidence_service.get_evidence(evidence_id).await?;
    if pending.uploaded_by_user_id != Some(auth.user_id)
        && !perm_checker
            .has_admin_override(&auth, ScopeType::Tournament)
            .await
    {
        return Err(ApiError::forbidden(
            "Only the user who initiated this upload may complete it",
        ));
    }

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
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<AddLinkEvidenceRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    let request_id = get_request_id(&headers);

    let evidence_type: portal_domain::entities::evidence::EvidenceType = req
        .evidence_type
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence type (must be video or link)"))?;

    // Admins may attach evidence on behalf of a match without being a
    // participant; everyone else must belong to one of the match's
    // registrations (enforced by the service).
    let acting_as_admin = perm_checker
        .has_admin_override(&auth, ScopeType::Tournament)
        .await;

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
            acting_as_admin,
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
    if let Some(ref et) = query.evidence_type
        && let Ok(parsed) = et.parse::<portal_domain::entities::evidence::EvidenceType>()
    {
        evidence.retain(|e| e.evidence_type == parsed);
    }
    if let Some(ref st) = query.status
        && let Ok(parsed) = st.parse::<portal_domain::entities::evidence::EvidenceStatus>()
    {
        evidence.retain(|e| e.status == parsed);
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
///
/// P-67 listed this as a redundant single-getter over the summary list, and it
/// is not: `find_by_match` excludes `pending` and `deleted`
/// (`portal-db/src/adapters/evidence.rs:53`), so the list cannot show an
/// in-flight upload at all. Between `initiate` and `complete` — the exact window
/// the three-step upload flow occupies — this is the only way to read an
/// evidence row's state. It has no frontend consumer today, which is why it was
/// proposed for deletion; deleting it would have removed the only read path for
/// a state the product genuinely has.
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
    perm_checker: PermissionChecker,
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

    // Uploader / match participant / tournament admin only — presigned
    // evidence downloads are not public.
    let acting_as_admin = perm_checker
        .has_admin_override(&auth, ScopeType::Tournament)
        .await;

    let access_url = state
        .evidence_service
        .get_access_url(
            evidence_id,
            auth.user_id,
            acting_as_admin,
            ip_address,
            user_agent,
        )
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
    perm_checker: PermissionChecker,
    Path((match_id, evidence_id)): Path<(TournamentMatchId, EvidenceId)>,
) -> ApiResult<StatusCode> {
    // Only the uploader, a participant of the evidence's match, or a
    // tournament admin may delete evidence (enforced by the service; the
    // admin bit is resolved here).
    let acting_as_admin = perm_checker
        .has_admin_override(&auth, ScopeType::Tournament)
        .await;

    // Before deleting, check if there's a corresponding demo_match_link to clean up.
    // Both `link_discovered` (catalog: prefix) and `link_demo` (with demo_id) store
    // `catalog_demo_id` in the evidence metadata.
    let evidence = state.evidence_service.get_evidence(evidence_id).await?;

    // Authorize before any side effects (the demo unlink below mutates state).
    state
        .evidence_service
        .delete_evidence(evidence_id, auth.user_id, acting_as_admin)
        .await?;

    if let Some(demo_id_str) = evidence
        .plugin_metadata
        .get("catalog_demo_id")
        .and_then(|v| v.as_str())
        && let Ok(demo_id) = demo_id_str.parse::<portal_core::DemoId>()
    {
        // Best-effort: ignore errors if the link was already removed
        let _ = state
            .demo_service
            .unlink_from_match(demo_id, match_id)
            .await;
    }

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

    let responses: Vec<DiscoveredEvidenceResponse> = discovered
        .into_iter()
        .map(DiscoveredEvidenceResponse::from)
        .collect();

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
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<LinkDiscoveredEvidenceRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    use portal_domain::entities::evidence::{
        DiscoveredEvidence, EvidenceSource, EvidenceStorage, EvidenceType,
    };
    use portal_domain::services::tournament::EvidencePluginClient;

    let request_id = get_request_id(&headers);

    // P-108: this took only `AuthenticatedUser`, so ANY logged-in user could attach
    // demo evidence to ANY match — including matches they have nothing to do with.
    // Evidence feeds result review and dispute resolution, so it is an integrity
    // surface, not a cosmetic one. The gate already existed and is applied by
    // `validate_demo` on this very surface; it was simply never applied here.
    // Same shape as P-24 and P-59.
    require_match_participant_or_admin(&state, &perm_checker, &auth, match_id).await?;

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

        // Link via evidence service. P-109: a person picked this demo out of the
        // suggestion list and pressed the button, so the row is stamped
        // `ManualUpload` — stamping `PluginDiscovery` hid it from the default
        // evidence listing every surface in the product reads.
        let evidence = state
            .evidence_service
            .link_discovered(
                match_id,
                discovered,
                req.game_number,
                auth.user_id,
                EvidenceSource::ManualUpload,
            )
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
        .link_discovered(
            match_id,
            discovered,
            req.game_number,
            auth.user_id,
            EvidenceSource::ManualUpload,
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
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a match participant", body = ApiError),
        (status = 404, description = "Match or evidence not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn validate_evidence(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<ValidateEvidenceRequest>,
) -> ApiResult<Json<DataResponse<ValidationResultResponse>>> {
    let request_id = get_request_id(&headers);

    // This endpoint PERSISTS the outcome (evidence_repo.mark_validated), so
    // it must be bound to the match like any other evidence mutation.
    require_match_participant_or_admin(&state, &perm_checker, &auth, match_id).await?;

    // Validate the first evidence item
    let evidence_id = req
        .evidence_ids
        .first()
        .ok_or_else(|| ApiError::bad_request("At least one evidence ID is required"))?;
    let evidence_id = portal_core::EvidenceId::from(*evidence_id);

    let (match_, plugin) = resolve_evidence_plugin(&state, match_id).await?;

    let evidence = state.evidence_service.get_evidence(evidence_id).await?;
    if evidence.match_id != match_id {
        return Err(ApiError::not_found("Evidence not found on this match"));
    }

    // The claimed **per-game** result to validate against.
    //
    // These used to default to 0, so an omitted score validated a demo against
    // a scoreline no game can have — and 0-0 fails the plugin's own sanity
    // check, so the omission silently produced "invalid". Refuse instead. They
    // cannot be defaulted from the match row either: `tournament_matches`
    // carries the **series** score (maps won, capped at 10 by
    // `SubmitResultClaimRequest`), while a demo records one map's rounds, so
    // filling them in from there would compare two different units and call
    // every honest demo a contradiction. The caller states the game's score,
    // which is what `game_results` on the claim records.
    let (Some(claimed_p1), Some(claimed_p2)) = (
        req.expected_participant1_score,
        req.expected_participant2_score,
    ) else {
        return Err(ApiError::bad_request(
            "expected_participant1_score and expected_participant2_score are required to validate evidence against a result",
        ));
    };

    // P-111: prefer the portal's own copy of the demo's extracted result.
    //
    // Every validation route in the product went through the external CS2
    // stats service, so nothing was ever validated in any deployment without
    // it — the reason `demo_match_links.validated` had never been true for a
    // single row. For a catalogued demo the portal already stores the parsed
    // result (`demos.metadata` + `demo_players`, written by `save_demo_stats`),
    // so the comparison needs no external call at all. The plugin remains the
    // route for evidence with no catalog row behind it.
    let catalog_demo_id = evidence
        .plugin_metadata
        .get("catalog_demo_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<portal_core::DemoId>().ok());

    let catalog_validation =
        catalog_validation_for(&state, catalog_demo_id, &match_, claimed_p1, claimed_p2).await?;

    let validation = if let Some(v) = catalog_validation {
        v
    } else {
        use portal_domain::services::tournament::EvidencePluginClient;

        let adapter = EvidencePluginAdapter::new(plugin)
            .ok_or_else(|| ApiError::bad_request("Game plugin does not support evidence"))?;
        let result = DomainGameResult {
            game_number: 1,
            map_id: String::new(),
            participant1_score: claimed_p1,
            participant2_score: claimed_p2,
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
        adapter.validate_evidence(&evidence, &result).await?
    };

    // Persist the verdict against the evidence row...
    state
        .evidence_service
        .record_validation(evidence_id, &validation)
        .await?;

    // ...and against the demo_match_link, which is the row every "Validated"
    // chip in the frontend actually reads (`DemoBrowser.vue`,
    // `EvidenceDisplay.vue`, `AdminDemoDetailPage.vue`). Nothing had ever
    // written it.
    if let Some(demo_id) = catalog_demo_id {
        state
            .demo_service
            .record_link_validation(
                demo_id,
                match_id,
                validation.is_valid,
                serde_json::to_value(&validation).unwrap_or_default(),
            )
            .await?;
    }

    Ok(Json(DataResponse::new(
        ValidationResultResponse::from(validation),
        request_id,
    )))
}

/// Validate a claim against the catalog's own copy of the demo's result, when
/// the evidence has a catalog row behind it and that row has been parsed.
///
/// Returns `None` when there is nothing to compare against — no catalog demo,
/// or a catalogued demo whose stats have not been ingested yet — which is the
/// signal to fall back to the plugin route.
async fn catalog_validation_for(
    state: &EvidenceState,
    catalog_demo_id: Option<portal_core::DemoId>,
    match_: &portal_domain::entities::TournamentMatch,
    claimed_p1: i32,
    claimed_p2: i32,
) -> ApiResult<Option<portal_domain::entities::evidence::EvidenceValidation>> {
    let Some(demo_id) = catalog_demo_id else {
        return Ok(None);
    };
    let Ok(demo) = state.demo_service.get_demo(demo_id).await else {
        return Ok(None);
    };
    let Some(meta) = demo.metadata else {
        return Ok(None);
    };

    let players = state
        .demo_service
        .get_demo_players(demo_id)
        .await
        .unwrap_or_default();
    let context = build_evidence_context(state, match_).await?;

    Ok(Some(validate_against_catalog(
        &meta, &players, &context, claimed_p1, claimed_p2,
    )))
}

/// Compare a catalogued demo's parsed result against a claimed scoreline.
///
/// The catalog records scores per *demo team name*, and nothing in the schema
/// says which demo team is participant 1. So the demo's players are joined to
/// the match participants by Steam ID first; only if that join fails does this
/// fall back to an order-insensitive score comparison, and it says so in a
/// warning rather than silently guessing.
fn validate_against_catalog(
    meta: &portal_domain::entities::demo::ParsedDemoMetadata,
    players: &[portal_domain::entities::demo::DemoPlayer],
    context: &MatchEvidenceContext,
    claimed_p1: i32,
    claimed_p2: i32,
) -> portal_domain::entities::evidence::EvidenceValidation {
    use portal_core::types::evidence::ExtractedMatchResult;
    use portal_domain::entities::evidence::EvidenceValidation;

    /// Which demo team a participant's Steam IDs sit on, if any is decisive.
    fn team_of(
        participant: &ParticipantContext,
        players: &[portal_domain::entities::demo::DemoPlayer],
    ) -> Option<String> {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for p in players {
            if participant.steam_ids.iter().any(|s| s == &p.steam_id)
                && let Some(team) = p.team_name.as_deref()
            {
                *counts.entry(team).or_default() += 1;
            }
        }
        let mut best: Vec<(&str, usize)> = counts.into_iter().collect();
        best.sort_by(|a, b| b.1.cmp(&a.1));
        match best.as_slice() {
            [(team, _)] => Some((*team).to_owned()),
            [(team, n), (_, m), ..] if n > m => Some((*team).to_owned()),
            _ => None,
        }
    }

    let mut warnings = Vec::new();

    // Demo-side scores, oriented to participant 1 / participant 2 when the
    // Steam-ID join is decisive for both sides and puts them on two teams.
    let oriented = match (context.participants.first(), context.participants.get(1)) {
        (Some(p1), Some(p2)) => match (team_of(p1, players), team_of(p2, players)) {
            (Some(t1), Some(t2)) if t1 != t2 => {
                let score_of = |team: &str| {
                    if team == meta.team1_name {
                        Some(meta.team1_score)
                    } else if team == meta.team2_name {
                        Some(meta.team2_score)
                    } else {
                        None
                    }
                };
                match (score_of(&t1), score_of(&t2)) {
                    (Some(a), Some(b)) => Some((a, b)),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    };

    let (demo_p1, demo_p2) = if let Some(pair) = oriented {
        pair
    } else {
        warnings.push(
            "Could not map demo teams to match participants by Steam ID; scores compared without side assignment".to_string(),
        );
        (meta.team1_score, meta.team2_score)
    };

    let extracted = Some(ExtractedMatchResult {
        map_id: meta.map_name.clone(),
        participant1_score: demo_p1,
        participant2_score: demo_p2,
        duration_seconds: meta.duration_seconds.unwrap_or(0),
        player_stats: serde_json::Value::Null,
    });

    let exact = demo_p1 == claimed_p1 && demo_p2 == claimed_p2;
    let unordered = demo_p1 == claimed_p2 && demo_p2 == claimed_p1;

    if exact {
        return EvidenceValidation {
            is_valid: true,
            // Not 1.0: the catalog's copy of the result is only as good as the
            // stats submission that produced it.
            confidence: 0.9,
            extracted_result: extracted,
            warnings,
            errors: Vec::new(),
        };
    }

    if unordered && oriented.is_none() {
        warnings.push(format!(
            "Demo scores {demo_p1}-{demo_p2} match the claim {claimed_p1}-{claimed_p2} only with sides swapped"
        ));
        return EvidenceValidation {
            is_valid: true,
            confidence: 0.5,
            extracted_result: extracted,
            warnings,
            errors: Vec::new(),
        };
    }

    EvidenceValidation {
        is_valid: false,
        confidence: 0.0,
        extracted_result: extracted,
        warnings,
        errors: vec![format!(
            "Demo on {} records {demo_p1} - {demo_p2}, but the claimed result is {claimed_p1} - {claimed_p2}",
            meta.map_name
        )],
    }
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

    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await
        .ok()??;

    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await
        .ok()?;

    let league_slug = if let Some(lid) = tournament.league_id {
        state
            .league_service
            .get_league(lid)
            .await
            .ok()
            .map(|l| l.slug)
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
fn plugin_cache()
-> &'static dashmap::DashMap<portal_core::TournamentId, Arc<dyn portal_plugins::GamePlugin>> {
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

    let plugin = state.plugin_manager.get(&game.plugin_id).ok_or_else(|| {
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
            if let Ok(player) = state.player_service.get_player(pid).await
                && let Some(sid) = &player.steam_id
            {
                steam_ids.push(sid.clone());
            }
        }

        participants.push(ParticipantContext {
            registration_id: reg_id.as_uuid(),
            name: reg.participant_name,
            player_ids: player_ids
                .iter()
                .map(portal_core::PlayerId::as_uuid)
                .collect(),
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
        (status = 403, description = "Not a match participant", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn validate_demo(
    State(state): State<EvidenceState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Query(query): Query<GetDemoStatsQuery>,
    ValidatedJson(req): ValidatedJson<ValidateDemoRequest>,
) -> ApiResult<Json<DataResponse<DemoValidationResponse>>> {
    let request_id = get_request_id(&headers);

    // Demo validation reaches the CS2 plugin (and the external demo service)
    // with caller-chosen scores; bind it to the match.
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;
    require_match_participant_or_admin(&state, &perm_checker, &auth, match_id).await?;

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
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<TournamentMatchId>,
    ValidatedJson(req): ValidatedJson<LinkDemoRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    use portal_domain::entities::evidence::{
        DiscoveredEvidence, EvidenceSource, EvidenceStorage, EvidenceType,
    };

    let request_id = get_request_id(&headers);

    // P-108: same gap as `link_discovered_evidence` above — no participant check.
    require_match_participant_or_admin(&state, &perm_checker, &auth, match_id).await?;

    // P-110: resolve the demo from the SAME source of truth the button's list
    // came from.
    //
    // This handler used to resolve `req.demo_name` against the external CS2
    // demo-stats service and 404 when it was absent — while holding `demo_id`,
    // which it then used anyway, three lines further down, to create the link.
    // The list the "Link demo" button acts on is the **catalog** (`GET /v1/demos`),
    // so every catalogued demo whose `.stats.json` was missing was offered and
    // then refused, and in any deployment without the stats service running the
    // button refused everything. The catalog row carries the file name, the S3
    // coordinate, the size and the parsed map — everything this handler took
    // from the stats service — so when the caller names a catalog demo, ask the
    // catalog. The stats-service path stays for callers that only have a file
    // name (no `demo_id`), which is the only case it was ever needed for.
    let discovered = if let Some(ref demo_id_str) = req.demo_id {
        let demo_id: portal_core::DemoId = demo_id_str
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid demo_id format"))?;

        // 404s if the catalog row is gone — a real, checkable existence proof.
        let demo = state.demo_service.get_demo(demo_id).await?;

        state
            .demo_service
            .link_to_match(
                demo_id,
                match_id,
                req.game_number,
                portal_core::DemoLinkType::Evidence,
                Some(auth.user_id),
            )
            .await?;

        let map_name = demo.metadata.as_ref().map(|m| m.map_name.clone());
        DiscoveredEvidence {
            external_id: format!("demo:{}", demo.file_name),
            evidence_type: EvidenceType::Demo,
            name: demo.file_name.clone(),
            storage: EvidenceStorage::S3 {
                bucket: demo.s3_bucket.clone(),
                key: demo.s3_key.clone(),
            },
            file_size_bytes: demo.file_size_bytes,
            metadata: serde_json::json!({
                "demo_name": demo.file_name,
                "map": map_name,
                "description": req.description,
                "catalog_demo_id": demo_id.to_string(),
            }),
            discovered_at: chrono::Utc::now(),
            relevance_score: 1.0,
        }
    } else {
        let cs2_plugin = create_cs2_plugin(&state);
        let stats = cs2_plugin
            .get_demo_stats(&req.demo_name)
            .await
            .map_err(|_| ApiError::not_found(format!("Demo not found: {}", req.demo_name)))?;

        DiscoveredEvidence {
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
                "catalog_demo_id": serde_json::Value::Null,
            }),
            discovered_at: chrono::Utc::now(),
            relevance_score: 1.0,
        }
    };

    // P-109: a human clicked "Link demo"; the row is stamped as such.
    let evidence = state
        .evidence_service
        .link_discovered(
            match_id,
            discovered,
            req.game_number,
            auth.user_id,
            EvidenceSource::ManualUpload,
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
/// * **Capability-URL auth (no bearer token)** — this endpoint emulates an
///   S3 presigned PUT, and clients upload to it exactly as they would to S3:
///   with NO `Authorization` header (attaching one to a real presigned URL
///   breaks SigV4, so the frontend rightly never sends it). The capability
///   model is the same as a presigned URL's: the path embeds a
///   server-generated UUIDv7 that only the (authenticated and authorized)
///   `initiate_upload` caller was told. Dev-only — hard-disabled in S3 mode.
/// * **Path traversal rejected** — absolute paths, `..` components, and
///   non-UTF-8 paths are refused outright. After joining we canonicalize the
///   parent directory and verify it stays inside `state.uploads_path`.
/// * **Size capped** — bodies above [`LOCAL_EVIDENCE_MAX_BYTES`] are rejected.
pub async fn local_evidence_upload(
    State(state): State<EvidenceState>,
    axum::extract::Path(path): axum::extract::Path<String>,
    body: axum::body::Bytes,
) -> Result<StatusCode, ApiError> {
    // S3 mode: this endpoint should not be used. Refuse rather than silently
    // writing files that nothing will ever read.
    if std::env::var("EVIDENCE_STORAGE")
        .ok()
        .is_some_and(|v| v.eq_ignore_ascii_case("s3"))
    {
        return Err(ApiError::not_found(
            "Local upload endpoint disabled in S3 mode",
        ));
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
        return Err(ApiError::bad_request(
            "Parent directory components not allowed",
        ));
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
