//! Award, standings, leaderboard, stat-catalog, and trophy-case handlers.
//!
//! Reads are public. Mutations are organizer-scoped:
//! - tournament awards → `tournament.settings.manage` (scoped; admin
//!   override applies),
//! - season awards → `league.seasons.manage` on the season's league.
//!
//! Design: `docs/design-tournament-awards.md` §5.

use crate::dto::common::DataResponse;
use crate::dto::requests::{
    CreateAwardRequest, LeaderboardQueryParams, PlayerStatsQueryParams, StandingsQueryParams,
    UpdateAwardRequest,
};
use crate::dto::responses::{
    AwardResponse, AwardStandingsResponse, AwardTemplateResponse, FinalizedAwardResponse,
    LeaderboardEntryResponse, PlayerStatsEntryResponse, PlayerTrophyResponse,
    StatCatalogEntryResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthenticatedUser, PermissionChecker};
use crate::state::AwardsState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use portal_core::types::TournamentStatus;
use portal_core::{AwardId, GameId, LeagueSeasonId, PlayerId, TournamentId, permissions};
use portal_domain::entities::award::{
    Award, AwardScopeType, MinQualifier, MinQualifierType, StatAggregation, StatDirection,
};
use portal_domain::entities::league_team::LeagueSeason;
use portal_domain::repositories::{
    LeaderboardQuery, LeaderboardScope, PlayerStatsQuery, PlayerStatsSort, UpdateAwardPresentation,
};
use portal_domain::services::CreateCustomAwardCommand;
use validator::Validate;

/// Default number of rows for standings/leaderboard reads.
const DEFAULT_LIMIT: i64 = 10;
/// Hard cap on rows for standings/leaderboard reads.
const MAX_LIMIT: i64 = 100;

/// Extract request ID from headers.
fn get_request_id(headers: &HeaderMap) -> &str {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}

/// Clamp an optional row limit into `1..=MAX_LIMIT`.
fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Parse an optional aggregation string (`sum` default).
fn parse_aggregation(raw: Option<&str>) -> Result<StatAggregation, ApiError> {
    raw.map_or_else(
        || Ok(StatAggregation::default()),
        |s| s.parse().map_err(|e: String| ApiError::bad_request(e)),
    )
}

/// Parse an optional direction string (`desc` default).
fn parse_direction(raw: Option<&str>) -> Result<StatDirection, ApiError> {
    raw.map_or_else(
        || Ok(StatDirection::default()),
        |s| s.parse().map_err(|e: String| ApiError::bad_request(e)),
    )
}

/// Resolve a season and its league (RBAC scope + game).
async fn resolve_season(
    state: &AwardsState,
    season_id: LeagueSeasonId,
) -> Result<(LeagueSeason, GameId), ApiError> {
    let season = state.league_season_service.get_season(season_id).await?;
    let league = state.league_service.get_league(season.league_id).await?;
    Ok((season, league.game_id))
}

/// Require `league.seasons.manage` on the season's league (admin override
/// applies). Returns the season's game for downstream validation.
async fn require_season_manage(
    state: &AwardsState,
    perm_checker: &PermissionChecker,
    auth: &AuthenticatedUser,
    season_id: LeagueSeasonId,
) -> Result<GameId, ApiError> {
    let (season, game_id) = resolve_season(state, season_id).await?;
    perm_checker
        .require_league_permission(
            auth,
            season.league_id.as_uuid(),
            permissions::league::SEASONS_MANAGE,
        )
        .await?;
    Ok(game_id)
}

