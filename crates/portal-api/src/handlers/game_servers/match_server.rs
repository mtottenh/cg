//! Match-facing server endpoints: admin assign/cancel + connect info.
//!
//! Design: docs/matchzy-integration.md §6.2 (route surface), §7.2 (panel
//! states the GET feeds).

use crate::dto::common::DataResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::game_server_flow;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::ids::TournamentMatchId;
use portal_core::permissions;
use portal_core::types::ReservationStatus;
use portal_domain::repositories::{
    LeagueTeamMemberRepository, ServerEventRepository, ServerReservationRepository,
    TournamentMatchRepository,
};
use serde::Serialize;
use utoipa::ToSchema;

fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Live per-map score snapshot.
#[derive(Debug, Serialize, ToSchema)]
pub struct LiveScoreResponse {
    /// 0-based map number (MatchZy convention).
    pub map_number: i64,
    pub team1_score: i32,
    pub team2_score: i32,
    pub round_number: Option<i64>,
}

/// A match's server reservation, scoped to what the caller may see.
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchServerResponse {
    /// Reservation status.
    pub status: ReservationStatus,
    /// Failure/cancellation reason, when terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Server display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    /// Connect address — participants and admins only, once ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// `sv_password` — participants and admins only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_password: Option<String>,
    /// GOTV details — visible to everyone once the match is live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotv_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotv_password: Option<String>,
    /// Whether the caller received the participant view.
    pub is_participant: bool,
    /// Latest live score, when the match is live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_score: Option<LiveScoreResponse>,
}

/// Whether the calling user plays in this match (roster or solo reg).
async fn is_participant(
    state: &AppState,
    match_id: TournamentMatchId,
    user: &AuthenticatedUser,
) -> Result<bool, ApiError> {
    let Some(match_) = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await
        .map_err(ApiError::from)?
    else {
        return Ok(false);
    };
    for reg_id in [
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ]
    .into_iter()
    .flatten()
    {
        let Ok(reg) = state.registration_service.get_registration(reg_id).await else {
            continue;
        };
        if reg.player_id == Some(user.player_id) {
            return Ok(true);
        }
        if let Some(team_season_id) = reg.team_season_id
            && let Ok(members) = state
                .league_team_member_repo
                .list_members_with_players(team_season_id)
                .await
            && members.iter().any(|m| m.player_id == user.player_id)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Get the server reservation for a match.
///
/// Participants and admins receive connect details once the server is
/// ready; everyone else gets status + GOTV once live.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/server",
    params(("match_id" = String, Path, description = "Match ID")),
    responses(
        (status = 200, description = "Reservation state", body = DataResponse<MatchServerResponse>),
        (status = 404, description = "No reservation for this match", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn get_match_server(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<MatchServerResponse>>> {
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))?;

    let reservation = state
        .server_reservation_repo
        .find_latest_by_match(match_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("this match has no server reservation"))?;

    let is_admin = perm_checker
        .has_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await;
    let participant = is_admin || is_participant(&state, match_id, &auth).await?;

    let server = match reservation.server_id {
        Some(id) => state.game_server_registry.get(id).await.ok(),
        None => None,
    };

    let connect_visible = participant
        && matches!(
            reservation.status,
            ReservationStatus::Ready | ReservationStatus::Live
        );
    let gotv_visible = matches!(reservation.status, ReservationStatus::Live) || connect_visible;

    let live_score = if reservation.status == ReservationStatus::Live {
        state
            .server_event_repo
            .latest_round_end(reservation.id)
            .await
            .ok()
            .flatten()
            .map(|event| {
                let score = |team: &str| {
                    event
                        .payload
                        .get(team)
                        .and_then(|t| t.get("score"))
                        .and_then(serde_json::Value::as_i64)
                        .and_then(|s| i32::try_from(s).ok())
                        .unwrap_or(0)
                };
                LiveScoreResponse {
                    map_number: event.map_number.map_or(0, i64::from),
                    team1_score: score("team1"),
                    team2_score: score("team2"),
                    round_number: event.round_number.map(i64::from),
                }
            })
    } else {
        None
    };

    let response = MatchServerResponse {
        status: reservation.status,
        failure_reason: reservation.failure_reason.clone(),
        server_name: server.as_ref().map(|s| s.name.clone()),
        ip_address: connect_visible
            .then(|| server.as_ref().map(|s| s.ip_address.to_string()))
            .flatten(),
        port: connect_visible
            .then(|| server.as_ref().map(|s| s.port))
            .flatten(),
        connect_password: connect_visible.then(|| reservation.connect_password.clone()),
        gotv_port: gotv_visible
            .then(|| server.as_ref().and_then(|s| s.gotv_port))
            .flatten(),
        gotv_password: gotv_visible
            .then(|| reservation.gotv_password.clone())
            .flatten(),
        is_participant: participant,
        live_score,
    };
    Ok(Json(DataResponse::new(response, request_id)))
}

/// Manually assign a server to a match (admin).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/server/assign",
    params(("match_id" = String, Path, description = "Match ID")),
    responses(
        (status = 201, description = "Reservation created/queued", body = DataResponse<MatchServerResponse>),
        (status = 400, description = "Match not ready for a server", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn assign_match_server(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<(StatusCode, Json<DataResponse<MatchServerResponse>>)> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))?;

    let reservation = game_server_flow::request_assignment(&state, match_id)
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(
            MatchServerResponse {
                status: reservation.status,
                failure_reason: reservation.failure_reason,
                server_name: None,
                ip_address: None,
                port: None,
                connect_password: None,
                gotv_port: None,
                gotv_password: None,
                is_participant: true,
                live_score: None,
            },
            request_id,
        )),
    ))
}

/// Cancel a match's server reservation (admin).
#[utoipa::path(
    delete,
    path = "/v1/matches/{match_id}/server",
    params(("match_id" = String, Path, description = "Match ID")),
    responses(
        (status = 204, description = "Reservation cancelled"),
        (status = 400, description = "No live reservation", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn cancel_match_server(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    Path(match_id): Path<String>,
) -> ApiResult<StatusCode> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))?;

    game_server_flow::cancel_assignment(&state, match_id, "cancelled by admin")
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Request body for a backup restore.
#[derive(Debug, serde::Deserialize, ToSchema)]
pub struct RestoreBackupRequest {
    /// Restore the newest backup at or before this round; omit for latest.
    pub before_round: Option<i32>,
}

/// Result of a restore.
#[derive(Debug, Serialize, ToSchema)]
pub struct RestoreBackupResponse {
    /// The backup file that was loaded.
    pub filename: String,
}

/// Restore a round backup onto the match's server (admin, Phase 4).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/server/restore",
    params(("match_id" = String, Path, description = "Match ID")),
    request_body = RestoreBackupRequest,
    responses(
        (status = 200, description = "Backup restored", body = DataResponse<RestoreBackupResponse>),
        (status = 400, description = "No backups / no live reservation", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn restore_match_server(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Json(body): Json<RestoreBackupRequest>,
) -> ApiResult<Json<DataResponse<RestoreBackupResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))?;

    tracing::info!(admin = %auth.username, %match_id, before_round = ?body.before_round,
        "admin backup restore");
    let filename = game_server_flow::restore_backup(&state, match_id, body.before_round)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(DataResponse::new(
        RestoreBackupResponse { filename },
        request_id,
    )))
}
