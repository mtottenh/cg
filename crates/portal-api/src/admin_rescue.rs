//! Admin match rescue: the operations an organiser needs to get a stuck
//! match moving again without touching the database.
//!
//! A match can end up somewhere the state machine has no way out of — a
//! no-show forfeit fired while both teams were in voice, a check-in window
//! expired, a veto has to be redone, the game server is holding a config
//! built from a roster that has since changed. Each function here is one
//! explicit, audited way back, orchestrating the services and repositories
//! that already own the pieces (progression revert, veto, reservations).
//!
//! The handlers live in `handlers::tournaments::admin_rescue` and
//! `handlers::game_servers::match_server`; this module carries the logic so
//! both can share it, the way `game_server_flow` does for the server path.

use crate::background::ensure_veto_session;
use crate::error::ApiError;
use crate::game_server_flow::{self, GameServerSettings};
use crate::handlers::veto::resolve_match_veto_format;
use crate::state::AppState;
use crate::websocket::agent_manager::AgentCommand;
use chrono::{DateTime, Duration, Utc};
use portal_core::types::{BracketType, ReservationStatus, TournamentMatchStatus};
use portal_core::{DomainError, TournamentMatchId, TournamentRegistrationId};
use portal_db::adapters::PgMatchRescueRepository;
use portal_domain::entities::ServerReservation;
use portal_domain::entities::tournament::TournamentMatch;
use portal_domain::entities::veto::VetoStatus;
use portal_domain::repositories::match_rescue::{
    MatchRescueRepository, MatchRescueSnapshot, RescueAudit,
};
use portal_domain::repositories::tournament::{
    TournamentBracketRepository as _, TournamentMapPoolRepository as _,
    TournamentMatchRepository as _,
};
use portal_domain::repositories::{MatchSubstitutionRepository as _, ServerReservationRepository};
use serde::Serialize;
use utoipa::ToSchema;

/// A live reservation untouched for this long is reported as stuck.
const RESERVATION_STUCK_AFTER_MINUTES: i64 = 5;
/// A started veto untouched for this long past its deadline is reported as
/// stalled (the timeout pass normally acts within a minute).
const VETO_STALLED_AFTER_MINUTES: i64 = 3;

fn rescue_repo(state: &AppState) -> PgMatchRescueRepository {
    PgMatchRescueRepository::new(state.db_pool.clone())
}

