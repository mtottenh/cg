//! Match lineup handlers — provisional declaration + read (Phase B/E).
//!
//! Declaration is a captain/owner/delegate action (same authority model as
//! check-in, P-24) enforced via [`super::require_registration_actor`]. A lineup
//! is opponent-visible only once `locked` (§0 Q3); before that only an actor for
//! the owning registration (or tournament staff) may see the player list.

use super::{get_request_id, require_registration_actor};
use crate::dto::common::DataResponse;
use crate::dto::requests::DeclareLineupRequest;
use crate::dto::responses::{LineupPlayerIdentity, LineupPlayerNames, MatchLineupResponse};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker, ValidatedJson};
use crate::state::TournamentState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use portal_core::types::LineupStatus;
use portal_core::{PlayerId, TournamentMatchId, TournamentRegistrationId};

/// Declare (or replace) the provisional lineup for a registration in a match.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/lineup",
    request_body = DeclareLineupRequest,
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Lineup declared", body = DataResponse<MatchLineupResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Not authorized to declare for this participant", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    operation_id = "declare_match_lineup",
    tag = "match_lifecycle"
)]
pub async fn declare_lineup(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
    ValidatedJson(req): ValidatedJson<DeclareLineupRequest>,
) -> ApiResult<Json<DataResponse<MatchLineupResponse>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;
    let registration_id: TournamentRegistrationId = req
        .registration_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid registration ID format"))?;

    require_registration_actor(&state, &auth, &perm_checker, registration_id).await?;

    let player_ids: Vec<PlayerId> = req
        .player_ids
        .iter()
        .map(|s| s.parse())
        .collect::<Result<_, _>>()
        .map_err(|_| ApiError::bad_request("Invalid player ID format"))?;

    let lineup = state
        .lineup_service
        .declare_lineup(
            match_id,
            registration_id,
            player_ids,
            auth.user_id,
            req.submit,
            req.notes,
        )
        .await?;

    // The declarer always sees their own lineup in full.
    let ids: Vec<PlayerId> = lineup.players.iter().map(|p| p.player_id).collect();
    let names = lineup_player_names(&state, ids).await;
    Ok(Json(DataResponse::new(
        MatchLineupResponse::from_visible(lineup, &names),
        request_id,
    )))
}

/// Display names for the players of one or more lineups, in one lookup.
///
/// A failed lookup degrades to "Unknown player" rows rather than failing the
/// request: the lineup itself is still correct without the names.
async fn lineup_player_names(state: &TournamentState, mut ids: Vec<PlayerId>) -> LineupPlayerNames {
    ids.sort_unstable_by_key(PlayerId::as_uuid);
    ids.dedup();
    if ids.is_empty() {
        return LineupPlayerNames::new();
    }
    match state.player_service.get_players_by_ids(&ids).await {
        Ok(players) => players
            .into_iter()
            .map(|p| {
                (
                    p.id,
                    LineupPlayerIdentity {
                        display_name: p.display_name,
                        avatar_url: p.avatar_url,
                    },
                )
            })
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "could not look up lineup player names");
            LineupPlayerNames::new()
        }
    }
}

/// List the lineups for a match.
///
/// A lineup's player list is shown when it is `locked` (opponent-visible) OR
/// the caller may act for that registration OR is tournament staff. Otherwise
/// the lineup metadata is returned with `players_visible = false` and no players.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/lineups",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID")
    ),
    responses(
        (status = 200, description = "Match lineups", body = DataResponse<Vec<MatchLineupResponse>>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    operation_id = "list_match_lineups",
    tag = "match_lifecycle"
)]
pub async fn get_match_lineups(
    State(state): State<TournamentState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((_tournament_id, match_id)): Path<(String, String)>,
) -> ApiResult<Json<DataResponse<Vec<MatchLineupResponse>>>> {
    let request_id = get_request_id(&headers);

    let match_id: TournamentMatchId = match_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid match ID format"))?;

    let lineups = state.lineup_service.list_for_match(match_id).await?;
    let ids: Vec<PlayerId> = lineups
        .iter()
        .flat_map(|wp| wp.players.iter().map(|p| p.player_id))
        .collect();
    let names = lineup_player_names(&state, ids).await;

    let mut out = Vec::with_capacity(lineups.len());
    for wp in lineups {
        // Locked lineups are public to any authenticated viewer (§0 Q3).
        // An unlocked lineup is only visible to an actor for the registration
        // (or tournament staff, which require_registration_actor also allows).
        let visible = wp.lineup.status == LineupStatus::Locked
            || require_registration_actor(&state, &auth, &perm_checker, wp.lineup.registration_id)
                .await
                .is_ok();
        out.push(MatchLineupResponse::from_with_visibility(
            wp.lineup, wp.players, visible, &names,
        ));
    }

    Ok(Json(DataResponse::new(out, request_id)))
}