/// Validate a stat key against the game plugin's catalog. Keys under the
/// open `kills.weapon.` set are always accepted (the weapon space is
/// data-driven, not enumerable).
async fn validate_stat_key_for_game(
    state: &AwardsState,
    game_id: GameId,
    stat_key: &str,
) -> Result<(), ApiError> {
    if stat_key.starts_with("kills.weapon.") {
        return Ok(());
    }

    let game = state
        .game_repo
        .find_by_id(game_id.as_uuid())
        .await?
        .ok_or_else(|| ApiError::not_found("Game not found"))?;

    let catalog = state
        .plugin_manager
        .get(&game.plugin_id)
        .and_then(|plugin| {
            plugin
                .as_tournament_plugin()
                .map(portal_plugins::traits::TournamentPlugin::stat_definitions)
        })
        .unwrap_or_default();

    if catalog.iter().any(|d| d.key == stat_key) {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "Unknown stat key '{stat_key}' for this game's stat catalog"
        )))
    }
}

/// Shared creation path for both scopes: template instantiation or custom
/// award with catalog validation.
async fn create_award_in_scope(
    state: &AwardsState,
    scope_type: AwardScopeType,
    scope_id: uuid::Uuid,
    game_id: GameId,
    request: CreateAwardRequest,
    auth: &AuthenticatedUser,
) -> Result<Award, ApiError> {
    if let Some(template_key) = &request.template_key {
        return state
            .award_service
            .create_from_template(
                scope_type,
                scope_id,
                game_id,
                template_key,
                request.name,
                auth.user_id,
            )
            .await
            .map_err(Into::into);
    }

    let name = request
        .name
        .ok_or_else(|| ApiError::bad_request("name is required for custom awards"))?;
    let stat_key = request
        .stat_key
        .ok_or_else(|| ApiError::bad_request("stat_key is required for custom awards"))?;
    validate_stat_key_for_game(state, game_id, &stat_key).await?;

    let aggregation = parse_aggregation(request.aggregation.as_deref())?;
    let direction = parse_direction(request.direction.as_deref())?;
    let min_qualifier = match (
        request.min_qualifier_type.as_deref(),
        request.min_qualifier_value,
    ) {
        (Some(qualifier_type), Some(value)) => Some(MinQualifier {
            qualifier_type: qualifier_type
                .parse::<MinQualifierType>()
                .map_err(ApiError::bad_request)?,
            value,
        }),
        (None, None) => None,
        _ => {
            return Err(ApiError::bad_request(
                "min_qualifier_type and min_qualifier_value must be provided together",
            ));
        }
    };

    state
        .award_service
        .create_custom(
            scope_type,
            scope_id,
            game_id,
            CreateCustomAwardCommand {
                name,
                description: request.description,
                icon: request.icon,
                color: request.color,
                stat_key,
                aggregation,
                direction,
                min_qualifier,
            },
            auth.user_id,
        )
        .await
        .map_err(Into::into)
}

/// Build the presentation update from a PATCH body.
fn presentation_update(request: UpdateAwardRequest) -> UpdateAwardPresentation {
    UpdateAwardPresentation {
        name: request.name,
        description: request.description,
        icon: request.icon,
        color: request.color,
    }
}

// =============================================================================
// TOURNAMENT-SCOPED AWARDS
// =============================================================================

/// List a tournament's awards.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/awards",
    params(("tournament_id" = String, Path, description = "Tournament ID")),
    responses(
        (status = 200, description = "Awards in the tournament", body = DataResponse<Vec<AwardResponse>>),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn list_tournament_awards(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
) -> ApiResult<Json<DataResponse<Vec<AwardResponse>>>> {
    let request_id = get_request_id(&headers);

    // 404 for unknown tournaments rather than an empty list.
    state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;

    let awards = state
        .award_service
        .list_awards(AwardScopeType::Tournament, tournament_id.as_uuid())
        .await?;

    Ok(Json(DataResponse::new(
        awards.into_iter().map(AwardResponse::from).collect(),
        request_id,
    )))
}

/// Create an award in a tournament (from template or custom).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/awards",
    params(("tournament_id" = String, Path, description = "Tournament ID")),
    request_body = CreateAwardRequest,
    responses(
        (status = 201, description = "Award created", body = DataResponse<AwardResponse>),
        (status = 400, description = "Validation error or unknown stat key", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing tournament.settings.manage", body = ApiError),
        (status = 404, description = "Tournament or template not found", body = ApiError),
        (status = 409, description = "Duplicate award name in scope", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn create_tournament_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    Json(request): Json<CreateAwardRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<AwardResponse>>)> {
    request.validate()?;
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            permissions::tournament::SETTINGS_MANAGE,
        )
        .await?;

    let tournament = state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;

    let award = create_award_in_scope(
        &state,
        AwardScopeType::Tournament,
        tournament_id.as_uuid(),
        tournament.game_id,
        request,
        &auth,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(AwardResponse::from(award), request_id)),
    ))
}

