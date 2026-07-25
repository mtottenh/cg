//! Match ↔ game-server orchestration (MatchZy Phases 2–3).
//!
//! Everything that ties a match to a server: reservation creation at veto
//! completion, allocation (§6.7), config generation (§6.3), driving the
//! agent's `load_match`, ingesting MatchZy webhooks into match state
//! (§6.4–§6.5), and releasing servers. Lives in the API layer because it
//! spans domain services, the plugin config builder, and the agent
//! WebSocket channel.

use std::collections::BTreeMap;

use chrono::{Duration, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{ServerReservationId, TournamentMatchId, TournamentRegistrationId};
use portal_core::types::{ReservationStatus, TournamentMatchStatus};
use portal_domain::entities::{
    GameServer, ServerEvent, ServerReservation, TournamentMatch, TransitionTrigger, VetoStatus,
};
use portal_domain::repositories::{
    CreateServerEvent, CreateServerReservation, LeagueTeamMemberRepository, ServerEventRepository,
    ServerReservationRepository, TournamentMatchGameRepository, TournamentMatchRepository,
};
use portal_domain::services::game_server::{
    derive_map_sides, generate_connect_password, generate_reservation_token, hash_token,
};
use portal_plugins::games::cs2::{
    MatchzyConfigInput, MatchzyTeam, build_matchzy_config, validate_matchzy_input,
};

use crate::state::AppState;
use crate::websocket::messages::{LiveScoreBroadcast, LobbyBroadcast, ServerAssignmentBroadcast};

/// Config-fetch token TTL (minted fresh on every load attempt).
const CONFIG_TOKEN_TTL_MINUTES: i64 = 15;
/// Load attempts before a reservation is failed.
const MAX_LOAD_RETRIES: i32 = 3;
/// Auto-confirm window for server-sourced result claims.
const DEFAULT_AUTO_CONFIRM_MINUTES: i64 = 10;

/// Per-tournament game-server settings (`tournaments.settings.game_server`).
#[derive(Debug, Clone, Default)]
pub struct GameServerSettings {
    pub enabled: bool,
    pub region: Option<String>,
    pub hostname: Option<String>,
    pub auto_confirm_minutes: Option<i64>,
    pub cvars: BTreeMap<String, String>,
}

impl GameServerSettings {
    /// Parse from the tournament `settings` JSONB. Absent → all defaults.
    #[must_use]
    pub fn from_tournament_settings(settings: &serde_json::Value) -> Self {
        let Some(gs) = settings.get("game_server") else {
            return Self::default();
        };
        let cvars = gs
            .get("cvars")
            .and_then(|c| c.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            enabled: gs.get("enabled").and_then(serde_json::Value::as_bool) == Some(true),
            region: gs
                .get("region")
                .and_then(|r| r.as_str())
                .map(str::to_string),
            hostname: gs
                .get("hostname")
                .and_then(|h| h.as_str())
                .map(str::to_string),
            auto_confirm_minutes: gs
                .get("auto_confirm_minutes")
                .and_then(serde_json::Value::as_i64),
            cvars,
        }
    }
}

/// Hook called when a match's veto completes (§6.6 trigger 1).
///
/// Creates a queued reservation and attempts immediate allocation when the
/// tournament has opted in. Errors are logged, never propagated — veto
/// completion must not fail because server setup hit a snag.
pub async fn on_veto_completed(state: &AppState, match_id: TournamentMatchId) {
    let result = async {
        let match_ = get_match(state, match_id).await?;
        let tournament = state
            .tournament_service
            .get_tournament(match_.tournament_id)
            .await?;
        let settings = GameServerSettings::from_tournament_settings(&tournament.settings);
        if !settings.enabled {
            return Ok::<Option<ServerReservation>, DomainError>(None);
        }
        let reservation = request_assignment(state, match_id).await?;
        Ok(Some(reservation))
    }
    .await;

    match result {
        Ok(Some(r)) => {
            tracing::info!(match_id = %match_id, reservation_id = %r.id, status = %r.status,
                "server reservation created on veto completion");
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(match_id = %match_id, error = %e,
                "server assignment on veto completion failed");
        }
    }
}

/// Create (or return) the live reservation for a match and try to allocate
/// + load immediately.
///
/// Used by the veto hook, the admin endpoint, and re-driven by the
/// lifecycle pass while `pending`.
pub async fn request_assignment(
    state: &AppState,
    match_id: TournamentMatchId,
) -> Result<ServerReservation, DomainError> {
    if let Some(existing) = state
        .server_reservation_repo
        .find_live_by_match(match_id)
        .await?
    {
        return Ok(existing);
    }

    let match_ = get_match(state, match_id).await?;
    if match_.participant1_registration_id.is_none()
        || match_.participant2_registration_id.is_none()
    {
        return Err(DomainError::InvalidState(
            "both participants must be set before assigning a server".into(),
        ));
    }
    if match_.status.is_terminal() {
        return Err(DomainError::InvalidState(format!(
            "match is {} — no server needed",
            match_.status
        )));
    }

    // The config needs a completed veto: it is the source of maps + sides.
    if match_.veto_required {
        let veto = state.veto_service.get_session_state(match_id).await?;
        if veto.session.status != VetoStatus::Completed {
            return Err(DomainError::InvalidState(
                "map veto has not completed yet".into(),
            ));
        }
    } else {
        return Err(DomainError::InvalidState(
            "server integration currently requires a map veto (maps come from the veto result)"
                .into(),
        ));
    }

    let event_token = generate_reservation_token();
    let reservation = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id,
            connect_password: generate_connect_password(),
            gotv_password: Some(generate_connect_password()),
            // Placeholder; a fresh config token is minted per load attempt.
            config_token_hash: hash_token(&generate_reservation_token()),
            event_token_hash: hash_token(&event_token),
            config_token_expires_at: Utc::now(),
        })
        .await?;

    // Build + persist the config now: it embeds the event token (only held
    // in memory here) and fails fast on missing Steam IDs, surfacing an
    // actionable reason in the UI instead of a dead reservation later.
    match build_config(state, &match_, &reservation, &event_token).await {
        Ok(config) => {
            state
                .server_reservation_repo
                .store_config(reservation.id, &config)
                .await?;
        }
        Err(reason) => {
            state
                .server_reservation_repo
                .mark_failed(reservation.id, &reason)
                .await?;
            broadcast_assignment(state, match_id, "failed", None, Some(&reason));
            return Err(DomainError::InvalidState(reason));
        }
    }

    let reservation = try_allocate_and_load(state, reservation).await?;
    broadcast_reservation(state, &reservation).await;
    Ok(reservation)
}

