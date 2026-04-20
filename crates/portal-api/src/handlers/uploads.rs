//! File upload handlers.

use crate::dto::common::DataResponse;
use crate::dto::responses::{LeagueTeamResponse, PlayerResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::UploadsState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use axum_extra::extract::Multipart;
use bytes::Bytes;
use portal_core::{permissions, LeagueTeamId};
use portal_domain::entities::league_team::UpdateLeagueTeamCommand;
use portal_domain::repositories::UpdatePlayer;
use portal_storage::image::{ImageProcessor, ImageType};
use portal_storage::StoreRequest;
use serde::Serialize;
use utoipa::ToSchema;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Upload result response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UploadResponse {
    /// URL of the uploaded file.
    pub url: String,
    /// URL of the thumbnail (if generated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

/// Extract file data from multipart request.
async fn extract_file(mut multipart: Multipart) -> Result<(String, Bytes), ApiError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to read multipart: {e}")))?
    {
        if field.name() == Some("file") {
            let filename = field
                .file_name().map_or_else(|| "upload".to_string(), String::from);

            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("Failed to read file: {e}")))?;

            return Ok((filename, data));
        }
    }

    Err(ApiError::bad_request("No file field in multipart request"))
}

/// Upload player avatar.
#[utoipa::path(
    post,
    path = "/v1/players/me/avatar",
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Avatar uploaded", body = DataResponse<PlayerResponse>),
        (status = 400, description = "Invalid image", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "players"
)]
pub async fn upload_player_avatar(
    State(state): State<UploadsState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    multipart: Multipart,
) -> ApiResult<Json<DataResponse<PlayerResponse>>> {
    let request_id = get_request_id(&headers);

    // Extract and process file
    let (filename, data) = extract_file(multipart).await?;
    let config = ImageType::PlayerAvatar.config();

    let processed = ImageProcessor::process(&data, &config).map_err(|e| {
        tracing::debug!("Image processing error: {:?}", e);
        ApiError::bad_request(format!("Invalid image: {e}"))
    })?;

    // Store the image
    let stored = state
        .storage
        .store(StoreRequest {
            data: processed.main,
            filename,
            content_type: processed.content_type,
            prefix: ImageType::PlayerAvatar.prefix().to_string(),
            owner_id: Some(auth.player_id.to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store image: {e}")))?;

    // Update player with new avatar URL
    let update = UpdatePlayer {
        avatar_url: Some(stored.url),
        ..Default::default()
    };

    let player = state
        .player_service
        .update_profile(auth.player_id, update)
        .await?;

    Ok(Json(DataResponse::new(PlayerResponse::from(player), request_id)))
}

/// Upload player banner.
#[utoipa::path(
    post,
    path = "/v1/players/me/banner",
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Banner uploaded", body = DataResponse<PlayerResponse>),
        (status = 400, description = "Invalid image", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "players"
)]
pub async fn upload_player_banner(
    State(state): State<UploadsState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    multipart: Multipart,
) -> ApiResult<Json<DataResponse<PlayerResponse>>> {
    let request_id = get_request_id(&headers);

    // Extract and process file
    let (filename, data) = extract_file(multipart).await?;
    let config = ImageType::PlayerBanner.config();

    let processed = ImageProcessor::process(&data, &config).map_err(|e| {
        tracing::debug!("Image processing error: {:?}", e);
        ApiError::bad_request(format!("Invalid image: {e}"))
    })?;

    // Store the image
    let stored = state
        .storage
        .store(StoreRequest {
            data: processed.main,
            filename,
            content_type: processed.content_type,
            prefix: ImageType::PlayerBanner.prefix().to_string(),
            owner_id: Some(auth.player_id.to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store image: {e}")))?;

    // Update player with new banner URL
    let update = UpdatePlayer {
        banner_url: Some(stored.url),
        ..Default::default()
    };

    let player = state
        .player_service
        .update_profile(auth.player_id, update)
        .await?;

    Ok(Json(DataResponse::new(PlayerResponse::from(player), request_id)))
}

/// Shared worker for team-image uploads. Handles auth (team settings manage
/// permission), file extraction + image processing for the given `image_type`,
/// stores the image, writes the URL back via `update_team_authorized`, and
/// returns the refreshed team.
async fn upload_team_image(
    state: UploadsState,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    team_id: LeagueTeamId,
    multipart: Multipart,
    image_type: ImageType,
    request_id: &str,
    mut apply: impl FnMut(&mut UpdateLeagueTeamCommand, String),
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    perm.require_team_permission(
        &auth,
        team_id.as_uuid(),
        permissions::team::SETTINGS_MANAGE,
    )
    .await?;

    let (filename, data) = extract_file(multipart).await?;
    let config = image_type.config();

    let processed = ImageProcessor::process(&data, &config).map_err(|e| {
        tracing::debug!("Image processing error: {:?}", e);
        ApiError::bad_request(format!("Invalid image: {e}"))
    })?;

    let stored = state
        .storage
        .store(StoreRequest {
            data: processed.main,
            filename,
            content_type: processed.content_type,
            prefix: image_type.prefix().to_string(),
            owner_id: Some(team_id.to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store image: {e}")))?;

    let mut cmd = UpdateLeagueTeamCommand::default();
    apply(&mut cmd, stored.url);

    let updated = state
        .league_team_service
        .update_team_authorized(team_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(
        LeagueTeamResponse::from(updated),
        request_id,
    )))
}

/// Upload league-team logo.
#[utoipa::path(
    post,
    path = "/v1/league-teams/{team_id}/logo",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Logo uploaded", body = DataResponse<LeagueTeamResponse>),
        (status = 400, description = "Invalid image", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - requires team.settings.manage", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn upload_team_logo(
    State(state): State<UploadsState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
    multipart: Multipart,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    upload_team_image(
        state,
        auth,
        perm,
        team_id,
        multipart,
        ImageType::TeamLogo,
        &request_id,
        |cmd, url| cmd.logo_url = Some(url),
    )
    .await
}

/// Upload league-team banner.
#[utoipa::path(
    post,
    path = "/v1/league-teams/{team_id}/banner",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Banner uploaded", body = DataResponse<LeagueTeamResponse>),
        (status = 400, description = "Invalid image", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Forbidden - requires team.settings.manage", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league-teams"
)]
pub async fn upload_team_banner(
    State(state): State<UploadsState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    headers: HeaderMap,
    Path(team_id): Path<LeagueTeamId>,
    multipart: Multipart,
) -> ApiResult<Json<DataResponse<LeagueTeamResponse>>> {
    let request_id = get_request_id(&headers).to_string();
    upload_team_image(
        state,
        auth,
        perm,
        team_id,
        multipart,
        ImageType::TeamBanner,
        &request_id,
        |cmd, url| cmd.banner_url = Some(url),
    )
    .await
}
