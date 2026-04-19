//! Tournament seeding handlers.
//!
//! Extracted from `tournaments/mod.rs` as part of the N1 split. Owns
//! the four seeding endpoints (get / auto / manual / clear). Seeding
//! is a read-modify-write over tournament registrations; the actual
//! algorithm implementations live in the seeding service.

use super::get_request_id;
use crate::dto::common::DataResponse;
use crate::dto::requests::{AutoSeedRequest, ManualSeedRequest};
use crate::dto::responses::SeededParticipantResponse;
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::TournamentState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use portal_core::TournamentId;

/// Get current seeding for a tournament.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/seeding",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 200, description = "Current seeding", body = DataResponse<Vec<SeededParticipantResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "tournaments"
)]
pub async fn get_seeding(
    State(state): State<TournamentState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<SeededParticipantResponse>>>> {
    let request_id = get_request_id(&headers);

    let seeded = state
        .seeding_service
        .get_current_seeding(tournament_id)
        .await?;

    let data: Vec<SeededParticipantResponse> = seeded
        .into_iter()
        .map(|p| SeededParticipantResponse {
            registration_id: p.registration_id.to_string(),
            participant_name: p.participant_name,
            seed: p.seed,
            seed_rating: p.seed_rating,
        })
        .collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Auto-seed participants using the specified algorithm.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/seeding/auto",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = AutoSeedRequest,
    responses(
        (status = 200, description = "Seeding complete", body = DataResponse<Vec<SeededParticipantResponse>>),
        (status = 400, description = "Invalid algorithm or tournament state", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn auto_seed(
    State(state): State<TournamentState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    Json(req): Json<AutoSeedRequest>,
) -> ApiResult<Json<DataResponse<Vec<SeededParticipantResponse>>>> {
    let request_id = get_request_id(&headers);

    let algorithm: portal_core::types::SeedingAlgorithm = req
        .algorithm
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid seeding algorithm"))?;

    let seeded = state
        .seeding_service
        .auto_seed(tournament_id, algorithm)
        .await?;

    let data: Vec<SeededParticipantResponse> = seeded
        .into_iter()
        .map(|p| SeededParticipantResponse {
            registration_id: p.registration_id.to_string(),
            participant_name: p.participant_name,
            seed: p.seed,
            seed_rating: p.seed_rating,
        })
        .collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Manually set seeds for participants.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/seeding/manual",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    request_body = ManualSeedRequest,
    responses(
        (status = 200, description = "Seeding complete", body = DataResponse<Vec<SeededParticipantResponse>>),
        (status = 400, description = "Invalid seeds", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament or registration not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn manual_seed(
    State(state): State<TournamentState>,
    _auth: AuthenticatedUser,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    ValidatedJson(req): ValidatedJson<ManualSeedRequest>,
) -> ApiResult<Json<DataResponse<Vec<SeededParticipantResponse>>>> {
    let request_id = get_request_id(&headers);

    // Parse registration IDs
    let seeds: Vec<(portal_core::TournamentRegistrationId, i32)> = req
        .seeds
        .into_iter()
        .map(|s| {
            s.registration_id
                .parse()
                .map(|id| (id, s.seed))
                .map_err(|_| ApiError::bad_request("Invalid registration ID format"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let seeded = state
        .seeding_service
        .manual_seed(tournament_id, seeds)
        .await?;

    let data: Vec<SeededParticipantResponse> = seeded
        .into_iter()
        .map(|p| SeededParticipantResponse {
            registration_id: p.registration_id.to_string(),
            participant_name: p.participant_name,
            seed: p.seed,
            seed_rating: p.seed_rating,
        })
        .collect();

    Ok(Json(DataResponse::new(data, request_id)))
}

/// Clear all seeds for a tournament.
#[utoipa::path(
    delete,
    path = "/v1/tournaments/{tournament_id}/seeding",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID")
    ),
    responses(
        (status = 204, description = "Seeds cleared"),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "tournaments"
)]
pub async fn clear_seeding(
    State(state): State<TournamentState>,
    _auth: AuthenticatedUser,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<StatusCode> {
    state
        .seeding_service
        .clear_seeding(tournament_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