/// Attempt allocation + `load_match` for a pending reservation.
/// No free server is not an error — the reservation stays queued.
pub async fn try_allocate_and_load(
    state: &AppState,
    reservation: ServerReservation,
) -> Result<ServerReservation, DomainError> {
    if reservation.status != ReservationStatus::Pending {
        return Ok(reservation);
    }
    let match_ = get_match(state, reservation.match_id).await?;
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;
    let settings = GameServerSettings::from_tournament_settings(&tournament.settings);

    let heartbeat_cutoff = Utc::now()
        - Duration::seconds(portal_domain::services::game_server::HEARTBEAT_STALENESS_SECS);
    let allocated = state
        .server_reservation_repo
        .allocate(
            reservation.id,
            tournament.game_id,
            match_.tournament_id,
            settings.region.as_deref(),
            heartbeat_cutoff,
            Utc::now(),
        )
        .await?;

    let Some((reservation, server)) = allocated else {
        tracing::info!(reservation_id = %reservation.id, "no eligible server free; queued");
        return Ok(reservation);
    };

    send_load(state, reservation, &server).await
}

/// Mint a fresh config token, instruct the agent to load, advance to
/// `configuring`. On failure the retry counter decides requeue vs fail.
async fn send_load(
    state: &AppState,
    reservation: ServerReservation,
    server: &GameServer,
) -> Result<ServerReservation, DomainError> {
    use crate::websocket::agent_manager::AgentCommand;

    let config_token = generate_reservation_token();
    state
        .server_reservation_repo
        .set_config_token(
            reservation.id,
            &hash_token(&config_token),
            Utc::now() + Duration::minutes(CONFIG_TOKEN_TTL_MINUTES),
        )
        .await?;

    let base = state.public_base_url.trim_end_matches('/');
    let url = format!(
        "{base}/v1/gameserver/match-config/{}",
        reservation.matchzy_id
    );

    // Best-effort reset first: MatchZy refuses to load over a loaded match.
    let _ = state
        .agent_manager
        .send_command(server.id, AgentCommand::EndMatch)
        .await;

    let load = state
        .agent_manager
        .send_command(
            server.id,
            AgentCommand::LoadMatch {
                url,
                header_name: "Authorization".to_string(),
                header_value: format!("Bearer {config_token}"),
            },
        )
        .await;

    match load {
        Ok(outcome) if outcome.ok => {
            state
                .server_reservation_repo
                .set_status(reservation.id, ReservationStatus::Configuring)
                .await?;
            state
                .game_server_registry
                .set_server_status(server.id, portal_core::types::GameServerStatus::Configuring)
                .await?;
            tracing::info!(reservation_id = %reservation.id, server = %server.name,
                "load_match sent");
            let mut r = reservation;
            r.status = ReservationStatus::Configuring;
            Ok(r)
        }
        Ok(outcome) => {
            let reason = outcome
                .error
                .or(outcome.output)
                .unwrap_or_else(|| "agent reported failure".into());
            handle_load_failure(state, reservation, &reason).await
        }
        Err(e) => handle_load_failure(state, reservation, &e.to_string()).await,
    }
}

