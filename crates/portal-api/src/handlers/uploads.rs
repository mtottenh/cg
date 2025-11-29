//! File upload handlers.

use crate::dto::common::DataResponse;
use crate::dto::responses::{PlayerResponse, TeamResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use axum_extra::extract::Multipart;
use bytes::Bytes;
use portal_core::TeamId;
use portal_domain::entities::team::UpdateTeamCommand;
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
                .file_name()
                .map(String::from)
                .unwrap_or_else(|| "upload".to_string());

            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("Failed to read file: {e}")))?;

            return Ok((filename, data));
        }
    }

    Err(ApiError::bad_request("No file field in multipart request"))
}

/// Upload team logo.
#[utoipa::path(
    post,
    path = "/v1/teams/{team_id}/logo",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Logo uploaded", body = DataResponse<TeamResponse>),
        (status = 400, description = "Invalid image", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a team captain", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn upload_team_logo(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    multipart: Multipart,
) -> ApiResult<Json<DataResponse<TeamResponse>>> {
    let request_id = get_request_id(&headers);

    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    // Verify user is captain
    let is_captain = state
        .team_service
        .is_captain(team_id, auth.player_id)
        .await?;

    if !is_captain {
        return Err(ApiError::forbidden("Only team captains can upload logos"));
    }

    // Extract and process file
    let (filename, data) = extract_file(multipart).await?;
    let config = ImageType::TeamLogo.config();

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
            prefix: ImageType::TeamLogo.prefix().to_string(),
            owner_id: Some(team_id.to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store image: {e}")))?;

    // Update team with new logo URL
    let cmd = UpdateTeamCommand {
        name: None,
        tag: None,
        description: None,
        logo_url: Some(stored.url),
        banner_url: None,
        primary_color: None,
        secondary_color: None,
        website_url: None,
    };

    let team = state
        .team_service
        .update_team_authorized(team_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(TeamResponse::from(team), request_id)))
}

/// Upload team banner.
#[utoipa::path(
    post,
    path = "/v1/teams/{team_id}/banner",
    params(
        ("team_id" = String, Path, description = "Team ID")
    ),
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Banner uploaded", body = DataResponse<TeamResponse>),
        (status = 400, description = "Invalid image", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not a team captain", body = ApiError),
        (status = 404, description = "Team not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn upload_team_banner(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    multipart: Multipart,
) -> ApiResult<Json<DataResponse<TeamResponse>>> {
    let request_id = get_request_id(&headers);

    let team_id: TeamId = team_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid team ID format"))?;

    // Verify user is captain
    let is_captain = state
        .team_service
        .is_captain(team_id, auth.player_id)
        .await?;

    if !is_captain {
        return Err(ApiError::forbidden("Only team captains can upload banners"));
    }

    // Extract and process file
    let (filename, data) = extract_file(multipart).await?;
    let config = ImageType::TeamBanner.config();

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
            prefix: ImageType::TeamBanner.prefix().to_string(),
            owner_id: Some(team_id.to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store image: {e}")))?;

    // Update team with new banner URL
    let cmd = UpdateTeamCommand {
        name: None,
        tag: None,
        description: None,
        logo_url: None,
        banner_url: Some(stored.url),
        primary_color: None,
        secondary_color: None,
        website_url: None,
    };

    let team = state
        .team_service
        .update_team_authorized(team_id, cmd)
        .await?;

    Ok(Json(DataResponse::new(TeamResponse::from(team), request_id)))
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
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