async fn load_match(
    state: &AppState,
    match_id: TournamentMatchId,
) -> Result<TournamentMatch, ApiError> {
    state
        .tournament_match_repo
        .find_by_id(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Match not found"))
}

/// Whether the bracket derives its table from match rows (round robin,
/// Swiss) rather than moving participants along links.
async fn is_standings_derived(state: &AppState, match_: &TournamentMatch) -> bool {
    matches!(
        state
            .tournament_bracket_repo
            .find_by_id(match_.bracket_id)
            .await
            .ok()
            .flatten()
            .map(|b| b.bracket_type),
        Some(BracketType::RoundRobin | BracketType::Swiss)
    )
}

/// Release the match's live server reservation, if it has one. Best-effort:
/// a rescue must not fail because the game server is unreachable.
async fn release_live_reservation(state: &AppState, match_id: TournamentMatchId, reason: &str) {
    match game_server_flow::cancel_assignment(state, match_id, reason).await {
        Ok(()) | Err(DomainError::InvalidState(_)) => {}
        Err(e) => {
            tracing::warn!(%match_id, error = %e, "rescue: could not release the server reservation");
        }
    }
}

// =============================================================================
// Reset to ready
// =============================================================================

/// Put a match back to `ready` so it can be scheduled and played afresh.
pub async fn reset_match(
    state: &AppState,
    match_id: TournamentMatchId,
    audit: &RescueAudit,
) -> Result<TournamentMatch, ApiError> {
    use TournamentMatchStatus as S;

    let match_ = load_match(state, match_id).await?;
    let original_status = match_.status;

    match original_status {
        S::Pending => {
            return Err(ApiError::bad_request(
                "This match is still waiting for its participants; there is nothing to reset",
            ));
        }
        S::Ready => {
            return Err(ApiError::bad_request(
                "This match is already ready to be scheduled",
            ));
        }
        S::Completed => {
            return Err(ApiError::bad_request(
                "A completed match cannot be reset. Correct its score, revert its progression, \
                 or rule on a dispute instead",
            ));
        }
        S::Scheduled
        | S::CheckingIn
        | S::PickBan
        | S::InProgress
        | S::AwaitingResult
        | S::Forfeit
        | S::Cancelled
        | S::Disputed => {}
    }
    if !match_.has_both_participants() {
        // A cancelled bye, or a slot emptied by a progression revert.
        return Err(ApiError::bad_request(
            "This match does not have both participants, so it cannot be replayed",
        ));
    }

    let standings_derived = is_standings_derived(state, &match_).await;
    let has_result = match_.winner_registration_id.is_some()
        || match_.loser_registration_id.is_some()
        || original_status == S::Forfeit;

    if standings_derived || has_result {
        // Takes the result out of the table (round robin / Swiss) or the
        // winner back out of the next round (elimination), and re-opens a
        // stage this match had closed. It refuses, before changing anything,
        // when the next round has already been played.
        state
            .progression_service
            .revert_progression(match_id)
            .await
            .map_err(|e| match e {
                DomainError::InvalidState(msg) | DomainError::Conflict(msg) => {
                    ApiError::bad_request(format!(
                        "Cannot reset this match: {msg}. Revert the later match first"
                    ))
                }
                other => ApiError::from(other),
            })?;
    } else if original_status == S::Cancelled
        && let Some(next_id) = match_.winner_progresses_to
        && let Some(next) = state.tournament_match_repo.find_by_id(next_id).await?
        && !matches!(next.status, S::Pending | S::Ready)
    {
        return Err(ApiError::bad_request(format!(
            "Cannot reset this match: the match it feeds is already {}. Reset that match first",
            next.status
        )));
    }

    release_live_reservation(state, match_id, "match reset by an admin").await;

    let summary = rescue_repo(state)
        .reset_to_ready(
            match_id,
            &[
                S::Scheduled,
                S::CheckingIn,
                S::PickBan,
                S::InProgress,
                S::AwaitingResult,
                S::Forfeit,
                S::Cancelled,
                S::Disputed,
            ],
            original_status,
            audit,
        )
        .await?;

    tracing::info!(
        %match_id,
        admin = %audit.admin_id,
        from = %original_status,
        superseded_claims = summary.superseded_claims,
        cancelled_disputes = summary.cancelled_disputes,
        cancelled_proposals = summary.cancelled_proposals,
        veto_discarded = summary.veto_discarded,
        reason = %audit.reason,
        "admin reset match to ready"
    );

    Ok(summary.match_)
}

// =============================================================================
// Check-in window
// =============================================================================

/// Open the check-in window now, or move its deadline.
pub async fn set_check_in_window(
    state: &AppState,
    match_id: TournamentMatchId,
    deadline: DateTime<Utc>,
    audit: &RescueAudit,
) -> Result<TournamentMatch, ApiError> {
    use TournamentMatchStatus as S;

    if deadline <= Utc::now() {
        return Err(ApiError::bad_request(
            "The check-in deadline must be in the future",
        ));
    }

    let match_ = load_match(state, match_id).await?;
    match match_.status {
        S::Ready | S::Scheduled | S::CheckingIn => {}
        S::Forfeit | S::Cancelled => {
            return Err(ApiError::bad_request(format!(
                "This match is {}: its check-in window has already closed. Reset the match \
                 first, then open check-in again",
                match_.status
            )));
        }
        other => {
            return Err(ApiError::bad_request(format!(
                "The check-in window can only be set while a match is ready, scheduled or \
                 checking in (this match is {other})"
            )));
        }
    }
    if !match_.has_both_participants() {
        return Err(ApiError::bad_request(
            "Both participants must be set before check-in can open",
        ));
    }

    let (updated, from) = rescue_repo(state)
        .set_check_in_window(match_id, deadline, audit)
        .await?;

    tracing::info!(
        %match_id, admin = %audit.admin_id, %from, %deadline, reason = %audit.reason,
        "admin set the check-in window"
    );

    // Opening the window is what makes a match veto-gated: do what the
    // lifecycle pass does when it opens one. A failure here is reported by
    // the diagnostics (`veto_missing`), not turned into a failed request —
    // the window itself is open.
    if !updated.veto_required
        && let Err(e) = ensure_veto_session(state, &updated).await
    {
        tracing::warn!(%match_id, error = %e, "rescue: veto session setup failed after opening check-in");
    }

    load_match(state, match_id).await
}

// =============================================================================
// Veto restart
// =============================================================================

/// What stands in the way of creating a veto session for this match, if
/// anything. `Ok(None)` means a session can be created.
async fn veto_setup_problem(
    state: &AppState,
    match_: &TournamentMatch,
) -> Result<Option<String>, ApiError> {
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;
    let stage_override = state
        .tournament_service
        .get_stage(match_.stage_id)
        .await?
        .and_then(|s| s.map_veto_format);
    let format = match resolve_match_veto_format(
        stage_override.as_deref(),
        tournament.default_map_veto_format.as_deref(),
        match_.match_format,
        &state.plugin_manager,
    ) {
        Ok(Some(format)) => format,
        Ok(None) => {
            return Ok(Some(
                "this tournament has no map veto format configured".to_string(),
            ));
        }
        Err(_) => {
            return Ok(Some(
                "the tournament's map veto format could not be resolved".to_string(),
            ));
        }
    };

    let pool_size = if let Ok(Some(pool)) = state
        .tournament_map_pool_repo
        .get_effective(match_.tournament_id, Some(match_.stage_id))
        .await
    {
        pool.maps.len()
    } else if let Ok(Some(game)) = state
        .game_repo
        .find_by_id(tournament.game_id.as_uuid())
        .await
    {
        crate::handlers::games::extract_map_pool(&game).len()
    } else {
        0
    };
    if pool_size < format.min_map_pool {
        return Ok(Some(format!(
            "the map pool has {pool_size} maps but the {} format needs at least {}",
            format.id, format.min_map_pool
        )));
    }
    Ok(None)
}

/// Discard the match's veto and start a fresh one.
pub async fn restart_veto(
    state: &AppState,
    match_id: TournamentMatchId,
    audit: &RescueAudit,
) -> Result<TournamentMatch, ApiError> {
    use crate::websocket::messages::{LobbyBroadcast, VetoStateBroadcast};
    use TournamentMatchStatus as S;

    let match_ = load_match(state, match_id).await?;
    if !matches!(match_.status, S::CheckingIn | S::PickBan | S::InProgress) {
        return Err(ApiError::bad_request(format!(
            "The veto can only be restarted while a match is checking in, in pick/ban or in \
             progress (this match is {})",
            match_.status
        )));
    }
    let repo = rescue_repo(state);
    if repo.has_live_result_claim(match_id).await? {
        return Err(ApiError::bad_request(
            "A result has already been reported for this match. Reset the match instead of \
             restarting its veto",
        ));
    }
    if let Some(problem) = veto_setup_problem(state, &match_).await? {
        return Err(ApiError::bad_request(format!(
            "The veto cannot be restarted: {problem}"
        )));
    }

    // The server holds a config built from the veto being thrown away.
    release_live_reservation(state, match_id, "veto restarted by an admin").await;

    let (updated, from) = repo.discard_veto_for_restart(match_id, audit).await?;

    let created = ensure_veto_session(state, &updated).await.map_err(|e| {
        ApiError::internal(format!(
            "The old veto was discarded but a new one could not be created: {e}"
        ))
    })?;
    if !created {
        return Err(ApiError::internal(
            "The old veto was discarded but a new one could not be created",
        ));
    }
    let session_state = state.veto_service.get_session_state(match_id).await?;
    let session = if session_state.session.status.can_start() {
        state
            .veto_service
            .start_session(session_state.session.id)
            .await?
    } else {
        session_state.session
    };

    tracing::info!(
        %match_id, admin = %audit.admin_id, %from, session_id = %session.id,
        reason = %audit.reason, "admin restarted the veto"
    );

    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
            session: crate::dto::responses::VetoSessionResponse::from(session),
        }));
    }

    load_match(state, match_id).await
}