async fn handle_load_failure(
    state: &AppState,
    reservation: ServerReservation,
    reason: &str,
) -> Result<ServerReservation, DomainError> {
    let retries = state
        .server_reservation_repo
        .increment_retry(reservation.id)
        .await?;
    tracing::warn!(reservation_id = %reservation.id, retries, reason,
        "load_match failed");
    if retries >= MAX_LOAD_RETRIES {
        state
            .server_reservation_repo
            .release(
                reservation.id,
                ReservationStatus::Failed,
                Some(&format!(
                    "server setup failed after {retries} attempts: {reason}"
                )),
            )
            .await?;
        broadcast_assignment(state, reservation.match_id, "failed", None, Some(reason));
    }
    // Below the retry cap the reservation keeps its server and stays
    // `pending`-equivalent for the lifecycle pass to retry the load.
    state
        .server_reservation_repo
        .find_by_id(reservation.id)
        .await?
        .ok_or_else(|| DomainError::Internal("reservation vanished".into()))
}

/// Cancel a match's live reservation (admin action or match cancellation).
pub async fn cancel_assignment(
    state: &AppState,
    match_id: TournamentMatchId,
    reason: &str,
) -> Result<(), DomainError> {
    use crate::websocket::agent_manager::AgentCommand;

    let Some(reservation) = state
        .server_reservation_repo
        .find_live_by_match(match_id)
        .await?
    else {
        return Err(DomainError::InvalidState(
            "this match has no live server reservation".into(),
        ));
    };
    if let Some(server_id) = reservation.server_id {
        let _ = state
            .agent_manager
            .send_command(server_id, AgentCommand::EndMatch)
            .await;
    }
    state
        .server_reservation_repo
        .release(reservation.id, ReservationStatus::Cancelled, Some(reason))
        .await?;
    broadcast_assignment(state, match_id, "cancelled", None, Some(reason));
    Ok(())
}

// =============================================================================
// Config generation (§6.3)
// =============================================================================

/// Build the MatchZy config. `Err` carries a user-actionable reason.
async fn build_config(
    state: &AppState,
    match_: &TournamentMatch,
    reservation: &ServerReservation,
    event_token: &str,
) -> Result<serde_json::Value, String> {
    let veto = state
        .veto_service
        .get_session_state(match_.id)
        .await
        .map_err(|e| format!("veto state unavailable: {e}"))?;

    let (Some(reg1), Some(reg2)) = (
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ) else {
        return Err("both participants must be set".into());
    };

    let team1 = roster_for(
        state,
        reg1,
        match_.participant1_name.as_deref().unwrap_or("Team 1"),
    )
    .await?;
    let team2 = roster_for(
        state,
        reg2,
        match_.participant2_name.as_deref().unwrap_or("Team 2"),
    )
    .await?;

    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await
        .map_err(|e| format!("tournament unavailable: {e}"))?;
    let settings = GameServerSettings::from_tournament_settings(&tournament.settings);

    let map_sides = derive_map_sides(&veto.session.selected_maps, &veto.actions, reg1);
    let players_per_team =
        u32::try_from(team1.players.len().max(team2.players.len()).max(5)).unwrap_or(5);

    let base = state.public_base_url.trim_end_matches('/');
    let input = MatchzyConfigInput {
        matchzy_id: reservation.matchzy_id,
        maplist: veto.session.selected_maps.clone(),
        map_sides,
        team1,
        team2,
        players_per_team,
        min_players_to_ready: 1,
        hostname: settings
            .hostname
            .unwrap_or_else(|| "Portal | {TEAM1} vs {TEAM2}".to_string()),
        connect_password: reservation.connect_password.clone(),
        gotv_password: reservation.gotv_password.clone(),
        event_url: format!("{base}/v1/gameserver/events"),
        event_token: event_token.to_string(),
        extra_cvars: settings.cvars,
    };
    validate_matchzy_input(&input)?;

    // Materialize the per-map game rows the event pipeline will fill in.
    ensure_match_games(state, match_, &veto).await;

    Ok(build_matchzy_config(&input))
}