/// Update a tournament award's presentation (active awards only).
#[utoipa::path(
    patch,
    path = "/v1/tournaments/{tournament_id}/awards/{award_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("award_id" = String, Path, description = "Award ID"),
    ),
    request_body = UpdateAwardRequest,
    responses(
        (status = 200, description = "Award updated", body = DataResponse<AwardResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing tournament.settings.manage", body = ApiError),
        (status = 404, description = "Award not found in this tournament", body = ApiError),
        (status = 409, description = "Award is finalized/void, or name collides", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn update_tournament_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((tournament_id, award_id)): Path<(TournamentId, AwardId)>,
    Json(request): Json<UpdateAwardRequest>,
) -> ApiResult<Json<DataResponse<AwardResponse>>> {
    request.validate()?;
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            permissions::tournament::SETTINGS_MANAGE,
        )
        .await?;

    let award = state
        .award_service
        .update_award(
            award_id,
            AwardScopeType::Tournament,
            tournament_id.as_uuid(),
            presentation_update(request),
        )
        .await?;

    Ok(Json(DataResponse::new(
        AwardResponse::from(award),
        request_id,
    )))
}

/// Void a tournament award (soft delete; finalized awards are permanent).
#[utoipa::path(
    delete,
    path = "/v1/tournaments/{tournament_id}/awards/{award_id}",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("award_id" = String, Path, description = "Award ID"),
    ),
    responses(
        (status = 200, description = "Award voided", body = DataResponse<AwardResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing tournament.settings.manage", body = ApiError),
        (status = 404, description = "Award not found in this tournament", body = ApiError),
        (status = 409, description = "Finalized awards cannot be voided", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn void_tournament_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((tournament_id, award_id)): Path<(TournamentId, AwardId)>,
) -> ApiResult<Json<DataResponse<AwardResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            permissions::tournament::SETTINGS_MANAGE,
        )
        .await?;

    let award = state
        .award_service
        .void_award(
            award_id,
            AwardScopeType::Tournament,
            tournament_id.as_uuid(),
        )
        .await?;

    Ok(Json(DataResponse::new(
        AwardResponse::from(award),
        request_id,
    )))
}

