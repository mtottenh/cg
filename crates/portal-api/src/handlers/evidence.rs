//! Evidence API handlers.
//!
//! Handlers for managing match evidence (demos, screenshots, videos, links).

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
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::Json;
use portal_core::{EvidenceId, TournamentMatchId};

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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    ValidatedJson(req): ValidatedJson<InitiateUploadRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<UploadInfoResponse>>)> {
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let evidence_type: portal_domain::entities::evidence::EvidenceType = req
        .evidence_type
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence type"))?;

    let upload_info = state
        .evidence_service
        .initiate_upload(
            match_id,
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    ValidatedJson(req): ValidatedJson<AddLinkEvidenceRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

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
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Query(query): Query<ListEvidenceQuery>,
) -> ApiResult<Json<DataResponse<Vec<EvidenceSummaryResponse>>>> {
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let evidence = if let Some(game_number) = query.game_number {
        state
            .evidence_service
            .get_game_evidence(match_id, game_number)
            .await?
    } else {
        state.evidence_service.get_match_evidence(match_id).await?
    };

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
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((match_id, evidence_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<EvidenceResponse>>> {
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;
    let evidence_id: EvidenceId = evidence_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))?;

    // Get all evidence for the match and find the specific one
    let evidence_list = state.evidence_service.get_match_evidence(match_id).await?;
    let evidence = evidence_list
        .into_iter()
        .find(|e| e.id == evidence_id)
        .ok_or_else(|| ApiError::not_found("Evidence not found"))?;

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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((_match_id, evidence_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let evidence_id: EvidenceId = evidence_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence ID format"))?;

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
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(match_id): Path<String>,
    Query(_query): Query<DiscoverEvidenceQuery>,
) -> ApiResult<Json<DataResponse<Vec<DiscoveredEvidenceResponse>>>> {
    let _match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    // Evidence discovery requires game-specific plugin integration
    // which is part of a future implementation phase
    Err(ApiError::not_implemented(
        "Evidence discovery requires plugin integration (coming soon)",
    ))
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
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(match_id): Path<String>,
    ValidatedJson(_req): ValidatedJson<LinkDiscoveredEvidenceRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    let _match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    // Linking discovered evidence requires the discovery feature
    // which uses game-specific plugin integration
    Err(ApiError::not_implemented(
        "Linking discovered evidence requires plugin integration (coming soon)",
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
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(match_id): Path<String>,
    ValidatedJson(_req): ValidatedJson<ValidateEvidenceRequest>,
) -> ApiResult<Json<DataResponse<ValidationResultResponse>>> {
    let _match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    // Evidence validation requires game-specific plugin integration
    // to parse demos/replays and verify claimed results
    Err(ApiError::not_implemented(
        "Evidence validation requires plugin integration (coming soon)",
    ))
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
    State(_state): State<AppState>,
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
    let cs2_plugin = Cs2PluginWithEvidence::new();
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
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path((_match_id, demo_name)): Path<(String, String)>,
    Query(_query): Query<GetDemoStatsQuery>,
) -> ApiResult<Json<DataResponse<DemoStatsResponse>>> {
    let request_id = get_request_id(&headers);

    let cs2_plugin = Cs2PluginWithEvidence::new();

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
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    ValidatedJson(req): ValidatedJson<LinkDemoRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<EvidenceResponse>>)> {
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let cs2_plugin = Cs2PluginWithEvidence::new();

    // Verify demo exists by fetching stats
    let stats = cs2_plugin
        .get_demo_stats(&req.demo_name)
        .await
        .map_err(|_| ApiError::not_found(format!("Demo not found: {}", req.demo_name)))?;

    let evidence_type: portal_domain::entities::evidence::EvidenceType = "video"
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid evidence type"))?;

    // Create evidence record using add_link (demo URL as external link)
    let evidence = state
        .evidence_service
        .add_link(
            match_id,
            req.game_number,
            evidence_type,
            cs2_plugin.get_demo_url(&req.demo_name),
            format!("CS2 Demo: {}", stats.map),
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