// =============================================================================
// Game server
// =============================================================================

/// What clearing a match off its server did.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServerClearResponse {
    /// A live reservation existed and was released (the server is free for
    /// the next match).
    pub released_reservation: bool,
    /// The game server acknowledged the end-match command: the loaded match
    /// config, and with it the player whitelist, is gone.
    pub server_cleared: bool,
    /// The server that was addressed, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    /// Why the server was not cleared, when it was not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Send end-match to a server and say honestly whether it took.
async fn end_match_on_server(
    state: &AppState,
    server_id: portal_core::ids::GameServerId,
) -> (bool, Option<String>) {
    match state
        .agent_manager
        .send_command(server_id, AgentCommand::EndMatch)
        .await
    {
        Ok(outcome) if outcome.ok => (true, None),
        Ok(outcome) => (
            false,
            Some(format!(
                "the game server rejected the end-match command: {}",
                outcome
                    .error
                    .or(outcome.output)
                    .unwrap_or_else(|| "no reason given".to_string())
            )),
        ),
        Err(e) => (
            false,
            Some(format!(
                "the game server's agent could not be reached ({e}); the match config may still \
                 be loaded on it"
            )),
        ),
    }
}

/// Clear the match's config off its game server and release the reservation.
///
/// Works with or without a live reservation: a config left loaded by a
/// reservation that already failed or was cancelled is cleared too, as long
/// as the server has not moved on to another match.
pub async fn clear_server(
    state: &AppState,
    match_id: TournamentMatchId,
    reason: &str,
) -> Result<ServerClearResponse, ApiError> {
    load_match(state, match_id).await?;

    let live = state
        .server_reservation_repo
        .find_live_by_match(match_id)
        .await?;

    if let Some(reservation) = live {
        let (server_name, server_cleared, detail) = match reservation.server_id {
            Some(server_id) => {
                let name = state
                    .game_server_registry
                    .get(server_id)
                    .await
                    .ok()
                    .map(|s| s.name);
                let (cleared, detail) = end_match_on_server(state, server_id).await;
                (name, cleared, detail)
            }
            None => (
                None,
                false,
                Some("the match was still queued and had not been given a server".to_string()),
            ),
        };
        state
            .server_reservation_repo
            .release(reservation.id, ReservationStatus::Cancelled, Some(reason))
            .await?;
        game_server_flow::broadcast_assignment(state, match_id, "cancelled", None, Some(reason));
        return Ok(ServerClearResponse {
            released_reservation: true,
            server_cleared,
            server_name,
            detail,
        });
    }

    let latest = state
        .server_reservation_repo
        .find_latest_by_match(match_id)
        .await?;
    let Some(server_id) = latest.and_then(|r| r.server_id) else {
        return Ok(ServerClearResponse {
            released_reservation: false,
            server_cleared: false,
            server_name: None,
            detail: Some("this match has not been loaded on any game server".to_string()),
        });
    };
    let server_name = state
        .game_server_registry
        .get(server_id)
        .await
        .ok()
        .map(|s| s.name);
    if state
        .server_reservation_repo
        .has_live_for_server(server_id)
        .await?
    {
        return Ok(ServerClearResponse {
            released_reservation: false,
            server_cleared: false,
            server_name,
            detail: Some(
                "the server this match used is now holding another match; it was left alone"
                    .to_string(),
            ),
        });
    }
    let (server_cleared, detail) = end_match_on_server(state, server_id).await;
    Ok(ServerClearResponse {
        released_reservation: false,
        server_cleared,
        server_name,
        detail,
    })
}

