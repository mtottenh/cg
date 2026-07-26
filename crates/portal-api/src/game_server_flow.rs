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
    CreateServerEvent, CreateServerReservation, LeagueTeamMemberRepository,
    MatchSubstitutionRepository as _, ServerEventRepository, ServerReservationRepository,
    TournamentMapPoolRepository, TournamentMatchGameRepository, TournamentMatchRepository,
};
use portal_domain::services::game_server::{
    derive_map_sides, generate_connect_password, generate_reservation_token, hash_token,
};
use portal_plugins::games::cs2::{
    MatchzyConfigInput, MatchzyMapRef, MatchzyTeam, build_matchzy_config, matchzy_map_tokens,
    validate_matchzy_input, workshop_numeric_id,
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
    /// Minutes a `ready` server may wait with nobody going live before
    /// admins are flagged (§6.6; default 20).
    pub no_show_minutes: Option<i64>,
    /// `settings.game_server.substitution_policy` — `"admin_approval"`
    /// routes requests through admin review; anything else applies rostered
    /// subs immediately (§6.8 default `roster_free`).
    pub substitution_policy: Option<String>,
}

impl GameServerSettings {
    /// Whether substitutions require admin approval (§6.8).
    #[must_use]
    pub fn substitution_policy_admin_approval(&self) -> bool {
        self.substitution_policy.as_deref() == Some("admin_approval")
    }

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
            no_show_minutes: gs
                .get("no_show_minutes")
                .and_then(serde_json::Value::as_i64),
            substitution_policy: gs
                .get("substitution_policy")
                .and_then(|p| p.as_str())
                .map(str::to_string),
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
    if !state.gameserver_enabled {
        return Err(DomainError::InvalidState(
            "game-server integration is disabled (PORTAL_GAMESERVER_ENABLED)".into(),
        ));
    }
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

    // With a veto the config's maps/sides come from its result; without
    // one (§6.6 trigger 2) they come from the tournament map pool, all
    // knife-for-sides — build_config branches on the same flag.
    if match_.veto_required {
        let veto = state.veto_service.get_session_state(match_id).await?;
        if veto.session.status != VetoStatus::Completed {
            return Err(DomainError::InvalidState(
                "map veto has not completed yet".into(),
            ));
        }
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
            match_.scheduled_at,
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
        // §10: fail this reservation, quarantine the server, and re-queue
        // the match on a fresh reservation for another server (M11).
        let match_id = reservation.match_id;
        let bad_server = reservation.server_id;
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
        if let Some(server_id) = bad_server {
            let _ = state
                .game_server_registry
                .set_server_status(server_id, portal_core::types::GameServerStatus::Error)
                .await;
        }
        broadcast_assignment(state, match_id, "failed", None, Some(reason));
        match request_assignment_boxed(state, match_id).await {
            Ok(replacement) => {
                tracing::info!(match_id = %match_id, reservation_id = %replacement.id,
                    "re-queued match on a fresh reservation after load failure");
                return Ok(replacement);
            }
            Err(e) => {
                tracing::warn!(match_id = %match_id, error = %e,
                    "could not re-queue match after load failure");
            }
        }
    }
    // Below the retry cap the reservation keeps its server and stays
    // `pending`-equivalent for the lifecycle pass to retry the load.
    state
        .server_reservation_repo
        .find_by_id(reservation.id)
        .await?
        .ok_or_else(|| DomainError::Internal("reservation vanished".into()))
}

/// Boxed indirection: `handle_load_failure` → `request_assignment` →
/// `try_allocate_and_load` → `send_load` → `handle_load_failure` would be
/// infinitely-sized without it.
fn request_assignment_boxed<'a>(
    state: &'a AppState,
    match_id: TournamentMatchId,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ServerReservation, DomainError>> + Send + 'a>,
