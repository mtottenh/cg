//! Notification endpoints: a user's own notifications, and an organiser's
//! Discord delivery settings for a tournament.

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use portal_core::{NotificationId, TournamentId};
use portal_domain::entities::notification::{Notification, TournamentNotificationSettings};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

fn request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// One notification.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationResponse {
    pub id: String,
    /// What it is about, e.g. `proposal_received`, `checkin_open`,
    /// `server_ready`.
    pub kind: String,
    pub title: String,
    pub body: String,
    /// SPA path to open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_id: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<DateTime<Utc>>,
}

impl From<Notification> for NotificationResponse {
    fn from(n: Notification) -> Self {
        Self {
            id: n.id.to_string(),
            kind: n.kind,
            title: n.title,
            body: n.body,
            link: n.link,
            tournament_id: n.tournament_id.map(|t| t.to_string()),
            match_id: n.match_id.map(|m| m.to_string()),
            created_at: n.created_at,
            read_at: n.read_at,
        }
    }
}

/// A page of the caller's notifications, newest first.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationListResponse {
    pub items: Vec<NotificationResponse>,
    pub unread_count: i64,
    pub has_more: bool,
}

/// Unread count, and the newest unread notification.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationUnreadCountResponse {
    pub unread_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_id: Option<String>,
}

/// Query for listing notifications.
#[derive(Debug, Deserialize, IntoParams)]
pub struct NotificationListQuery {
    /// Only unread notifications.
    #[serde(default)]
    pub unread_only: bool,
    /// Page size, at most 100 (default 30).
    pub limit: Option<i64>,
    /// Return notifications older than this one.
    pub before: Option<String>,
}

/// Which notifications to mark read: `ids`, or `all`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkNotificationsReadRequest {
    /// At most 100 ids.
    pub ids: Option<Vec<String>>,
    pub all: Option<bool>,
}

/// How many notifications were marked read.
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkNotificationsReadResponse {
    pub updated: u64,
}

/// A tournament's Discord delivery settings. The webhook URL itself is never
/// returned: it lets anyone post to the channel.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationSettingsResponse {
    pub discord_enabled: bool,
    pub discord_webhook_configured: bool,
    /// The last 6 characters of the stored webhook URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_webhook_hint: Option<String>,
}

impl From<Option<TournamentNotificationSettings>> for NotificationSettingsResponse {
    fn from(s: Option<TournamentNotificationSettings>) -> Self {
        let url = s.as_ref().and_then(|s| s.discord_webhook_url.as_deref());
        Self {
            discord_enabled: s.as_ref().is_some_and(|s| s.discord_enabled),
            discord_webhook_configured: url.is_some(),
            discord_webhook_hint: url.map(|u| {
                let chars: Vec<char> = u.chars().collect();
                chars[chars.len().saturating_sub(6)..].iter().collect()
            }),
        }
    }
}

/// Update a tournament's Discord delivery settings.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNotificationSettingsRequest {
    pub discord_enabled: bool,
    /// Omit to keep the stored URL; `null` or `""` to remove it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub discord_webhook_url: Option<Option<String>>,
}

/// Distinguish an absent field (`None`) from an explicit `null`
/// (`Some(None)`): the API contract gives the two different meanings (keep
/// the stored URL vs remove it).
#[allow(clippy::option_option)]
fn double_option<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(de).map(Some)
}

/// Whether a test message reached Discord.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationTestResponse {
    pub delivered: bool,
    /// What went wrong (HTTP status or error), never the URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// List the caller's notifications.
#[utoipa::path(
    get,
    path = "/v1/me/notifications",
    params(NotificationListQuery),
    responses(
        (status = 200, description = "The caller's notifications, newest first", body = DataResponse<NotificationListResponse>),
        (status = 400, description = "Invalid cursor", body = ApiError),
        (status = 401, description = "Not logged in", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "notifications"
)]
pub async fn list_my_notifications(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(q): Query<NotificationListQuery>,
) -> ApiResult<Json<DataResponse<NotificationListResponse>>> {
    let before: Option<NotificationId> = q
        .before
        .as_deref()
        .map(|b| {
            b.parse()
                .map_err(|_| ApiError::bad_request("Invalid notification ID format"))
        })
        .transpose()?;
    let page = state
        .notifier
        .service
        .list(auth.user_id, q.unread_only, q.limit.unwrap_or(30), before)
        .await?;
    Ok(Json(DataResponse::new(
        NotificationListResponse {
            items: page.items.into_iter().map(Into::into).collect(),
            unread_count: page.unread_count,
            has_more: page.has_more,
        },
        request_id(&headers),
    )))
}