/// One player the match config would list for a team.
#[derive(Debug, Serialize, ToSchema)]
pub struct RosterCheckPlayer {
    pub player_id: String,
    pub display_name: String,
    /// The player's linked SteamID64, as stored.
    pub steam_id: Option<String>,
    /// Whether the player can be put in the server's whitelist.
    pub valid: bool,
    /// `roster` or `substitution`.
    pub source: String,
    /// Why the player is not valid, when they are not.
    pub problem: Option<String>,
}

/// One side of the match as the config would list it.
#[derive(Debug, Serialize, ToSchema)]
pub struct RosterCheckTeam {
    pub registration_id: String,
    pub name: String,
    /// Players with a usable Steam ID.
    pub usable_players: usize,
    pub players: Vec<RosterCheckPlayer>,
    pub problems: Vec<String>,
}

/// Whether a MatchZy config can be built for the match, and from whom.
#[derive(Debug, Serialize, ToSchema)]
pub struct RosterCheckResponse {
    /// False when the config cannot be built or a team cannot ready up.
    pub ok: bool,
    pub players_per_team: u32,
    pub min_players_to_ready: u32,
    pub teams: Vec<RosterCheckTeam>,
    /// Problems that are not about one team (e.g. the veto has not finished).
    pub problems: Vec<String>,
}

impl RosterCheckResponse {
    /// The problems that stop a config from being built at all (as opposed
    /// to a short-handed team, which loads but cannot ready up).
    fn blocking_problems(&self) -> Vec<String> {
        let mut out = self.problems.clone();
        for team in &self.teams {
            let invalid: Vec<&str> = team
                .players
                .iter()
                .filter(|p| !p.valid)
                .map(|p| p.display_name.as_str())
                .collect();
            if !invalid.is_empty() {
                out.push(format!(
                    "players on \"{}\" have no usable Steam ID: {}",
                    team.name,
                    invalid.join(", ")
                ));
            }
            if team.players.is_empty() {
                out.push(format!("\"{}\" has no players", team.name));
            }
        }
        out
    }
}