> {
    Box::pin(request_assignment(state, match_id))
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
///
/// `pub` so integration tests can exercise config generation (catalog →
/// maplist token translation) without a live agent connection.
pub async fn build_config(
    state: &AppState,
    match_: &TournamentMatch,
    reservation: &ServerReservation,
    event_token: &str,
) -> Result<serde_json::Value, String> {
    let (Some(reg1), Some(reg2)) = (
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ) else {
        return Err("both participants must be set".into());
    };

    // Maps + sides: veto result when a veto ran; else the tournament map
    // pool in listed order with knife-for-sides (§6.6 trigger 2).
    let (maplist, veto_state): (
        Vec<String>,
        Option<portal_domain::entities::VetoSessionState>,
    ) = if match_.veto_required {
        let veto = state
            .veto_service
            .get_session_state(match_.id)
            .await
            .map_err(|e| format!("veto state unavailable: {e}"))?;
        (veto.session.selected_maps.clone(), Some(veto))
    } else {
        let maps_required = usize::try_from(match_.maps_required.max(1)).unwrap_or(1);
        let pool = state
            .tournament_map_pool_repo
            .get_effective(match_.tournament_id, Some(match_.stage_id))
            .await
            .map_err(|e| format!("map pool unavailable: {e}"))?
            .ok_or_else(|| "tournament has no map pool configured".to_string())?;
        if pool.maps.len() < maps_required {
            return Err(format!(
                "map pool has {} maps but the match needs {maps_required}",
                pool.maps.len()
            ));
        }
        (pool.maps.into_iter().take(maps_required).collect(), None)
    };

    let team1 = roster_for(
        state,
        match_.id,
        reg1,
        match_.participant1_name.as_deref().unwrap_or("Team 1"),
    )
    .await?;
    let team2 = roster_for(
        state,
        match_.id,
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

    let map_sides = match &veto_state {
        Some(veto) => derive_map_sides(&maplist, &veto.actions, reg1),
        None => vec!["knife".to_string(); maplist.len()],
    };
    // §6.3/§12-Q3: team size comes from the admin-editable game config —
    // roster size must not widen the server (an 8-man roster is still 5v5).
    let game_row = state
        .game_repo
        .find_by_id(tournament.game_id.as_uuid())
        .await
        .ok()
        .flatten();
    let players_per_team = game_row
        .as_ref()
        .and_then(|game| u32::try_from(game.team_size_default).ok())
        .unwrap_or(5);

    // Resolve portal map ids to MatchZy tokens through the game's map
    // catalog: workshop item id when the map is workshop-hosted, else the
    // engine-level name. Ids missing from the catalog (edited after the
    // veto) pass through unchanged — for stock maps the id IS the name.
    let catalog = game_row
        .as_ref()
        .map(|game| {
            let plugin = state.plugin_manager.get(&game.plugin_id);
            crate::handlers::games::load_available_maps(game, &plugin)
        })
        .unwrap_or_default();
    let mut map_refs = Vec::with_capacity(maplist.len());
    for map_id in &maplist {
        let map_ref = match catalog.iter().find(|m| &m.id == map_id) {
            Some(m) => MatchzyMapRef {
                portal_id: m.id.clone(),
                engine_name: m.engine_name.clone(),
                workshop_id: match m.external_id.as_deref() {
                    Some(ext) => Some(workshop_numeric_id(ext).ok_or_else(|| {
                        format!("map \"{}\" has an unparseable workshop id: {ext}", m.id)
                    })?),
                    None => None,
                },
            },
            None => MatchzyMapRef {
                portal_id: map_id.clone(),
                engine_name: None,
                workshop_id: None,
            },
        };
        map_refs.push(map_ref);
    }
    let matchzy_maplist = matchzy_map_tokens(&map_refs)?;

    let base = state.public_base_url.trim_end_matches('/');
    let input = MatchzyConfigInput {
        matchzy_id: reservation.matchzy_id,
        maplist: matchzy_maplist,
        map_sides,
        team1,
        team2,
        players_per_team,
        // Full team must ready; short-handed subs lower it in-server via
        // css_readyrequired (§6.8).
        min_players_to_ready: players_per_team,
        hostname: settings
            .hostname
            .unwrap_or_else(|| "Portal | {TEAM1} vs {TEAM2}".to_string()),
        connect_password: reservation.connect_password.clone(),
        gotv_password: reservation.gotv_password.clone(),
        event_url: format!("{base}/v1/gameserver/events"),
        backup_url: format!("{base}/v1/gameserver/backups"),
        event_token: event_token.to_string(),
        extra_cvars: settings.cvars,
    };
    validate_matchzy_input(&input)?;

    // Materialize the per-map game rows the event pipeline will fill in.
    ensure_match_games(state, match_, &maplist, veto_state.as_ref()).await;

    Ok(build_matchzy_config(&input))
}

/// Resolve a registration's EFFECTIVE lineup to `(steamid64, name)` pairs:
/// the roster (or solo player) with applied substitutions swapped in
/// (§6.3 / §6.8 consistency rule — a rebuilt config must contain the
/// substitute, not the departed player).
async fn roster_for(
    state: &AppState,
    match_id: TournamentMatchId,
    registration_id: TournamentRegistrationId,
    fallback_name: &str,
) -> Result<MatchzyTeam, String> {
    let reg = state
        .registration_service
        .get_registration(registration_id)
        .await
        .map_err(|e| format!("registration unavailable: {e}"))?;

    let player_ids = effective_player_ids(state, match_id, registration_id)
        .await
        .map_err(|e| format!("effective roster unavailable: {e}"))?;

    let mut players: Vec<(String, String)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for player_id in player_ids {
        let player = state
            .player_service
            .get_player(player_id)
            .await
            .map_err(|e| format!("player lookup failed: {e}"))?;
        match player
            .steam_id
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok())
        {
            Some(steam64) => players.push((steam64.to_string(), player.display_name)),
            None => missing.push(player.display_name),
        }
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
    maplist: &[String],
    veto: Option<&portal_domain::entities::VetoSessionState>,
) {
    use portal_core::types::VetoActionType;

    for (index, map) in maplist.iter().enumerate() {
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
        // Pick attribution: the pick/decider action for this map — side
        // selection is separate and may be absent (deciders, knife mode).
        let Some(veto) = veto else { continue };
        let pick_action = veto.actions.iter().find(|a| {
            a.map_id == *map
                && matches!(
                    a.action_type,
                    VetoActionType::Pick | VetoActionType::Decider
                )
        });
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
            reservation_id: Some(reservation.id),
            server_id,
            event_type: event_type.clone(),
            map_number: map_number.and_then(|n| i32::try_from(n).ok()),
            round_number: round_number.and_then(|n| i32::try_from(n).ok()),
            payload,
        })
        .await?;
    let _ = &event_type;

    // Dedupe hit: MatchZy replay or duplicate. Lifecycle transitions must
    // still be recoverable — a re-driven `load_match` (§6.6) re-emits
    // `series_start` with the same dedupe key, and dropping it would strand
    // the reservation in `configuring` (M2). These events are idempotent,
    // so process them from an ephemeral row; scoring events stay dropped.
    let Some(event) = inserted else {
        if matches!(event_type.as_str(), "series_start" | "going_live") {
            let ephemeral = ServerEvent {
                id: portal_core::ids::ServerEventId::new(),
                reservation_id: Some(reservation.id),
                server_id: Some(server_id),
                event_type,
                map_number: map_number.and_then(|n| i32::try_from(n).ok()),
                round_number: None,
                payload: serde_json::Value::Null,
                processed: false,
                processed_at: None,
                processing_error: None,
                received_at: Utc::now(),
            };
            if let Err(e) = process_event(state, reservation, &ephemeral).await {
                tracing::warn!(reservation_id = %reservation.id, error = %e,
                    "replayed lifecycle event processing failed");
            }
        }
        return Ok(());
    };

    let outcome = process_event(state, reservation, &event).await;
    match outcome {
        Ok(()) => {
            state
                .server_event_repo
                .mark_processed(event.id, None)
                .await?;
        }
        Err(e) => {
            // Leave processed = FALSE: the lifecycle sweep retries once
            // and only then parks the row with its error (M3/§6.4).
            state
                .server_event_repo
                .record_processing_error(event.id, &e.to_string())
                .await?;
            tracing::warn!(reservation_id = %reservation.id, event = %event.event_type,
                error = %e, "server event processing failed; queued for retry");
        }
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
            // Halftime-blocked roster edits retry on the next round (§6.8).
            retry_applying_substitutions(state, reservation).await;
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
    if s1 == s2 {
        // A drawn map must not be awarded to anyone (review minor); leave
        // the game row open for admin resolution.
        tracing::warn!(match_id = %reservation.match_id, s1, s2,
            "map_result reported a draw; not recording a winner");
        return Ok(());
    }
    let winner = if s1 > s2 {
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
    if !state.gameserver_enabled {
        return summary;
    }
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

    // 3b. `ready` with nobody live past the no-show window: flag admins
    //     (never auto-forfeit, §6.6).
    match state
        .server_reservation_repo
        .list_active_stale(now - Duration::minutes(20))
        .await
    {
        Ok(stale) => {
            for reservation in stale {
                if reservation.status == ReservationStatus::Ready
                    && reservation.went_live_at.is_none()
                {
                    tracing::warn!(match_id = %reservation.match_id,
                        reservation_id = %reservation.id,
                        "server ready >20m with nobody connected — admin attention needed");
                    summary.errors += 0; // observability only; no state change
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "lifecycle: no-show sweep failed");
            summary.errors += 1;
        }
    }

    // 3c. Halftime-parked substitutions: retry (round_end also retries,
    //     but a silent server should not strand them — review minor).
    match state.match_substitution_repo.list_applying(20).await {
        Ok(applying) => {
            for substitution in applying {
                let Some(reservation_id) = substitution.reservation_id else {
                    continue;
                };
                if let Ok(Some(reservation)) = state
                    .server_reservation_repo
                    .find_by_id(reservation_id)
                    .await
                {
                    retry_applying_substitutions(state, &reservation).await;
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "lifecycle: substitution retry sweep failed");
            summary.errors += 1;
        }
    }

    // 4. Unprocessed events (a processing failure left them queued, §6.4):
    //    one retry, then park with the recorded error.
    match state.server_event_repo.list_unprocessed(20).await {
        Ok(events) => {
            for event in events {
                let Some(reservation_id) = event.reservation_id else {
                    let _ = state
                        .server_event_repo
                        .mark_processed(event.id, Some("no reservation"))
                        .await;
                    continue;
                };
                let Ok(Some(reservation)) = state
                    .server_reservation_repo
                    .find_by_id(reservation_id)
                    .await
                else {
                    let _ = state
                        .server_event_repo
                        .mark_processed(event.id, Some("reservation gone"))
                        .await;
                    continue;
                };
                let outcome = process_event(state, &reservation, &event).await;
                let error = outcome.as_ref().err().map(ToString::to_string);
                let _ = state
                    .server_event_repo
                    .mark_processed(event.id, error.as_deref())
                    .await;
                if error.is_none() {
                    summary.reconciled += 1;
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "lifecycle: list_unprocessed failed");
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

// =============================================================================
// Mid-series substitutions (§6.8)
// =============================================================================

/// Errors here are user-facing (`InvalidState`/`NotAuthorized`/`Conflict`).
#[allow(clippy::too_many_lines)]
pub async fn request_substitution(
    state: &AppState,
    match_id: TournamentMatchId,
    requester_user: portal_core::ids::UserId,
    requester_player: portal_core::ids::PlayerId,
    player_out: portal_core::ids::PlayerId,
    player_in: Option<portal_core::ids::PlayerId>,
) -> Result<portal_domain::entities::MatchSubstitution, DomainError> {
    use portal_core::types::SubstitutionStatus;
    use portal_domain::repositories::{CreateMatchSubstitution, MatchSubstitutionRepository};

    let match_ = get_match(state, match_id).await?;
    let reservation = state
        .server_reservation_repo
        .find_live_by_match(match_id)
        .await?
        .ok_or_else(|| {
            DomainError::InvalidState(
                "substitutions are only available while a server is assigned".into(),
            )
        })?;
    if !matches!(
        reservation.status,
        ReservationStatus::Ready | ReservationStatus::Live
    ) {
        return Err(DomainError::InvalidState(
            "the server is not ready yet — request the substitution once it is".into(),
        ));
    }

    // Which side is the outgoing player on?
    let mut side: Option<(TournamentRegistrationId, bool)> = None;
    for (reg_id, is_team1) in [
        (match_.participant1_registration_id, true),
        (match_.participant2_registration_id, false),
    ] {
        let Some(reg_id) = reg_id else { continue };
        if effective_player_ids(state, match_id, reg_id)
            .await?
            .contains(&player_out)
        {
            side = Some((reg_id, is_team1));
            break;
        }
    }
    let Some((registration_id, is_team1)) = side else {
        return Err(DomainError::InvalidState(
            "the outgoing player is not in this match's effective roster".into(),
        ));
    };

    // Authority: captain / owner / active delegate / solo player (§6.8
    // reuses the veto authority chain).
    state
        .veto_authorization_service
        .can_act_for_registration(registration_id, requester_user, requester_player)
        .await
        .map_err(|_| {
            DomainError::NotAuthorized(
                "only the captain (or a veto delegate) can request substitutions".into(),
            )
        })?;

    // Validate the incoming player: on the same team-season roster, has a
    // linked Steam ID, and is not already listed on either side.
    let incoming: Option<(portal_core::ids::PlayerId, String, String)> = if let Some(player_in) =
        player_in
    {
        let reg = state
            .registration_service
            .get_registration(registration_id)
            .await?;
        let Some(team_season_id) = reg.team_season_id else {
            return Err(DomainError::InvalidState(
                "solo registrations cannot substitute".into(),
            ));
        };
        let members = state
            .league_team_member_repo
            .list_members_with_players(team_season_id)
            .await?;
        if !members.iter().any(|m| m.player_id == player_in) {
            return Err(DomainError::InvalidState(
                "the substitute must be on the team-season roster (non-roster \
                 emergency subs are admin-only)"
                    .into(),
            ));
        }
        for (other_reg, _) in [
            (match_.participant1_registration_id, true),
            (match_.participant2_registration_id, false),
        ] {
            if let Some(other_reg) = other_reg
                && effective_player_ids(state, match_id, other_reg)
                    .await?
                    .contains(&player_in)
            {
                return Err(DomainError::Conflict(
                    "that player is already in the match".into(),
                ));
            }
        }
        let player = state.player_service.get_player(player_in).await?;
        let steam64 = player
            .steam_id
            .as_deref()
            .and_then(|sid| sid.parse::<u64>().ok())
            .ok_or_else(|| {
                DomainError::InvalidState(format!("{} has no linked Steam ID", player.display_name))
            })?;
        Some((player_in, steam64.to_string(), player.display_name))
    } else {
        None
    };

    // Substitutions take effect from the next game (completed games count).
    let completed = state
        .tournament_match_game_repo
        .count_completed(match_id)
        .await
        .unwrap_or(0);
    let from_game_number = i32::try_from(completed).unwrap_or(0) + 1;

    // Policy gate (§6.8): roster subs apply immediately by default.
    let tournament = state
        .tournament_service
        .get_tournament(match_.tournament_id)
        .await?;
    let settings = GameServerSettings::from_tournament_settings(&tournament.settings);
    let initial_status = if settings.substitution_policy_admin_approval() {
        SubstitutionStatus::AwaitingApproval
    } else {
        SubstitutionStatus::Pending
    };

    let substitution = state
        .match_substitution_repo
        .create(CreateMatchSubstitution {
            id: portal_core::ids::MatchSubstitutionId::new(),
            match_id,
            registration_id,
            reservation_id: Some(reservation.id),
            player_out_id: player_out,
            player_in_id: player_in,
            from_game_number,
            status: initial_status,
            requested_by: requester_user,
        })
        .await?;

    if initial_status == SubstitutionStatus::Pending {
        apply_substitution(state, &substitution, is_team1, incoming).await?;
    }
    state
        .match_substitution_repo
        .find_by_id(substitution.id)
        .await?
        .ok_or_else(|| DomainError::Internal("substitution vanished".into()))
}

/// The players currently listed for a registration: roster (or solo) with
/// applied substitutions swapped in (§6.8 consistency rule).
async fn effective_player_ids(
    state: &AppState,
    match_id: TournamentMatchId,
    registration_id: TournamentRegistrationId,
) -> Result<Vec<portal_core::ids::PlayerId>, DomainError> {
    use portal_domain::repositories::MatchSubstitutionRepository;

    let reg = state
        .registration_service
        .get_registration(registration_id)
        .await?;
    let mut players: Vec<portal_core::ids::PlayerId> = Vec::new();
    if let Some(team_season_id) = reg.team_season_id {
        players.extend(
            state
                .league_team_member_repo
                .list_members_with_players(team_season_id)
                .await?
                .into_iter()
                .map(|m| m.player_id),
        );
    } else if let Some(player_id) = reg.player_id {
        players.push(player_id);
    }
    for sub in state
        .match_substitution_repo
        .list_applied_by_match(match_id)
        .await?
        .into_iter()
        .filter(|sub| sub.registration_id == registration_id)
    {
        players.retain(|p| *p != sub.player_out_id);
        if let Some(player_in) = sub.player_in_id {
            players.push(player_in);
        }
    }
    Ok(players)
}

/// Send the roster edit to the server. Halftime rejections keep the row in
/// `applying`; round_end processing and the lifecycle pass retry it.
async fn apply_substitution(
    state: &AppState,
    substitution: &portal_domain::entities::MatchSubstitution,
    is_team1: bool,
    incoming: Option<(portal_core::ids::PlayerId, String, String)>,
) -> Result<(), DomainError> {
    use crate::websocket::agent_manager::AgentCommand;
    use portal_core::types::SubstitutionStatus;
    use portal_domain::repositories::MatchSubstitutionRepository;

    let Some(reservation_id) = substitution.reservation_id else {
        return Err(DomainError::InvalidState(
            "substitution has no reservation".into(),
        ));
    };
    let Some(reservation) = state
        .server_reservation_repo
        .find_by_id(reservation_id)
        .await?
    else {
        return Err(DomainError::InvalidState("reservation vanished".into()));
    };
    let Some(server_id) = reservation.server_id else {
        return Err(DomainError::InvalidState(
            "reservation has no server".into(),
        ));
    };

    state
        .match_substitution_repo
        .set_status(substitution.id, SubstitutionStatus::Applying, None, None)
        .await?;

    let out_player = state
        .player_service
        .get_player(substitution.player_out_id)
        .await?;
    let out_steam = out_player.steam_id.clone().unwrap_or_default();
    let team = if is_team1 { "team1" } else { "team2" };
    let add = incoming
        .as_ref()
        .map(|(_, steam64, name)| vec![(steam64.clone(), team.to_string(), name.clone())])
        .unwrap_or_default();

    let outcome = state
        .agent_manager
        .send_command(
            server_id,
            AgentCommand::RosterEdit {
                remove: vec![out_steam],
                add,
            },
        )
        .await;

    match outcome {
        Ok(result) if result.ok => {
            let output = result.output.unwrap_or_default();
            // MatchZy blocks roster commands during halftime (§2.1); the
            // console output is our only signal. Best-effort detection.
            if output.to_ascii_lowercase().contains("halftime") {
                tracing::info!(substitution_id = %substitution.id,
                    "roster edit blocked by halftime; will retry");
                return Ok(());
            }
            state
                .match_substitution_repo
                .mark_applied(substitution.id, Utc::now())
                .await?;
            // Short-handed: lower the in-server ready threshold (§6.8).
            if incoming.is_none() {
                let remaining = effective_player_ids(
                    state,
                    substitution.match_id,
                    substitution.registration_id,
                )
                .await
                .map(|p| p.len())
                .unwrap_or(4);
                let _ = state
                    .agent_manager
                    .send_command(
                        server_id,
                        AgentCommand::Exec {
                            command: format!("css_readyrequired {}", remaining.min(5)),
                        },
                    )
                    .await;
            }
            broadcast_lineup_update(state, substitution.match_id);
            Ok(())
        }
        Ok(result) => {
            let reason = result.error.or(result.output).unwrap_or_default();
            state
                .match_substitution_repo
                .set_status(
                    substitution.id,
                    SubstitutionStatus::Applying,
                    Some(&reason),
                    None,
                )
                .await?;
            Ok(())
        }
        Err(e) => {
            state
                .match_substitution_repo
                .set_status(
                    substitution.id,
                    SubstitutionStatus::Applying,
                    Some(&e.to_string()),
                    None,
                )
                .await?;
            Ok(())
        }
    }
}

/// Retry `applying` substitutions for a reservation (called on round_end
/// and from the lifecycle pass — the halftime-retry loop, §6.8).
pub async fn retry_applying_substitutions(state: &AppState, reservation: &ServerReservation) {
    use portal_domain::repositories::MatchSubstitutionRepository;

    let Ok(applying) = state
        .match_substitution_repo
        .list_applying_by_reservation(reservation.id)
        .await
    else {
        return;
    };
    for substitution in applying {
        let Ok(match_) = get_match(state, substitution.match_id).await else {
            continue;
        };
        let is_team1 = match_.participant1_registration_id == Some(substitution.registration_id);
        let incoming = match substitution.player_in_id {
            Some(player_in) => match state.player_service.get_player(player_in).await {
                Ok(player) => player
                    .steam_id
                    .as_deref()
                    .and_then(|sid| sid.parse::<u64>().ok())
                    .map(|steam64| (player_in, steam64.to_string(), player.display_name)),
                Err(_) => None,
            },
            None => None,
        };
        if substitution.player_in_id.is_some() && incoming.is_none() {
            continue;
        }
        if let Err(e) = apply_substitution(state, &substitution, is_team1, incoming).await {
            tracing::warn!(substitution_id = %substitution.id, error = %e,
                "substitution retry failed");
        }
    }
}

fn broadcast_lineup_update(state: &AppState, match_id: TournamentMatchId) {
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        lobby.broadcast(LobbyBroadcast::LineupUpdate);
    }
}

/// Apply an admin-approved substitution (status already reset to pending).
pub async fn approve_and_apply(
    state: &AppState,
    substitution_id: portal_core::ids::MatchSubstitutionId,
) -> Result<(), DomainError> {
    use portal_domain::repositories::MatchSubstitutionRepository;

    let Some(substitution) = state
        .match_substitution_repo
        .find_by_id(substitution_id)
        .await?
    else {
        return Err(DomainError::InvalidState("substitution not found".into()));
    };
    let match_ = get_match(state, substitution.match_id).await?;
    let is_team1 = match_.participant1_registration_id == Some(substitution.registration_id);
    let incoming = match substitution.player_in_id {
        Some(player_in) => {
            let player = state.player_service.get_player(player_in).await?;
            let steam64 = player
                .steam_id
                .as_deref()
                .and_then(|sid| sid.parse::<u64>().ok())
                .ok_or_else(|| {
                    DomainError::InvalidState(format!(
                        "{} has no linked Steam ID",
                        player.display_name
                    ))
                })?;
            Some((player_in, steam64.to_string(), player.display_name))
        }
        None => None,
    };
    apply_substitution(state, &substitution, is_team1, incoming).await
}

/// Restore a round backup onto the reservation's server (admin, Phase 4).
///
/// Picks the newest backup at or before `before_round` (or the latest),
/// mints a fresh config token, and drives `matchzy_loadbackup_url`.
pub async fn restore_backup(
    state: &AppState,
    match_id: TournamentMatchId,
    before_round: Option<i32>,
) -> Result<String, DomainError> {
    use crate::websocket::agent_manager::AgentCommand;
    use portal_domain::repositories::ServerEventRepository;

    let reservation = state
        .server_reservation_repo
        .find_live_by_match(match_id)
        .await?
        .ok_or_else(|| {
            DomainError::InvalidState("this match has no live server reservation".into())
        })?;
    let Some(server_id) = reservation.server_id else {
        return Err(DomainError::InvalidState(
            "reservation has no server".into(),
        ));
    };
    let backup = state
        .server_event_repo
        .latest_backup(reservation.id, before_round)
        .await?
        .ok_or_else(|| DomainError::InvalidState("no backups uploaded yet".into()))?;
    let filename = backup
        .payload
        .get("filename")
        .and_then(|f| f.as_str())
        .ok_or_else(|| DomainError::Internal("backup event has no filename".into()))?
        .to_string();

    let token = generate_reservation_token();
    state
        .server_reservation_repo
        .set_config_token(
            reservation.id,
            &hash_token(&token),
            Utc::now() + Duration::minutes(CONFIG_TOKEN_TTL_MINUTES),
        )
        .await?;
    let base = state.public_base_url.trim_end_matches('/');
    let url = format!(
        "{base}/v1/gameserver/backups/{}/{filename}",
        reservation.matchzy_id
    );

    let outcome = state
        .agent_manager
        .send_command(
            server_id,
            AgentCommand::LoadBackup {
                url,
                header_name: "Authorization".to_string(),
                header_value: format!("Bearer {token}"),
            },
        )
        .await?;
    if outcome.ok {
        Ok(filename)
    } else {
        Err(DomainError::Conflict(
            outcome
                .error
                .or(outcome.output)
                .unwrap_or_else(|| "restore failed".into()),
        ))
    }
}

/// Roster options for the substitution picker (§7.2): per side, the
/// currently listed players and the bench (rostered, not listed).
pub struct SubstitutionSide {
    pub registration_id: TournamentRegistrationId,
    pub participant_name: String,
    pub active: Vec<(portal_core::ids::PlayerId, String)>,
    pub bench: Vec<(portal_core::ids::PlayerId, String, bool)>,
}

pub async fn substitution_options(
    state: &AppState,
    match_id: TournamentMatchId,
) -> Result<Vec<SubstitutionSide>, DomainError> {
    let match_ = get_match(state, match_id).await?;
    let mut sides = Vec::new();
    for reg_id in [
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ]
    .into_iter()
    .flatten()
    {
        let reg = state.registration_service.get_registration(reg_id).await?;
        let active_ids = effective_player_ids(state, match_id, reg_id).await?;
        let mut active = Vec::new();
        for player_id in &active_ids {
            if let Ok(player) = state.player_service.get_player(*player_id).await {
                active.push((*player_id, player.display_name));
            }
        }
        let mut bench = Vec::new();
        if let Some(team_season_id) = reg.team_season_id {
            for member in state
                .league_team_member_repo
                .list_members_with_players(team_season_id)
                .await?
            {
                if active_ids.contains(&member.player_id) {
                    continue;
                }
                let has_steam = state
                    .player_service
                    .get_player(member.player_id)
                    .await
                    .ok()
                    .and_then(|p| p.steam_id)
                    .is_some_and(|sid| sid.parse::<u64>().is_ok());
                bench.push((member.player_id, member.display_name, has_steam));
            }
        }
        sides.push(SubstitutionSide {
            registration_id: reg_id,
            participant_name: reg.participant_name,
            active,
            bench,
        });
    }
    Ok(sides)
}