/// Resolve a registration's roster to `(steamid64, name)` pairs.
async fn roster_for(
    state: &AppState,
    registration_id: TournamentRegistrationId,
    fallback_name: &str,
) -> Result<MatchzyTeam, String> {
    let reg = state
        .registration_service
        .get_registration(registration_id)
        .await
        .map_err(|e| format!("registration unavailable: {e}"))?;

    let mut players: Vec<(String, String)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    let mut push_player =
        |steam_id: Option<&str>, name: String| match steam_id.and_then(|s| s.parse::<u64>().ok()) {
            Some(steam64) => players.push((steam64.to_string(), name)),
            None => missing.push(name),
        };

    if let Some(team_season_id) = reg.team_season_id {
        let members = state
            .league_team_member_repo
            .list_members_with_players(team_season_id)
            .await
            .map_err(|e| format!("roster unavailable: {e}"))?;
        for member in members {
            let player = state
                .player_service
                .get_player(member.player_id)
                .await
                .map_err(|e| format!("player lookup failed: {e}"))?;
            push_player(player.steam_id.as_deref(), player.display_name);
        }
    } else if let Some(player_id) = reg.player_id {
        let player = state
            .player_service
            .get_player(player_id)
            .await
            .map_err(|e| format!("player lookup failed: {e}"))?;
        push_player(player.steam_id.as_deref(), player.display_name);
    }

    if !missing.is_empty() {
        return Err(format!(
            "players on \"{fallback_name}\" have no linked Steam ID: {}",
            missing.join(", ")
        ));
    }

    Ok(MatchzyTeam {
        id: registration_id.to_string(),
        name: reg.participant_name,
        players,
    })
}

/// Create the `tournament_match_games` rows from the veto result (maps in
/// play order, picker + side-selector attribution). Idempotent.
async fn ensure_match_games(
    state: &AppState,
    match_: &TournamentMatch,
    veto: &portal_domain::entities::VetoSessionState,
) {
    for (index, map) in veto.session.selected_maps.iter().enumerate() {
        let game_number = i32::try_from(index + 1).unwrap_or(i32::MAX);
        let existing = state
            .tournament_match_game_repo
            .find_by_number(match_.id, game_number)
            .await
            .ok()
            .flatten();
        let game = match existing {
            Some(g) => g,
            None => {
                match state
                    .tournament_match_game_repo
                    .create(portal_domain::repositories::CreateTournamentMatchGame {
                        match_id: match_.id,
                        game_number,
                        map_id: Some(map.clone()),
                    })
                    .await
                {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!(match_id = %match_.id, game_number, error = %e,
                            "failed to create match game row");
                        continue;
                    }
                }
            }
        };
        let pick_action = veto
            .actions
            .iter()
            .find(|a| a.map_id == *map && a.side_selection.is_some());
        if let Some(action) = pick_action {
            let _ = state
                .tournament_match_game_repo
                .update(
                    game.id,
                    portal_domain::repositories::UpdateTournamentMatchGame {
                        map_id: None,
                        map_picked_by: action.performed_by_registration_id,
                        side_selection_by: action.side_selected_by_registration_id,
                        game_data: None,
                    },
                )
                .await;
        }
    }
}

// =============================================================================
// Event ingestion + processing (§6.4–§6.5)
// =============================================================================