async fn roster_check_team(
    state: &AppState,
    match_: &TournamentMatch,
    registration_id: TournamentRegistrationId,
    fallback_name: &str,
    substitutes: &[portal_core::ids::PlayerId],
    min_players_to_ready: u32,
) -> Result<RosterCheckTeam, ApiError> {
    let name = state
        .registration_service
        .get_registration(registration_id)
        .await
        .map_or_else(|_| fallback_name.to_string(), |r| r.participant_name);

    let player_ids =
        game_server_flow::effective_player_ids(state, match_.id, registration_id).await?;

    let mut players = Vec::with_capacity(player_ids.len());
    for player_id in player_ids {
        let player = state.player_service.get_player(player_id).await?;
        let steam_id = player.steam_id.clone().filter(|s| !s.trim().is_empty());
        let (valid, problem) = match steam_id.as_deref() {
            None => (false, Some("no linked Steam ID".to_string())),
            Some(s) if s.parse::<u64>().is_err() => (false, Some("invalid Steam ID".to_string())),
            Some(_) => (true, None),
        };
        players.push(RosterCheckPlayer {
            player_id: player_id.to_string(),
            display_name: player.display_name,
            steam_id,
            valid,
            source: if substitutes.contains(&player_id) {
                "substitution"
            } else {
                "roster"
            }
            .to_string(),
            problem,
        });
    }

    let usable_players = players.iter().filter(|p| p.valid).count();
    let mut problems = Vec::new();
    let invalid = players.len() - usable_players;
    if invalid > 0 {
        problems.push(format!(
            "{invalid} player(s) have no usable Steam ID and cannot be whitelisted on the server"
        ));
    }
    if usable_players < usize::try_from(min_players_to_ready).unwrap_or(usize::MAX) {
        problems.push(format!(
            "only {usable_players} usable players; {min_players_to_ready} needed to ready up"
        ));
    }

    Ok(RosterCheckTeam {
        registration_id: registration_id.to_string(),
        name,
        usable_players,
        players,
        problems,
    })
}

/// Check, without changing anything, whether a server config can be built
/// for the match from the rosters as they stand now.
pub async fn roster_check(
    state: &AppState,
    match_id: TournamentMatchId,
) -> Result<RosterCheckResponse, ApiError> {
    let match_ = load_match(state, match_id).await?;
    roster_check_for(state, &match_).await
}

async fn roster_check_for(
    state: &AppState,
    match_: &TournamentMatch,
) -> Result<RosterCheckResponse, ApiError> {
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;
    let settings = GameServerSettings::from_tournament_settings(&tournament.settings);
    // Same resolution as `game_server_flow::build_config`.
    let players_per_team = settings
        .players_per_team
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| (1..=16).contains(n))
        .or(
            match state
                .game_repo
                .find_by_id(tournament.game_id.as_uuid())
                .await
            {
                Ok(Some(game)) => u32::try_from(game.team_size_default).ok(),
                _ => None,
            },
        )
        .unwrap_or(5);
    let min_players_to_ready = settings
        .min_players_to_ready
        .and_then(|n| u32::try_from(n).ok())
        .map_or(players_per_team, |n| n.clamp(1, players_per_team));

    let mut problems = Vec::new();
    if !state.gameserver_enabled {
        problems.push("game-server integration is disabled on this portal".to_string());
    }
    if match_.veto_required {
        match state.veto_service.get_session_state(match_.id).await {
            Ok(veto) if veto.session.status == VetoStatus::Completed => {}
            Ok(_) => problems.push("the map veto has not completed".to_string()),
            Err(_) => problems.push("the match requires a map veto but has none".to_string()),
        }
    }

    let substitutes: Vec<portal_core::ids::PlayerId> = state
        .match_substitution_repo
        .list_applied_by_match(match_.id)
        .await?
        .into_iter()
        .filter_map(|s| s.player_in_id)
        .collect();

    let mut teams = Vec::with_capacity(2);
    for (registration_id, fallback) in [
        (
            match_.participant1_registration_id,
            match_.participant1_name.as_deref().unwrap_or("Team 1"),
        ),
        (
            match_.participant2_registration_id,
            match_.participant2_name.as_deref().unwrap_or("Team 2"),
        ),
    ] {
        match registration_id {
            Some(registration_id) => teams.push(
                roster_check_team(
                    state,
                    match_,
                    registration_id,
                    fallback,
                    &substitutes,
                    min_players_to_ready,
                )
                .await?,
            ),
            None => problems.push(format!("{fallback} has not been decided yet")),
        }
    }

    let ok = problems.is_empty() && teams.iter().all(|t| t.problems.is_empty());
    Ok(RosterCheckResponse {
        ok,
        players_per_team,
        min_players_to_ready,
        teams,
        problems,
    })
}