/// Live standings for a tournament award.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/awards/{award_id}/standings",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("award_id" = String, Path, description = "Award ID"),
        StandingsQueryParams,
    ),
    responses(
        (status = 200, description = "Current award standings", body = DataResponse<AwardStandingsResponse>),
        (status = 404, description = "Award not found in this tournament", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_tournament_award_standings(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path((tournament_id, award_id)): Path<(TournamentId, AwardId)>,
    Query(query): Query<StandingsQueryParams>,
) -> ApiResult<Json<DataResponse<AwardStandingsResponse>>> {
    let request_id = get_request_id(&headers);

    let award = state
        .award_service
        .get_award_in_scope(
            award_id,
            AwardScopeType::Tournament,
            tournament_id.as_uuid(),
        )
        .await?;
    let entries = state
        .award_service
        .standings(&award, clamp_limit(query.limit))
        .await?;

    Ok(Json(DataResponse::new(
        AwardStandingsResponse {
            award: AwardResponse::from(award),
            entries: LeaderboardEntryResponse::from_entries(entries),
        },
        request_id,
    )))
}

/// Finalize a tournament award (manual trigger; completion auto-finalizes).
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/awards/{award_id}/finalize",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        ("award_id" = String, Path, description = "Award ID"),
    ),
    responses(
        (status = 200, description = "Award finalized (podium snapshotted)", body = DataResponse<FinalizedAwardResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing tournament.settings.manage", body = ApiError),
        (status = 404, description = "Award not found in this tournament", body = ApiError),
        (status = 409, description = "Award voided or results locked", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn finalize_tournament_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((tournament_id, award_id)): Path<(TournamentId, AwardId)>,
) -> ApiResult<Json<DataResponse<FinalizedAwardResponse>>> {
    let request_id = get_request_id(&headers);

    perm_checker
        .require_tournament_permission(
            &auth,
            tournament_id.as_uuid(),
            permissions::tournament::SETTINGS_MANAGE,
        )
        .await?;

    // Once the tournament itself is finalized, award history is locked.
    let tournament = state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;
    let scope_locked = tournament.status == TournamentStatus::Finalized;

    let (award, results) = state
        .award_service
        .finalize(
            award_id,
            AwardScopeType::Tournament,
            tournament_id.as_uuid(),
            scope_locked,
        )
        .await?;

    Ok(Json(DataResponse::new(
        FinalizedAwardResponse {
            award: AwardResponse::from(award),
            results: results.into_iter().map(Into::into).collect(),
        },
        request_id,
    )))
}

// =============================================================================
// SEASON-SCOPED AWARDS
// =============================================================================

/// List a league season's awards.
#[utoipa::path(
    get,
    path = "/v1/league-seasons/{season_id}/awards",
    params(("season_id" = String, Path, description = "League season ID")),
    responses(
        (status = 200, description = "Awards in the season", body = DataResponse<Vec<AwardResponse>>),
        (status = 404, description = "Season not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn list_season_awards(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
) -> ApiResult<Json<DataResponse<Vec<AwardResponse>>>> {
    let request_id = get_request_id(&headers);

    state.league_season_service.get_season(season_id).await?;

    let awards = state
        .award_service
        .list_awards(AwardScopeType::LeagueSeason, season_id.as_uuid())
        .await?;

    Ok(Json(DataResponse::new(
        awards.into_iter().map(AwardResponse::from).collect(),
        request_id,
    )))
}

/// Create an award in a league season (from template or custom).
#[utoipa::path(
    post,
    path = "/v1/league-seasons/{season_id}/awards",
    params(("season_id" = String, Path, description = "League season ID")),
    request_body = CreateAwardRequest,
    responses(
        (status = 201, description = "Award created", body = DataResponse<AwardResponse>),
        (status = 400, description = "Validation error or unknown stat key", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing league.seasons.manage", body = ApiError),
        (status = 404, description = "Season or template not found", body = ApiError),
        (status = 409, description = "Duplicate award name in scope", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn create_season_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
    Json(request): Json<CreateAwardRequest>,
) -> ApiResult<(StatusCode, Json<DataResponse<AwardResponse>>)> {
    request.validate()?;
    let request_id = get_request_id(&headers);

    let game_id = require_season_manage(&state, &perm_checker, &auth, season_id).await?;

    let award = create_award_in_scope(
        &state,
        AwardScopeType::LeagueSeason,
        season_id.as_uuid(),
        game_id,
        request,
        &auth,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(AwardResponse::from(award), request_id)),
    ))
}

/// Update a season award's presentation (active awards only).
#[utoipa::path(
    patch,
    path = "/v1/league-seasons/{season_id}/awards/{award_id}",
    params(
        ("season_id" = String, Path, description = "League season ID"),
        ("award_id" = String, Path, description = "Award ID"),
    ),
    request_body = UpdateAwardRequest,
    responses(
        (status = 200, description = "Award updated", body = DataResponse<AwardResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing league.seasons.manage", body = ApiError),
        (status = 404, description = "Award not found in this season", body = ApiError),
        (status = 409, description = "Award is finalized/void, or name collides", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn update_season_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((season_id, award_id)): Path<(LeagueSeasonId, AwardId)>,
    Json(request): Json<UpdateAwardRequest>,
) -> ApiResult<Json<DataResponse<AwardResponse>>> {
    request.validate()?;
    let request_id = get_request_id(&headers);

    require_season_manage(&state, &perm_checker, &auth, season_id).await?;

    let award = state
        .award_service
        .update_award(
            award_id,
            AwardScopeType::LeagueSeason,
            season_id.as_uuid(),
            presentation_update(request),
        )
        .await?;

    Ok(Json(DataResponse::new(
        AwardResponse::from(award),
        request_id,
    )))
}

/// Void a season award (soft delete; finalized awards are permanent).
#[utoipa::path(
    delete,
    path = "/v1/league-seasons/{season_id}/awards/{award_id}",
    params(
        ("season_id" = String, Path, description = "League season ID"),
        ("award_id" = String, Path, description = "Award ID"),
    ),
    responses(
        (status = 200, description = "Award voided", body = DataResponse<AwardResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing league.seasons.manage", body = ApiError),
        (status = 404, description = "Award not found in this season", body = ApiError),
        (status = 409, description = "Finalized awards cannot be voided", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn void_season_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((season_id, award_id)): Path<(LeagueSeasonId, AwardId)>,
) -> ApiResult<Json<DataResponse<AwardResponse>>> {
    let request_id = get_request_id(&headers);

    require_season_manage(&state, &perm_checker, &auth, season_id).await?;

    let award = state
        .award_service
        .void_award(award_id, AwardScopeType::LeagueSeason, season_id.as_uuid())
        .await?;

    Ok(Json(DataResponse::new(
        AwardResponse::from(award),
        request_id,
    )))
}

/// Live standings for a season award.
#[utoipa::path(
    get,
    path = "/v1/league-seasons/{season_id}/awards/{award_id}/standings",
    params(
        ("season_id" = String, Path, description = "League season ID"),
        ("award_id" = String, Path, description = "Award ID"),
        StandingsQueryParams,
    ),
    responses(
        (status = 200, description = "Current award standings", body = DataResponse<AwardStandingsResponse>),
        (status = 404, description = "Award not found in this season", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_season_award_standings(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path((season_id, award_id)): Path<(LeagueSeasonId, AwardId)>,
    Query(query): Query<StandingsQueryParams>,
) -> ApiResult<Json<DataResponse<AwardStandingsResponse>>> {
    let request_id = get_request_id(&headers);

    let award = state
        .award_service
        .get_award_in_scope(award_id, AwardScopeType::LeagueSeason, season_id.as_uuid())
        .await?;
    let entries = state
        .award_service
        .standings(&award, clamp_limit(query.limit))
        .await?;

    Ok(Json(DataResponse::new(
        AwardStandingsResponse {
            award: AwardResponse::from(award),
            entries: LeaderboardEntryResponse::from_entries(entries),
        },
        request_id,
    )))
}

/// Finalize a season award (manual trigger).
#[utoipa::path(
    post,
    path = "/v1/league-seasons/{season_id}/awards/{award_id}/finalize",
    params(
        ("season_id" = String, Path, description = "League season ID"),
        ("award_id" = String, Path, description = "Award ID"),
    ),
    responses(
        (status = 200, description = "Award finalized (podium snapshotted)", body = DataResponse<FinalizedAwardResponse>),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 403, description = "Missing league.seasons.manage", body = ApiError),
        (status = 404, description = "Award not found in this season", body = ApiError),
        (status = 409, description = "Award voided", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "awards"
)]
pub async fn finalize_season_award(
    State(state): State<AwardsState>,
    auth: AuthenticatedUser,
    perm_checker: PermissionChecker,
    headers: HeaderMap,
    Path((season_id, award_id)): Path<(LeagueSeasonId, AwardId)>,
) -> ApiResult<Json<DataResponse<FinalizedAwardResponse>>> {
    let request_id = get_request_id(&headers);

    require_season_manage(&state, &perm_checker, &auth, season_id).await?;

    // Seasons have no post-completion lock state, so re-finalization
    // (recompute) stays available to season managers.
    let (award, results) = state
        .award_service
        .finalize(
            award_id,
            AwardScopeType::LeagueSeason,
            season_id.as_uuid(),
            false,
        )
        .await?;

    Ok(Json(DataResponse::new(
        FinalizedAwardResponse {
            award: AwardResponse::from(award),
            results: results.into_iter().map(Into::into).collect(),
        },
        request_id,
    )))
}

// =============================================================================
// LEADERBOARDS
// =============================================================================

/// Build and run a leaderboard query for a scope.
async fn run_leaderboard(
    state: &AwardsState,
    scope: LeaderboardScope,
    query: LeaderboardQueryParams,
) -> Result<Vec<LeaderboardEntryResponse>, ApiError> {
    let min_qualifier = match (query.min_matches, query.min_rounds) {
        (Some(_), Some(_)) => {
            return Err(ApiError::bad_request(
                "min_matches and min_rounds are mutually exclusive",
            ));
        }
        (Some(value), None) => Some(MinQualifier {
            qualifier_type: MinQualifierType::Matches,
            value,
        }),
        (None, Some(value)) => Some(MinQualifier {
            qualifier_type: MinQualifierType::Rounds,
            value,
        }),
        (None, None) => None,
    };
    if let Some(q) = &min_qualifier
        && q.value < 1
    {
        return Err(ApiError::bad_request("Qualifier thresholds must be >= 1"));
    }

    let entries = state
        .award_service
        .leaderboard(&LeaderboardQuery {
            scope,
            stat_key: query.stat_key,
            aggregation: parse_aggregation(query.aggregation.as_deref())?,
            direction: parse_direction(query.direction.as_deref())?,
            min_qualifier,
            limit: clamp_limit(query.limit),
        })
        .await?;

    Ok(LeaderboardEntryResponse::from_entries(entries))
}

/// Plain stat leaderboard over a tournament's linked demos.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/leaderboards",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        LeaderboardQueryParams,
    ),
    responses(
        (status = 200, description = "Ranked leaderboard rows", body = DataResponse<Vec<LeaderboardEntryResponse>>),
        (status = 400, description = "Invalid query parameters", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_tournament_leaderboard(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    Query(query): Query<LeaderboardQueryParams>,
) -> ApiResult<Json<DataResponse<Vec<LeaderboardEntryResponse>>>> {
    let request_id = get_request_id(&headers);

    state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;

    let entries =
        run_leaderboard(&state, LeaderboardScope::Tournament(tournament_id), query).await?;

    Ok(Json(DataResponse::new(entries, request_id)))
}

/// Plain stat leaderboard over a league season's linked demos.
#[utoipa::path(
    get,
    path = "/v1/league-seasons/{season_id}/leaderboards",
    params(
        ("season_id" = String, Path, description = "League season ID"),
        LeaderboardQueryParams,
    ),
    responses(
        (status = 200, description = "Ranked leaderboard rows", body = DataResponse<Vec<LeaderboardEntryResponse>>),
        (status = 400, description = "Invalid query parameters", body = ApiError),
        (status = 404, description = "Season not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_season_leaderboard(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
    Query(query): Query<LeaderboardQueryParams>,
) -> ApiResult<Json<DataResponse<Vec<LeaderboardEntryResponse>>>> {
    let request_id = get_request_id(&headers);

    state.league_season_service.get_season(season_id).await?;

    let entries = run_leaderboard(&state, LeaderboardScope::Season(season_id), query).await?;

    Ok(Json(DataResponse::new(entries, request_id)))
}

// =============================================================================
// COMBINED PLAYER-STATS LEADERBOARD
// =============================================================================

/// Default rows for the combined player-stats leaderboard.
const PLAYER_STATS_DEFAULT_LIMIT: i64 = 100;
/// Hard cap on rows for the combined player-stats leaderboard.
const PLAYER_STATS_MAX_LIMIT: i64 = 200;

/// Map an optional sort string to a `PlayerStatsSort` (`kills` default).
fn parse_player_stats_sort(raw: Option<&str>) -> Result<PlayerStatsSort, ApiError> {
    match raw {
        None | Some("kills") => Ok(PlayerStatsSort::Kills),
        Some("deaths") => Ok(PlayerStatsSort::Deaths),
        Some("assists") => Ok(PlayerStatsSort::Assists),
        Some("total_damage") => Ok(PlayerStatsSort::TotalDamage),
        Some("adr") => Ok(PlayerStatsSort::Adr),
        Some(other) => Err(ApiError::bad_request(format!(
            "Invalid sort '{other}'; expected one of kills, deaths, assists, total_damage, adr"
        ))),
    }
}

/// Build and run a combined player-stats leaderboard query for a scope.
async fn run_player_stats(
    state: &AwardsState,
    scope: LeaderboardScope,
    query: PlayerStatsQueryParams,
) -> Result<Vec<PlayerStatsEntryResponse>, ApiError> {
    let sort = parse_player_stats_sort(query.sort.as_deref())?;
    let min_demos = i64::from(query.min_demos.unwrap_or(1).max(1));
    let min_rounds = f64::from(query.min_rounds.unwrap_or(0).max(0));
    let limit = query
        .limit
        .unwrap_or(PLAYER_STATS_DEFAULT_LIMIT)
        .clamp(1, PLAYER_STATS_MAX_LIMIT);

    let entries = state
        .award_service
        .player_stats_leaderboard(&PlayerStatsQuery {
            scope,
            sort,
            min_demos,
            min_rounds,
            limit,
        })
        .await?;

    Ok(entries.into_iter().map(Into::into).collect())
}

/// Combined per-player stat leaderboard over a tournament's linked demos.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/stats-leaderboard",
    params(
        ("tournament_id" = String, Path, description = "Tournament ID"),
        PlayerStatsQueryParams,
    ),
    responses(
        (status = 200, description = "Combined player-stats rows", body = DataResponse<Vec<PlayerStatsEntryResponse>>),
        (status = 400, description = "Invalid query parameters", body = ApiError),
        (status = 404, description = "Tournament not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_tournament_player_stats(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(tournament_id): Path<TournamentId>,
    Query(query): Query<PlayerStatsQueryParams>,
) -> ApiResult<Json<DataResponse<Vec<PlayerStatsEntryResponse>>>> {
    let request_id = get_request_id(&headers);

    state
        .tournament_service
        .get_tournament(tournament_id)
        .await?;

    let entries =
        run_player_stats(&state, LeaderboardScope::Tournament(tournament_id), query).await?;

    Ok(Json(DataResponse::new(entries, request_id)))
}

/// Combined per-player stat leaderboard over a league season's linked demos.
#[utoipa::path(
    get,
    path = "/v1/league-seasons/{season_id}/stats-leaderboard",
    params(
        ("season_id" = String, Path, description = "League season ID"),
        PlayerStatsQueryParams,
    ),
    responses(
        (status = 200, description = "Combined player-stats rows", body = DataResponse<Vec<PlayerStatsEntryResponse>>),
        (status = 400, description = "Invalid query parameters", body = ApiError),
        (status = 404, description = "Season not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_season_player_stats(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(season_id): Path<LeagueSeasonId>,
    Query(query): Query<PlayerStatsQueryParams>,
) -> ApiResult<Json<DataResponse<Vec<PlayerStatsEntryResponse>>>> {
    let request_id = get_request_id(&headers);

    state.league_season_service.get_season(season_id).await?;

    let entries = run_player_stats(&state, LeaderboardScope::Season(season_id), query).await?;

    Ok(Json(DataResponse::new(entries, request_id)))
}

// =============================================================================
// GAME CATALOGS
// =============================================================================

/// Resolve a game by UUID or slug.
async fn resolve_game(
    state: &AwardsState,
    game_id_or_slug: &str,
) -> Result<portal_db::entities::GameRow, ApiError> {
    let game = if let Ok(uuid) = game_id_or_slug.parse::<uuid::Uuid>() {
        state.game_repo.find_by_id(uuid).await?
    } else {
        state.game_repo.find_by_slug(game_id_or_slug).await?
    };
    game.ok_or_else(|| ApiError::not_found(format!("Game not found: {game_id_or_slug}")))
}

/// A game's stat catalog (for award-builder UIs).
#[utoipa::path(
    get,
    path = "/v1/games/{game_id}/stat-catalog",
    params(("game_id" = String, Path, description = "Game ID or slug (e.g. cs2)")),
    responses(
        (status = 200, description = "Stat definitions the game plugin can extract", body = DataResponse<Vec<StatCatalogEntryResponse>>),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_stat_catalog(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(game_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<StatCatalogEntryResponse>>>> {
    let request_id = get_request_id(&headers);

    let game = resolve_game(&state, &game_id).await?;
    let catalog: Vec<StatCatalogEntryResponse> = state
        .plugin_manager
        .get(&game.plugin_id)
        .and_then(|plugin| {
            plugin
                .as_tournament_plugin()
                .map(portal_plugins::traits::TournamentPlugin::stat_definitions)
        })
        .unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(DataResponse::new(catalog, request_id)))
}

/// A game's award templates (for the organizer's picker).
#[utoipa::path(
    get,
    path = "/v1/games/{game_id}/award-templates",
    params(("game_id" = String, Path, description = "Game ID or slug (e.g. cs2)")),
    responses(
        (status = 200, description = "Award templates for the game", body = DataResponse<Vec<AwardTemplateResponse>>),
        (status = 404, description = "Game not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn list_award_templates(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(game_id): Path<String>,
) -> ApiResult<Json<DataResponse<Vec<AwardTemplateResponse>>>> {
    let request_id = get_request_id(&headers);

    let game = resolve_game(&state, &game_id).await?;
    let templates = state
        .award_service
        .list_templates(GameId::from(game.id))
        .await?;

    Ok(Json(DataResponse::new(
        templates.into_iter().map(Into::into).collect(),
        request_id,
    )))
}

// =============================================================================
// TROPHY CASE
// =============================================================================

/// A player's trophy case: finalized award results with scope context.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/awards",
    params(("player_id" = String, Path, description = "Player ID")),
    responses(
        (status = 200, description = "Finalized awards won by the player", body = DataResponse<Vec<PlayerTrophyResponse>>),
        (status = 404, description = "Player not found", body = ApiError),
    ),
    tag = "awards"
)]
pub async fn get_player_awards(
    State(state): State<AwardsState>,
    headers: HeaderMap,
    Path(player_id): Path<PlayerId>,
) -> ApiResult<Json<DataResponse<Vec<PlayerTrophyResponse>>>> {
    let request_id = get_request_id(&headers);

    // 404 for unknown players rather than an empty trophy case.
    state.player_service.get_player(player_id).await?;

    let trophies = state.award_service.player_trophies(player_id).await?;

    Ok(Json(DataResponse::new(
        trophies.into_iter().map(Into::into).collect(),
        request_id,
    )))
}