/// The caller's unread notification count.
#[utoipa::path(
    get,
    path = "/v1/me/notifications/unread-count",
    responses(
        (status = 200, description = "Unread count and the newest unread id", body = DataResponse<NotificationUnreadCountResponse>),
        (status = 401, description = "Not logged in", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "notifications"
)]
pub async fn my_unread_count(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<NotificationUnreadCountResponse>>> {
    let (unread_count, latest) = state.notifier.service.unread_count(auth.user_id).await?;
    Ok(Json(DataResponse::new(
        NotificationUnreadCountResponse {
            unread_count,
            latest_id: latest.map(|l| l.to_string()),
        },
        request_id(&headers),
    )))
}

/// Mark the caller's notifications read.
#[utoipa::path(
    post,
    path = "/v1/me/notifications/read",
    request_body = MarkNotificationsReadRequest,
    responses(
        (status = 200, description = "How many were marked read", body = DataResponse<MarkNotificationsReadResponse>),
        (status = 400, description = "Neither or both of `ids` and `all`, or more than 100 ids", body = ApiError),
        (status = 401, description = "Not logged in", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "notifications"
)]
pub async fn mark_my_notifications_read(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Json(req): Json<MarkNotificationsReadRequest>,
) -> ApiResult<Json<DataResponse<MarkNotificationsReadResponse>>> {
    let all = req.all.unwrap_or(false);
    let updated = match (req.ids, all) {
        (Some(ids), false) => {
            if ids.is_empty() || ids.len() > 100 {
                return Err(ApiError::bad_request("Send between 1 and 100 ids"));
            }
            let ids = ids
                .iter()
                .map(|i| {
                    i.parse::<NotificationId>()
                        .map_err(|_| ApiError::bad_request("Invalid notification ID format"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            state
                .notifier
                .service
                .mark_read(auth.user_id, Some(&ids))
                .await?
        }
        (None, true) => state.notifier.service.mark_read(auth.user_id, None).await?,
        _ => {
            return Err(ApiError::bad_request(
                "Send either `ids` or `all: true`, not both",
            ));
        }
    };
    Ok(Json(DataResponse::new(
        MarkNotificationsReadResponse { updated },
        request_id(&headers),
    )))
}

fn parse_tournament_id(raw: &str) -> Result<TournamentId, ApiError> {
    raw.parse()
        .map_err(|_| ApiError::bad_request("Invalid tournament ID format"))
}

async fn require_settings_permission(
    state: &AppState,
    perm_checker: &PermissionChecker,
    auth: &AuthenticatedUser,
    tournament_id: TournamentId,
) -> Result<(), ApiError> {
    // 404 before 403, so an unknown id is not mistaken for a permission
    // problem.
    state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;
    perm_checker
        .require_tournament_permission(
            auth,
            tournament_id.as_uuid(),
            portal_core::permissions::tournament::SETTINGS_MANAGE,
        )
        .await
}

/// Read a tournament's notification delivery settings.
#[utoipa::path(
    get,
    path = "/v1/admin/tournaments/{tournament_id}/notification-settings",
    params(("tournament_id" = String, Path, description = "Tournament ID")),
    responses(
        (status = 200, description = "Delivery settings (the webhook URL is never returned)", body = DataResponse<NotificationSettingsResponse>),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "notifications"
)]
pub async fn get_notification_settings(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<NotificationSettingsResponse>>> {
    let tournament_id = parse_tournament_id(&tournament_id)?;
    require_settings_permission(&state, &perm_checker, &auth, tournament_id).await?;
    let settings = state.notifier.service.settings(tournament_id).await?;
    Ok(Json(DataResponse::new(
        settings.into(),
        request_id(&headers),
    )))
}

/// Save a tournament's notification delivery settings.
#[utoipa::path(
    put,
    path = "/v1/admin/tournaments/{tournament_id}/notification-settings",
    params(("tournament_id" = String, Path, description = "Tournament ID")),
    request_body = UpdateNotificationSettingsRequest,
    responses(
        (status = 200, description = "Saved", body = DataResponse<NotificationSettingsResponse>),
        (status = 400, description = "Not a Discord webhook URL, or Discord turned on without one", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "notifications"
)]
pub async fn update_notification_settings(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<String>,
    Json(req): Json<UpdateNotificationSettingsRequest>,
) -> ApiResult<Json<DataResponse<NotificationSettingsResponse>>> {
    let tournament_id = parse_tournament_id(&tournament_id)?;
    require_settings_permission(&state, &perm_checker, &auth, tournament_id).await?;
    let saved = state
        .notifier
        .service
        .save_settings(
            tournament_id,
            req.discord_enabled,
            req.discord_webhook_url,
            auth.user_id,
        )
        .await?;
    Ok(Json(DataResponse::new(
        Some(saved).into(),
        request_id(&headers),
    )))
}

/// Send a test message to the tournament's Discord webhook.
#[utoipa::path(
    post,
    path = "/v1/admin/tournaments/{tournament_id}/notification-settings/test",
    params(("tournament_id" = String, Path, description = "Tournament ID")),
    responses(
        (status = 200, description = "Whether Discord accepted the message", body = DataResponse<NotificationTestResponse>),
        (status = 400, description = "No webhook configured", body = ApiError),
        (status = 403, description = "Forbidden", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "notifications"
)]
pub async fn test_notification_settings(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<String>,
) -> ApiResult<Json<DataResponse<NotificationTestResponse>>> {
    let tournament_id = parse_tournament_id(&tournament_id)?;
    require_settings_permission(&state, &perm_checker, &auth, tournament_id).await?;
    let url = state
        .notifier
        .service
        .settings(tournament_id)
        .await?
        .and_then(|s| s.discord_webhook_url)
        .ok_or_else(|| ApiError::bad_request("No Discord webhook is configured"))?;
    let tournament = state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;
    let outcome = state
        .notifier
        .post_to_discord(
            &url,
            &format!("**{}** — Test message from the portal", tournament.name),
        )
        .await;
    Ok(Json(DataResponse::new(
        match outcome {
            Ok(()) => NotificationTestResponse {
                delivered: true,
                detail: None,
            },
            Err(detail) => NotificationTestResponse {
                delivered: false,
                detail: Some(detail),
            },
        },
        request_id(&headers),
    )))
}