/// Rebuild the match config from the rosters as they stand now and load it:
/// clear the server, drop the old reservation, create and allocate a new one.
pub async fn reload_server(
    state: &AppState,
    match_id: TournamentMatchId,
    audit: &RescueAudit,
) -> Result<ServerReservation, ApiError> {
    let match_ = load_match(state, match_id).await?;
    if match_.status.is_terminal() {
        return Err(ApiError::bad_request(format!(
            "This match is {}; it does not need a server",
            match_.status
        )));
    }

    // Refuse before tearing anything down: a reload that cannot build a
    // config would only swap a loaded match for a failed reservation.
    let check = roster_check_for(state, &match_).await?;
    let blocking = check.blocking_problems();
    if !blocking.is_empty() {
        return Err(ApiError::bad_request(format!(
            "The match config cannot be rebuilt: {}",
            blocking.join("; ")
        )));
    }

    let reason = format!("config reloaded by an admin: {}", audit.reason);
    clear_server(state, match_id, &reason).await?;

    let reservation = game_server_flow::request_assignment(state, match_id).await?;

    if let Err(e) = rescue_repo(state)
        .log_admin_action(match_id, match_.status, "server config reload", audit)
        .await
    {
        tracing::warn!(%match_id, error = %e, "rescue: could not log the server reload");
    }
    tracing::info!(
        %match_id, admin = %audit.admin_id, reservation_id = %reservation.id,
        status = %reservation.status, reason = %audit.reason,
        "admin reloaded the match config on the game server"
    );
    Ok(reservation)
}

// =============================================================================
// Completion retry
// =============================================================================

/// Result of re-running a match's completion.
#[derive(Debug, Serialize, ToSchema)]
pub struct CompletionRetryResponse {
    /// Whether settlement ran to the end.
    pub ran: bool,
    /// What it did, in words.
    pub outcome: String,
}

/// Re-run settlement for a decided match: advance the winner if that never
/// happened, recompute the standings, and move the stage on if it is done.
pub async fn retry_completion(
    state: &AppState,
    match_id: TournamentMatchId,
    audit: &RescueAudit,
) -> Result<CompletionRetryResponse, ApiError> {
    let match_ = load_match(state, match_id).await?;
    if !match_.status.is_decided() {
        return Err(ApiError::bad_request(format!(
            "Only a completed or forfeited match can have its completion re-run (this match \
             is {})",
            match_.status
        )));
    }

    let settled = state.match_completion_saga.settle_result(match_id).await?;
    let rederived = state
        .match_completion_saga
        .rederive_bracket(match_.bracket_id)
        .await?;

    if let Err(e) = rescue_repo(state)
        .log_admin_action(match_id, match_.status, "completion retry", audit)
        .await
    {
        tracing::warn!(%match_id, error = %e, "rescue: could not log the completion retry");
    }

    let mut parts = vec![if settled.progression_ran {
        "progression ran for this match"
    } else {
        "progression had already run (or is waiting on a result review)"
    }];
    parts.push("standings were recomputed");
    if settled.stage_advanced || rederived.stage_advanced {
        parts.push("the stage advanced");
    }
    Ok(CompletionRetryResponse {
        ran: true,
        outcome: parts.join("; "),
    })
}

// =============================================================================
// Diagnostics
// =============================================================================

/// The match's veto session, summarised.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticsVeto {
    pub session_id: String,
    pub status: VetoStatus,
    pub current_action_number: u32,
    pub action_deadline: Option<DateTime<Utc>>,
}

/// The match's newest server reservation, summarised.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticsReservation {
    pub id: String,
    pub status: ReservationStatus,
    pub server_name: Option<String>,
    pub failure_reason: Option<String>,
    /// 1-based place in the server queue while `pending`.
    pub queue_position: Option<i64>,
    pub retry_count: i32,
    pub updated_at: DateTime<Utc>,
}

/// A pending schedule proposal.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticsProposal {
    pub id: String,
    pub proposed_by_registration_id: String,
    pub expires_at: DateTime<Utc>,
}

/// A pending result claim.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticsClaim {
    pub id: String,
    /// `server`, `participant` or `admin`.
    pub source: String,
    pub auto_confirm_at: Option<DateTime<Utc>>,
}

/// The match's newest completion saga.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticsSaga {
    pub status: String,
    pub failed_step: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Something wrong with the match, and the rescue action that fixes it.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticsProblem {
    /// Stable identifier (closed list — see the handler docs).
    pub code: String,
    /// What is wrong, for the admin.
    pub message: String,
    /// The rescue action to offer: `set_time`, `extend_checkin`,
    /// `reset_veto`, `reset`, `assign_server`, `reload_server`,
    /// `clear_server`, `roster_check`, `retry_completion`.
    pub suggested_action: String,
}

