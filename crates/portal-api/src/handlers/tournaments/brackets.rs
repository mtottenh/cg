//! Tournament bracket and match read handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Just
//! the three read endpoints here — bracket mutation lives in the
//! progression saga, and match mutation is in
//! [`super::match_lifecycle`] / [`super::scheduling`].

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::responses::{TournamentBracketResponse, TournamentMatchResponse};
use crate::error::{ApiError, ApiResult};
use crate::state::TournamentState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use portal_core::{TournamentId, TournamentMatchId};

/// Get brackets for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/brackets",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "List of brackets", body = DataResponse<Vec<TournamentBracketResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_brackets(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentBracketResponse>>>> {
    let request_id = get_request_id(&headers);

    let brackets = state.tournament_service.get_bracket(tournament_id).await?;

    let data: Vec<TournamentBracketResponse> = brackets.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Get matches for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "List of matches", body = DataResponse<Vec<TournamentMatchResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_matches(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<TournamentMatchResponse>>>> {
    let request_id = get_request_id(&headers);

    let matches = state
        .tournament_service
        .get_tournament_matches(tournament_id)
        .await?;

    let data: Vec<TournamentMatchResponse> = matches.into_iter().map(Into::into).collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Get a single match by ID.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("match_id" = String, Path, description = "Match ID"),
    ),
    responses(
        (status = 200, description = "Match details", body = DataResponse<TournamentMatchResponse>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_match(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path((tournament_id, match_id)): Path<(TournamentId, TournamentMatchId)>,
) -> ApiResult<Json<DataResponse<TournamentMatchResponse>>> {
    let request_id = get_request_id(&headers);

    let match_ = state
        .tournament_service
        .get_tournament_match(tournament_id, match_id)
        .await?;

    Ok(Json(DataResponse::new(match_.into(), request_id)))
}