/// Store and process one MatchZy webhook event. Returns quickly; processing
/// failures are recorded on the event row, never surfaced to MatchZy
/// (which would drop the event anyway — it never retries).
pub async fn ingest_server_event(
    state: &AppState,
    reservation: &ServerReservation,
    payload: serde_json::Value,
) -> Result<(), DomainError> {
    let event_type = payload
        .get("event")
        .and_then(|e| e.as_str())
        .unwrap_or("unknown")
        .to_string();
    let map_number = payload
        .get("map_number")
        .and_then(serde_json::Value::as_i64);
    let round_number = payload
        .get("round_number")
        .and_then(serde_json::Value::as_i64);

    let Some(server_id) = reservation.server_id else {
        return Err(DomainError::InvalidState(
            "reservation has no server assigned".into(),
        ));
    };

    let inserted = state
        .server_event_repo
        .insert(CreateServerEvent {
            reservation_id: reservation.id,
            server_id,
            event_type: event_type.clone(),
            map_number: map_number.and_then(|n| i32::try_from(n).ok()),
            round_number: round_number.and_then(|n| i32::try_from(n).ok()),
            payload,
        })
        .await?;

    // Dedupe hit: MatchZy replay or duplicate — already handled.
    let Some(event) = inserted else {
        return Ok(());
    };

    let outcome = process_event(state, reservation, &event).await;
    let error_text = outcome.as_ref().err().map(ToString::to_string);
    state
        .server_event_repo
        .mark_processed(event.id, error_text.as_deref())
        .await?;
    if let Err(e) = outcome {
        tracing::warn!(reservation_id = %reservation.id, event = %event.event_type,
            error = %e, "server event processing failed");
    }
    Ok(())
}

async fn process_event(
    state: &AppState,
    reservation: &ServerReservation,
    event: &ServerEvent,
) -> Result<(), DomainError> {
    match event.event_type.as_str() {
        "series_start" => {
            state
                .server_reservation_repo
                .set_status(reservation.id, ReservationStatus::Ready)
                .await?;
            if let Some(server_id) = reservation.server_id {
                state
                    .game_server_registry
                    .set_server_status(server_id, portal_core::types::GameServerStatus::InMatch)
                    .await?;
            }
            broadcast_reservation_by_id(state, reservation.id).await;
        }
        "going_live" => {
            state
                .server_reservation_repo
                .mark_live(reservation.id, Utc::now())
                .await?;
            // pick_ban → in_progress; tolerate already-started.
            let transition = state
                .match_lifecycle_service
                .transition(
                    reservation.match_id,
                    TournamentMatchStatus::InProgress,
                    TransitionTrigger::System {
                        job_name: "gameserver".to_string(),
                    },
                    Some("server went live".to_string()),
                )
                .await;
            if let Err(e) = transition {
                tracing::debug!(match_id = %reservation.match_id, error = %e,
                    "going_live transition skipped");
            }
            if let Some(game_number) = event.map_number.map(|n| n + 1)
                && let Ok(Some(game)) = state
                    .tournament_match_game_repo
                    .find_by_number(reservation.match_id, game_number)
                    .await
            {
                let _ = state.tournament_match_game_repo.start(game.id).await;
            }
            broadcast_reservation_by_id(state, reservation.id).await;
        }
        "round_end" => {
            state.server_reservation_repo.touch(reservation.id).await?;
            broadcast_live_score(state, reservation, &event.payload);
        }
        "map_result" => {
            state.server_reservation_repo.touch(reservation.id).await?;
            apply_map_result(state, reservation, event).await?;
            broadcast_live_score(state, reservation, &event.payload);
        }
        "series_end" => {
            apply_series_end(state, reservation, event).await?;
        }
        other => {
            tracing::debug!(event_type = other, "unhandled matchzy event stored");
        }
    }
    Ok(())
}