/// Everything an admin needs to see to work out why a match is stuck.
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchDiagnosticsResponse {
    pub status: TournamentMatchStatus,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub check_in_deadline: Option<DateTime<Utc>>,
    pub participant1_checked_in_at: Option<DateTime<Utc>>,
    pub participant2_checked_in_at: Option<DateTime<Utc>>,
    pub veto_required: bool,
    pub veto: Option<DiagnosticsVeto>,
    pub reservation: Option<DiagnosticsReservation>,
    pub pending_proposal: Option<DiagnosticsProposal>,
    pub pending_claim: Option<DiagnosticsClaim>,
    pub open_dispute_id: Option<String>,
    pub disputed: bool,
    pub saga: Option<DiagnosticsSaga>,
    pub problems: Vec<DiagnosticsProblem>,
}

fn problem(code: &str, suggested_action: &str, message: impl Into<String>) -> DiagnosticsProblem {
    DiagnosticsProblem {
        code: code.to_string(),
        message: message.into(),
        suggested_action: suggested_action.to_string(),
    }
}

/// Whether the forfeit deciding this match came from the check-in timeout.
fn forfeit_is_no_show(snapshot: &MatchRescueSnapshot) -> bool {
    snapshot.active_forfeit.as_ref().is_some_and(|f| {
        f.triggered_by_system
            && (f.forfeit_type == "no_show"
                || f.reason
                    .as_deref()
                    .is_some_and(|r| r.to_lowercase().contains("check")))
    })
}

