//! File upload handlers.

use crate::dto::common::DataResponse;
use crate::dto::responses::PlayerResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use axum_extra::extract::Multipart;
use bytes::Bytes;
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