/// Per-map scores from a MatchZy team object (§2.2: use `score`, never
/// `winner.team` — that field is the map LEADER, not the winner).
fn team_scores(payload: &serde_json::Value) -> (i32, i32) {
    let score = |team: &str| {
        payload
            .get(team)
            .and_then(|t| t.get("score"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|s| i32::try_from(s).ok())
            .unwrap_or(0)
    };
    (score("team1"), score("team2"))
}

async fn apply_map_result(
    state: &AppState,
    reservation: &ServerReservation,
    event: &ServerEvent,
) -> Result<(), DomainError> {
    let match_ = get_match(state, reservation.match_id).await?;
    let (s1, s2) = team_scores(&event.payload);
    let winner = if s1 >= s2 {
        match_.participant1_registration_id
    } else {
        match_.participant2_registration_id
    };
    let Some(winner) = winner else {
        return Err(DomainError::InvalidState(
            "match has no participants".into(),
        ));
    };
    let game_number = event.map_number.map_or(1, |n| n + 1);
    if let Some(game) = state
        .tournament_match_game_repo
        .find_by_number(reservation.match_id, game_number)
        .await?
    {
        state
            .tournament_match_game_repo
            .submit_result(game.id, s1, s2, winner, None, None)
            .await?;
    }
    Ok(())
}

async fn apply_series_end(
    state: &AppState,
    reservation: &ServerReservation,
    event: &ServerEvent,
) -> Result<(), DomainError> {
    let match_ = get_match(state, reservation.match_id).await?;
    let series = |key: &str| {
        event
            .payload
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .and_then(|s| i32::try_from(s).ok())
            .unwrap_or(0)
    };
    let (s1, s2) = (series("team1_series_score"), series("team2_series_score"));

    if s1 == s2 {
        // clinch_series is always set; a tie is a protocol violation.
        state
            .server_reservation_repo
            .release(
                reservation.id,
                ReservationStatus::Completed,
                Some("series ended tied — needs admin review"),
            )
            .await?;
        return Err(DomainError::InvalidState(format!(
            "series ended tied {s1}-{s2}; result not auto-submitted"
        )));
    }

    let (Some(reg1), Some(reg2)) = (
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ) else {
        return Err(DomainError::InvalidState(
            "match has no participants".into(),
        ));
    };
    let winner = if s1 > s2 { reg1 } else { reg2 };

    // Per-map results from the game rows the map_result events filled in.
    let games = state
        .tournament_match_game_repo
        .list_by_match(reservation.match_id)
        .await?;
    let game_results: Vec<portal_domain::entities::GameResultInput> = games
        .iter()
        .filter(|g| g.winner_registration_id.is_some())
        .map(|g| portal_domain::entities::GameResultInput {
            game_number: g.game_number,
            map_id: g.map_id.clone().unwrap_or_default(),
            participant1_score: g.participant1_score.unwrap_or(0),
            participant2_score: g.participant2_score.unwrap_or(0),
            duration_seconds: None,
            evidence_ids: Vec::new(),
            demo_link_id: None,
        })
        .collect();

    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;
    let settings = GameServerSettings::from_tournament_settings(&tournament.settings);
    let auto_confirm_at = Utc::now()
        + Duration::minutes(
            settings
                .auto_confirm_minutes
                .unwrap_or(DEFAULT_AUTO_CONFIRM_MINUTES),
        );

    let claim = state
        .result_service
        .submit_server_claim(
            reservation.match_id,
            winner,
            s1,
            s2,
            game_results,
            auto_confirm_at,
        )
        .await;
    match claim {
        Ok(claim) => {
            tracing::info!(match_id = %reservation.match_id, claim_id = %claim.id,
                "server result claim submitted ({s1}-{s2})");
        }
        Err(e) => {
            // A participant claim already pending/confirmed: first claim
            // wins (§10); the review flow reconciles conflicts.
            tracing::warn!(match_id = %reservation.match_id, error = %e,
                "server result claim not submitted");
        }
    }

    state
        .server_reservation_repo
        .release(reservation.id, ReservationStatus::Completed, None)
        .await?;
    broadcast_assignment(state, reservation.match_id, "completed", None, None);
    Ok(())
}

// =============================================================================
// Broadcasts (§7.3)
// =============================================================================

/// Connect info for participant-facing surfaces.
#[must_use]
pub fn connect_info(
    reservation: &ServerReservation,
    server: &GameServer,
) -> crate::websocket::messages::ServerConnectInfo {
    crate::websocket::messages::ServerConnectInfo {
        ip_address: server.ip_address.to_string(),
        port: server.port,
        connect_password: reservation.connect_password.clone(),
        gotv_port: server.gotv_port,
        gotv_password: reservation.gotv_password.clone(),
    }
}

async fn broadcast_reservation_by_id(state: &AppState, id: ServerReservationId) {
    if let Ok(Some(reservation)) = state.server_reservation_repo.find_by_id(id).await {
        broadcast_reservation(state, &reservation).await;
    }
}

async fn broadcast_reservation(state: &AppState, reservation: &ServerReservation) {
    let connect = match reservation.server_id {
        Some(server_id)
            if matches!(
                reservation.status,
                ReservationStatus::Ready | ReservationStatus::Live
            ) =>
        {
            match state.game_server_registry.get(server_id).await {
                Ok(server) => Some(connect_info(reservation, &server)),
                Err(_) => None,
            }
        }
        _ => None,
    };
    broadcast_assignment(
        state,
        reservation.match_id,
        &reservation.status.to_string(),
        connect,
        reservation.failure_reason.as_deref(),
    );
}

fn broadcast_assignment(
    state: &AppState,
    match_id: TournamentMatchId,
    status: &str,
    connect: Option<crate::websocket::messages::ServerConnectInfo>,
    reason: Option<&str>,
) {
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        lobby.broadcast(LobbyBroadcast::ServerAssignmentUpdate(
            ServerAssignmentBroadcast {
                status: status.to_string(),
                connect,
                reason: reason.map(str::to_string),
            },
        ));
    }
}