/// Read-only: the match's state across its side tables, and what is wrong.
pub async fn diagnostics(
    state: &AppState,
    match_id: TournamentMatchId,
) -> Result<MatchDiagnosticsResponse, ApiError> {
    use TournamentMatchStatus as S;

    let match_ = load_match(state, match_id).await?;
    let snapshot = rescue_repo(state).snapshot(match_id).await?;
    let now = Utc::now();

    let veto_session = state
        .veto_service
        .get_session_state(match_id)
        .await
        .ok()
        .map(|v| v.session);

    let latest_reservation = state
        .server_reservation_repo
        .find_latest_by_match(match_id)
        .await?;
    let reservation_live = latest_reservation.as_ref().is_some_and(|r| {
        matches!(
            r.status,
            ReservationStatus::Pending
                | ReservationStatus::Configuring
                | ReservationStatus::Ready
                | ReservationStatus::Live
        )
    });

    let mut problems = Vec::new();

    if match_.status == S::Scheduled && match_.scheduled_at.is_none() {
        problems.push(problem(
            "scheduled_without_time",
            "set_time",
            "The match is marked scheduled but has no time, so check-in will never open.",
        ));
    }

    if match_.status == S::CheckingIn {
        let both_in = match_.participant1_checked_in_at.is_some()
            && match_.participant2_checked_in_at.is_some();
        if both_in {
            problems.push(problem(
                "both_checked_in_not_advanced",
                "reset_veto",
                "Both teams have checked in but the match did not move on to pick/ban.",
            ));
        } else if match_.check_in_deadline.is_some_and(|d| d < now) {
            problems.push(problem(
                "checkin_expired",
                "extend_checkin",
                "The check-in deadline has passed; absent teams are about to be forfeited.",
            ));
        }
    }

    if matches!(match_.status, S::Forfeit | S::Cancelled) && forfeit_is_no_show(&snapshot) {
        if match_.status == S::Cancelled {
            problems.push(problem(
                "cancelled_by_no_show",
                "reset",
                "Neither team checked in before the deadline, so the match was called off.",
            ));
        } else {
            problems.push(problem(
                "forfeit_by_no_show",
                "reset",
                "One team did not check in before the deadline and was forfeited.",
            ));
        }
    }

    if matches!(match_.status, S::CheckingIn | S::PickBan) && veto_session.is_none() {
        // checking_in without a session is only a problem when the tournament
        // wants a veto; pick_ban without one always is.
        let wants_veto = veto_setup_problem(state, &match_).await?.is_none();
        if match_.status == S::PickBan || wants_veto {
            problems.push(problem(
                "veto_missing",
                "reset_veto",
                "The match should have a map veto but no veto session exists.",
            ));
        }
    }
    if match_.status == S::PickBan
        && let Some(session) = &veto_session
    {
        let stalled = match session.status {
            VetoStatus::Pending | VetoStatus::Cancelled => true,
            VetoStatus::CoinFlip | VetoStatus::InProgress => session
                .action_deadline
                .is_some_and(|d| d + Duration::minutes(VETO_STALLED_AFTER_MINUTES) < now),
            VetoStatus::Completed => false,
        };
        if stalled {
            problems.push(problem(
                "veto_stalled",
                "reset_veto",
                "The map veto is not progressing.",
            ));
        }
    }

    if let Some(reservation) = &latest_reservation {
        if reservation_live && match_.status.is_terminal() {
            problems.push(problem(
                "reservation_live_match_terminal",
                "clear_server",
                "The match is over but it is still holding the game server.",
            ));
        } else if reservation_live
            && matches!(
                reservation.status,
                ReservationStatus::Pending | ReservationStatus::Configuring
            )
            && reservation.updated_at + Duration::minutes(RESERVATION_STUCK_AFTER_MINUTES) < now
        {
            problems.push(problem(
                "reservation_stuck",
                "reload_server",
                "The game server has not finished loading this match.",
            ));
        } else if reservation.status == ReservationStatus::Failed && !match_.status.is_terminal() {
            problems.push(problem(
                "reservation_failed",
                "reload_server",
                format!(
                    "Setting up the game server failed: {}",
                    reservation
                        .failure_reason
                        .as_deref()
                        .unwrap_or("no reason recorded")
                ),
            ));
        }
    }
    if match_.status == S::PickBan
        && !reservation_live
        && veto_session
            .as_ref()
            .is_some_and(|s| s.status == VetoStatus::Completed)
        && !latest_reservation
            .as_ref()
            .is_some_and(|r| r.status == ReservationStatus::Failed)
    {
        problems.push(problem(
            "pick_ban_veto_complete_no_server",
            "assign_server",
            "The veto has finished but no game server was requested for the match.",
        ));
    }

    if matches!(match_.status, S::CheckingIn | S::PickBan | S::InProgress)
        && match_.has_both_participants()
        && let Ok(check) = roster_check_for(state, &match_).await
        && check
            .teams
            .iter()
            .any(|t| t.players.iter().any(|p| !p.valid) || t.players.is_empty())
    {
        problems.push(problem(
            "roster_invalid",
            "roster_check",
            "A rostered player has no usable Steam ID; the server config cannot list them.",
        ));
    }

    if match_.status.is_decided()
        && let Some(saga) = &snapshot.saga
        && matches!(saga.status.as_str(), "failed" | "paused")
    {
        problems.push(problem(
            "saga_failed",
            "retry_completion",
            format!(
                "Completing this match {} at step {}; standings or the bracket may be behind.",
                if saga.status == "paused" {
                    "is paused"
                } else {
                    "failed"
                },
                saga.failed_step.as_deref().unwrap_or("unknown")
            ),
        ));
    }

    if match_.disputed && snapshot.open_dispute_id.is_none() && match_.status != S::Disputed {
        problems.push(problem(
            "disputed_flag_stale",
            "reset",
            "The match is flagged as disputed but has no open dispute.",
        ));
    }

    let reservation = match latest_reservation {
        Some(r) => {
            let server_name = match r.server_id {
                Some(id) => state
                    .game_server_registry
                    .get(id)
                    .await
                    .ok()
                    .map(|s| s.name),
                None => None,
            };
            let queue_position = if r.status == ReservationStatus::Pending {
                state
                    .server_reservation_repo
                    .count_pending_before(r.created_at)
                    .await
                    .ok()
                    .map(|ahead| ahead + 1)
            } else {
                None
            };
            Some(DiagnosticsReservation {
                id: r.id.to_string(),
                status: r.status,
                server_name,
                failure_reason: r.failure_reason,
                queue_position,
                retry_count: r.retry_count,
                updated_at: r.updated_at,
            })
        }
        None => None,
    };

    Ok(MatchDiagnosticsResponse {
        status: match_.status,
        scheduled_at: match_.scheduled_at,
        check_in_deadline: match_.check_in_deadline,
        participant1_checked_in_at: match_.participant1_checked_in_at,
        participant2_checked_in_at: match_.participant2_checked_in_at,
        veto_required: match_.veto_required,
        veto: veto_session.map(|s| DiagnosticsVeto {
            session_id: s.id.to_string(),
            status: s.status,
            current_action_number: s.current_action_number,
            action_deadline: s.action_deadline,
        }),
        reservation,
        pending_proposal: snapshot.pending_proposal.map(|p| DiagnosticsProposal {
            id: p.id.to_string(),
            proposed_by_registration_id: p.proposed_by_registration_id.to_string(),
            expires_at: p.expires_at,
        }),
        pending_claim: snapshot.pending_claim.map(|c| DiagnosticsClaim {
            id: c.id.to_string(),
            source: c.source,
            auto_confirm_at: c.auto_confirm_at,
        }),
        open_dispute_id: snapshot.open_dispute_id.map(|id| id.to_string()),
        disputed: match_.disputed,
        saga: snapshot.saga.map(|s| DiagnosticsSaga {
            status: s.status,
            failed_step: s.failed_step,
            updated_at: s.updated_at,
        }),
        problems,
    })
}
