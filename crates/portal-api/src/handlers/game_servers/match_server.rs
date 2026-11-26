//! Match-facing server endpoints: admin assign/cancel + connect info.
//!
//! Design: docs/matchzy-integration.md §6.2 (route surface), §7.2 (panel
//! states the GET feeds).

use crate::admin_rescue::{self, RosterCheckResponse, ServerClearResponse};
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
use portal_domain::repositories::match_rescue::RescueAudit;
use portal_domain::repositories::pug::AdhocTeamRepository as _;
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
    /// GOTV details — visible to every authenticated user once the match is
    /// live (participants and admins from `ready`). `gotv_host` is the
    /// address spectators connect to; it is deliberately separate from
    /// `ip_address`, which stays participant-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotv_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotv_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gotv_password: Option<String>,
    /// Whether the caller received the participant view.
    pub is_participant: bool,
    /// 1-based place in the server queue, when `pending` (§6.6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_position: Option<i64>,
    /// The server holding the reservation — admins only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// Failed load attempts so far — admins only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<i32>,
    /// Latest live score, when the match is live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_score: Option<LiveScoreResponse>,
}

/// Whether the calling user plays in this match (roster or solo reg).
pub(super) async fn is_participant(
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
        // Ad-hoc rosters (PUG containers)
        if let Some(adhoc_uuid) = reg.adhoc_team_id
            && state
                .adhoc_team_repo
                .is_member(
                    portal_core::AdhocTeamId::from_uuid(adhoc_uuid),
                    user.player_id,
                )
                .await
                .unwrap_or(false)
        {
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

    let queue_position = if reservation.status == ReservationStatus::Pending {
        state
            .server_reservation_repo
            .count_pending_before(reservation.created_at)
            .await
            .ok()
            .map(|ahead| ahead + 1)
    } else {
        None
    };

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
        gotv_host: gotv_visible
            .then(|| {
                server
                    .as_ref()
                    .filter(|s| s.gotv_port.is_some())
                    .map(|s| s.ip_address.to_string())
            })
            .flatten(),
        gotv_port: gotv_visible
            .then(|| server.as_ref().and_then(|s| s.gotv_port))
            .flatten(),
        gotv_password: gotv_visible
            .then(|| reservation.gotv_password.clone())
            .flatten(),
        is_participant: participant,
        queue_position,
        server_id: is_admin
            .then(|| reservation.server_id.map(|id| id.to_string()))
            .flatten(),
        retry_count: is_admin.then_some(reservation.retry_count),
        live_score,
    };
    Ok(Json(DataResponse::new(response, request_id)))
}

/// The admin view of a reservation that was just created or re-created:
/// status, failure reason and the server it landed on. Connect details are
/// left to `GET /v1/matches/{match_id}/server`, which applies the visibility
/// rules once the server reports ready.
async fn admin_reservation_summary(
    state: &AppState,
    reservation: portal_domain::entities::ServerReservation,
) -> MatchServerResponse {
    let server_name = match reservation.server_id {
        Some(id) => state
            .game_server_registry
            .get(id)
            .await
            .ok()
            .map(|s| s.name),
        None => None,
    };
    let queue_position = if reservation.status == ReservationStatus::Pending {
        state
            .server_reservation_repo
            .count_pending_before(reservation.created_at)
            .await
            .ok()
            .map(|ahead| ahead + 1)
    } else {
        None
    };
    MatchServerResponse {
        status: reservation.status,
        failure_reason: reservation.failure_reason,
        server_name,
        ip_address: None,
        port: None,
        connect_password: None,
        gotv_host: None,
        gotv_port: None,
        gotv_password: None,
        is_participant: true,
        queue_position,
        server_id: reservation.server_id.map(|id| id.to_string()),
        retry_count: Some(reservation.retry_count),
        live_score: None,
    }
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

    let response = admin_reservation_summary(&state, reservation).await;
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(response, request_id)),
    ))
}

/// Clear a match off its game server (admin).
///
/// Ends the match loaded on the server — which drops MatchZy's player
/// whitelist, so anyone can connect again — and releases the match's live
/// reservation if it has one. Also works when the reservation has already
/// failed or been cancelled but the config is still loaded, as long as the
/// server has not moved on to another match.
///
/// `server_cleared` is false, with `detail` saying why, when the server's
/// agent could not be reached or refused the command; the reservation is
/// released either way.
#[utoipa::path(
    delete,
    path = "/v1/matches/{match_id}/server",
    params(("match_id" = String, Path, description = "Match ID")),
    responses(
        (status = 200, description = "What was released and cleared", body = DataResponse<ServerClearResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn cancel_match_server(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<ServerClearResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))?;

    tracing::info!(admin = %auth.username, %match_id, "admin clearing match from its game server");
    let outcome = admin_rescue::clear_server(&state, match_id, "cancelled by admin").await?;
    Ok(Json(DataResponse::new(outcome, request_id)))
}

/// Body of a server config reload.
#[derive(Debug, serde::Deserialize, ToSchema)]
pub struct ReloadMatchServerRequest {
    /// Why the config is being reloaded. Recorded on the match's status log.
    pub reason: String,
}

/// Rebuild the match config and reload it on a server (admin).
///
/// For a match whose loaded config is wrong — a roster changed, a Steam ID
/// was linked after the fact, the load failed. Checks that a config can be
/// built from the rosters as they stand now (400 with the reason if not,
/// before anything is torn down), then clears the server, drops the old
/// reservation and creates and allocates a new one.
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/server/reload",
    params(("match_id" = String, Path, description = "Match ID")),
    request_body = ReloadMatchServerRequest,
    responses(
        (status = 200, description = "New reservation created/queued", body = DataResponse<MatchServerResponse>),
        (status = 400, description = "The config cannot be built, or the match needs no server", body = ApiError),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn reload_match_server(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
    Json(body): Json<ReloadMatchServerRequest>,
) -> ApiResult<Json<DataResponse<MatchServerResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))?;
    let reason = body.reason.trim();
    if reason.is_empty() || reason.chars().count() > 256 {
        return Err(ApiError::bad_request(
            "A reason of at most 256 characters is required",
        ));
    }
    let audit = RescueAudit {
        admin_id: auth.user_id,
        reason: reason.to_string(),
    };

    let reservation = admin_rescue::reload_server(&state, match_id, &audit).await?;
    let response = admin_reservation_summary(&state, reservation).await;
    Ok(Json(DataResponse::new(response, request_id)))
}

/// Check whether a server config can be built for a match (admin, read-only).
///
/// Lists, per team, the players the config would whitelist — roster plus
/// applied substitutions — and flags anyone without a usable Steam ID, a
/// team too short to ready up, and an unfinished veto.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/server/roster-check",
    params(("match_id" = String, Path, description = "Match ID")),
    responses(
        (status = 200, description = "Roster check", body = DataResponse<RosterCheckResponse>),
        (status = 403, description = "Missing admin.servers.manage", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "match_lifecycle"
)]
pub async fn match_server_roster_check(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(match_id): Path<String>,
) -> ApiResult<Json<DataResponse<RosterCheckResponse>>> {
    perm_checker
        .require_permission(&auth, permissions::admin::SERVERS_MANAGE)
        .await?;
    let request_id = get_request_id(&headers);
    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid match id"))?;

    let check = admin_rescue::roster_check(&state, match_id).await?;
    Ok(Json(DataResponse::new(check, request_id)))
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