fn broadcast_live_score(
    state: &AppState,
    reservation: &ServerReservation,
    payload: &serde_json::Value,
) {
    let (s1, s2) = team_scores(payload);
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&reservation.match_id) {
        lobby.broadcast(LobbyBroadcast::LiveScoreUpdate(LiveScoreBroadcast {
            map_number: payload
                .get("map_number")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            team1_score: s1,
            team2_score: s2,
            round_number: payload
                .get("round_number")
                .and_then(serde_json::Value::as_i64),
        }));
    }
}

async fn get_match(
    state: &AppState,
    match_id: TournamentMatchId,
) -> Result<TournamentMatch, DomainError> {
    state
        .tournament_match_repo
        .find_by_id(match_id)
        .await?
        .ok_or_else(|| DomainError::InvalidState(format!("match {match_id} not found")))
}

// =============================================================================
// Trigger drain task
// =============================================================================

/// Spawn the veto-completion drain: receives match ids fired by the veto
/// handlers and runs [`on_veto_completed`] outside the request path.
/// Started by portal-app beside the lifecycle task.
#[must_use]
pub fn spawn_server_assignment_task(
    state: AppState,
    shutdown: std::sync::Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let receiver = state.server_assignment_rx.lock().await.take();
        let Some(mut receiver) = receiver else {
            tracing::warn!("server assignment receiver already taken; task exiting");
            return;
        };
        loop {
            tokio::select! {
                () = shutdown.notified() => break,
                msg = receiver.recv() => match msg {
                    Some(match_id) => on_veto_completed(&state, match_id).await,
                    None => break,
                },
            }
        }
        tracing::info!("server assignment task stopped");
    })
}

// =============================================================================
// Lifecycle pass (§6.6): allocation, load retry, reconciliation
// =============================================================================

/// Counters from one reservation maintenance pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReservationPassSummary {
    /// Pending reservations that got a server + load this pass.
    pub allocated: u32,
    /// Stuck `configuring` reservations re-driven.
    pub load_retried: u32,
    /// Stale ready/live reservations reconciled via `get5_status`.
    pub reconciled: u32,
    /// Errors (each logged; the pass continues).
    pub errors: u32,
}

/// How long `configuring` may sit without `series_start` before re-driving.
const CONFIGURING_STUCK_MINUTES: i64 = 2;
/// How long ready/live may go without events before reconciliation.
const ACTIVE_STALE_MINUTES: i64 = 5;

/// One maintenance pass over reservations; called from the lifecycle loop.
pub async fn run_reservation_pass(state: &AppState) -> ReservationPassSummary {
    let mut summary = ReservationPassSummary::default();
    let now = Utc::now();

    // 1. Queued reservations → allocate + load.
    match state.server_reservation_repo.list_pending(50).await {
        Ok(pending) => {
            for reservation in pending {
                // A pending reservation that already holds a server is a
                // failed load awaiting retry — handled in phase 2 shape via
                // send-load below.
                let had_server = reservation.server_id.is_some();
                let outcome = if had_server {
                    match reservation
                        .server_id
                        .map(|id| state.game_server_registry.get(id))
                    {
                        Some(fut) => match fut.await {
                            Ok(server) => send_load_public(state, reservation, &server).await,
                            Err(e) => Err(e),
                        },
                        None => unreachable!(),
                    }
                } else {
                    try_allocate_and_load(state, reservation).await
                };
                match outcome {
                    Ok(r) if r.status == ReservationStatus::Configuring => {
                        summary.allocated += 1;
                        broadcast_reservation(state, &r).await;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "reservation allocation failed");
                        summary.errors += 1;
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "lifecycle: list_pending reservations failed");
            summary.errors += 1;
        }
    }

    // 2. `configuring` with no series_start → re-drive load_match.
    match state
        .server_reservation_repo
        .list_stuck_configuring(now - Duration::minutes(CONFIGURING_STUCK_MINUTES))
        .await
    {
        Ok(stuck) => {
            for reservation in stuck {
                let Some(server_id) = reservation.server_id else {
                    continue;
                };
                match state.game_server_registry.get(server_id).await {
                    Ok(server) => match send_load_public(state, reservation, &server).await {
                        Ok(_) => summary.load_retried += 1,
                        Err(e) => {
                            tracing::warn!(error = %e, "load retry failed");
                            summary.errors += 1;
                        }
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, "load retry: server lookup failed");
                        summary.errors += 1;
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "lifecycle: list_stuck_configuring failed");
            summary.errors += 1;
        }
    }

    // 3. ready/live with no events → reconcile via get5_status (§6.6:
    //    MatchZy webhooks have no retries; this is the safety net).
    match state
        .server_reservation_repo
        .list_active_stale(now - Duration::minutes(ACTIVE_STALE_MINUTES))
        .await
    {
        Ok(stale) => {
            for reservation in stale {
                match reconcile_stale(state, &reservation).await {
                    Ok(true) => summary.reconciled += 1,
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(reservation_id = %reservation.id, error = %e,
                            "reconciliation failed");
                        summary.errors += 1;
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "lifecycle: list_active_stale failed");
            summary.errors += 1;
        }
    }

    summary
}

/// Re-drive `load_match` for a reservation that already holds a server.
async fn send_load_public(
    state: &AppState,
    reservation: ServerReservation,
    server: &GameServer,
) -> Result<ServerReservation, DomainError> {
    send_load(state, reservation, server).await
}

/// Ask the agent for `get5_status` and settle a silent reservation.
/// Returns `Ok(true)` when the reservation was moved to a terminal state.
async fn reconcile_stale(
    state: &AppState,
    reservation: &ServerReservation,
) -> Result<bool, DomainError> {
    use crate::websocket::agent_manager::AgentCommand;

    let Some(server_id) = reservation.server_id else {
        return Ok(false);
    };
    let outcome = state
        .agent_manager
        .send_command(server_id, AgentCommand::Status)
        .await?;
    let Some(raw) = outcome.output else {
        return Ok(false);
    };
    let Ok(status) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Ok(false);
    };
    let gamestate = status
        .get("gamestate")
        .and_then(|g| g.as_str())
        .unwrap_or("unknown");

    // Still warming up or live: nothing to reconcile, just note activity so
    // we don't re-poll every pass.
    if !matches!(gamestate, "none" | "post_game") {
        state.server_reservation_repo.touch(reservation.id).await?;
        return Ok(false);
    }

    // The series ended (or the match was wiped) and we missed the webhooks.
    let series = |team: &str| {
        status
            .get(team)
            .and_then(|t| t.get("series_score"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
    };
    let scores = (series("team1"), series("team2"));
    tracing::warn!(reservation_id = %reservation.id, gamestate, ?scores,
        "reconciling silent reservation");

    if let (Some(s1), Some(s2)) = scores
        && s1 != s2
    {
        // Synthesize a series_end so the normal pipeline (claim, release,
        // broadcast) runs — including the dedupe guard if the real webhook
        // arrives late after all.
        let synthetic = serde_json::json!({
            "event": "series_end",
            "matchid": reservation.matchzy_id,
            "team1_series_score": s1,
            "team2_series_score": s2,
            "reconciled": true,
        });
        ingest_server_event(state, reservation, synthetic).await?;
        return Ok(true);
    }

    state
        .server_reservation_repo
        .release(
            reservation.id,
            ReservationStatus::Failed,
            Some("no events received and server is idle — needs admin review"),
        )
        .await?;
    broadcast_assignment(
        state,
        reservation.match_id,
        "failed",
        None,
        Some("server went silent; match result needs admin review"),
    );
    Ok(true)
}
