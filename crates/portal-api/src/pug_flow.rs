//! PUG orchestration: everything that spans the PUG aggregate and the
//! tournament/veto/game-server subsystems, mirroring `game_server_flow`.
//!
//! A PUG lobby lives in the `pugs` tables until it locks. Locking
//! **materializes** it as a hidden single-match container tournament
//! (`tournaments.kind = 'pug'`) with two ad-hoc teams, after which the
//! standard pipeline (veto session → WS lobby → server reservation →
//! MatchZy → results → demos) runs unmodified. This module owns:
//!
//! - the materializer (`lock_pug`)
//! - the wheel spin (`spin_wheel`) — weighted draw recorded as a `random`
//!   veto action + a deterministic `wheel_spin` WS broadcast
//! - cancellation (`cancel_pug`)
//! - status hooks driven by veto completion and server events
//! - demo tagging (`tag_demo_if_pug`) so PUG demos land in the `pug`
//!   category, keeping PUG stats out of tournament profiles

use chrono::Utc;
use serde_json::json;

use portal_core::types::{
    BracketType, PugMapSelectionMode, PugStatus, RegistrationType, SchedulingMode, StageFormat,
    TournamentKind, TournamentMatchStatus, TournamentRegistrationStatus, WithdrawalPolicy,
};
use portal_core::{DemoId, DomainError, PlayerId, PugId, TournamentMatchId, UserId};
use portal_domain::entities::pug::Pug;
use portal_domain::entities::{TransitionTrigger, VetoActionResult};
use portal_domain::repositories::pug::CreateWheelSpin;
#[allow(unused_imports)]
use portal_domain::repositories::pug::{AdhocTeamRepository as _, PugRepository as _};
use portal_domain::repositories::tournament::{
    CreateTournament, CreateTournamentBracket, CreateTournamentMatch,
    CreateTournamentRegistration, CreateTournamentStage, TournamentBracketRepository as _,
    TournamentMapPoolRepository as _, TournamentMatchRepository as _,
    TournamentRegistrationRepository as _, TournamentRepository as _,
    TournamentStageRepository as _, UpsertTournamentMapPool,
};
use portal_domain::services::{LockPlan, PugService, WheelDraw};

use crate::dto::responses::veto::{VetoActionResponse, VetoSessionResponse};
use crate::state::AppState;
use crate::websocket::messages::{
    LobbyBroadcast, VetoActionBroadcast, VetoCompleteBroadcast, WheelSpinBroadcast,
};

/// Suggested wheel animation length. Purely presentational; clients may
/// clamp it, but every client uses the same value for the same spin.
pub const WHEEL_SPIN_DURATION_MS: u32 = 5_500;

/// Materialized pugs that never produced a live server within this window
/// are cancelled by the sweeper.
pub const STALLED_MATERIALIZED_MINUTES: i64 = 60;

// =============================================================================
// LOCK (materialize)
// =============================================================================

/// Lock the lobby: validate, then materialize the container tournament and
/// start map selection. Returns the refreshed pug.
pub async fn lock_pug(
    state: &AppState,
    pug_id: PugId,
    actor_user: UserId,
    actor_player: PlayerId,
    force: bool,
) -> Result<Pug, DomainError> {
    let plan = state
        .pug_service
        .prepare_lock(pug_id, actor_user, actor_player, force)
        .await?;

    // Resolve the final veto pool. Veto mode: custom pool or the game's
    // default (standard formats need min_map_pool maps). Wheel mode: the
    // deduped nominations, padded from the game default pool when short —
    // a bo3 wheel needs at least 3 distinct maps to land on.
    let game_row = state
        .game_repo
        .find_by_id(plan.pug.game_id.as_uuid())
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::LookupFailed {
            resource: "game",
            query: format!("id {}", plan.pug.game_id),
        })?;
    let default_pool = crate::handlers::games::extract_map_pool(&game_row);

    let map_pool = match plan.pug.map_selection_mode {
        PugMapSelectionMode::Veto => {
            let pool = if plan.map_pool_hint.is_empty() {
                default_pool
            } else {
                plan.map_pool_hint.clone()
            };
            let format = state
                .veto_service
                .resolve_format(&plan.veto_format_id)
                .map_err(|e| DomainError::InvalidState(format!("veto format: {e}")))?;
            if pool.len() < format.min_map_pool {
                return Err(DomainError::InvalidState(format!(
                    "The veto needs at least {} maps ({} available)",
                    format.min_map_pool,
                    pool.len()
                )));
            }
            pool
        }
        PugMapSelectionMode::Wheel => {
            let needed = usize::try_from(plan.pug.match_format.game_count()).unwrap_or(1);
            let mut pool = plan.map_pool_hint.clone();
            for map in default_pool {
                if pool.len() >= needed {
                    break;
                }
                if !pool.contains(&map) {
                    pool.push(map);
                }
            }
            if pool.len() < needed {
                return Err(DomainError::InvalidState(format!(
                    "The wheel needs at least {needed} distinct maps ({} available)",
                    pool.len()
                )));
            }
            pool
        }
    };

    let (tournament_id, match_id) = materialize(state, &plan, &map_pool).await?;

    state
        .pug_service
        .repo()
        .set_materialized(pug_id, tournament_id, match_id)
        .await?;

    tracing::info!(pug_id = %pug_id, %tournament_id, %match_id, "PUG materialized");
    state.pug_service.get(pug_id).await
}

/// Create the hidden container tournament + everything the match pipeline
/// needs. Not transactional across repos (mirrors existing multi-repo
/// flows); a mid-way failure leaves invisible kind='pug' rows behind and
/// the pug still in `gathering` — retrying lock is safe because
/// materialization only marks the pug after full success.
async fn materialize(
    state: &AppState,
    plan: &LockPlan,
    map_pool: &[String],
) -> Result<(portal_core::TournamentId, TournamentMatchId), DomainError> {
    let pug = &plan.pug;
    let short = &pug.id.to_string()[..8];

    let team_name = |players: &[portal_domain::entities::PugPlayer], fallback: &str| {
        players
            .iter()
            .find(|p| p.is_captain)
            .or_else(|| players.first())
            .map_or_else(|| fallback.to_string(), |p| format!("team_{}", p.display_name))
    };
    let team1_name = team_name(&plan.team1, "Team 1");
    let team2_name = team_name(&plan.team2, "Team 2");

    let settings = json!({
        "game_server": {
            "enabled": true,
            "region": pug.region,
            "players_per_team": pug.team_size,
        },
        "side_selection_mode": pug.side_selection_mode.to_string(),
    });

    let tournament = state
        .tournament_repo
        .create(CreateTournament {
            game_id: pug.game_id,
            league_id: None,
            season_id: None,
            name: format!("PUG #{short}"),
            slug: format!("pug-{}", pug.id),
            description: None,
            format: portal_core::types::TournamentFormat::SingleElimination,
            format_settings: json!({}),
            participant_type: portal_core::types::TournamentParticipantType::AdHoc,
            kind: TournamentKind::Pug,
            team_size: Some(pug.team_size),
            min_participants: 2,
            max_participants: 2,
            registration_type: RegistrationType::InviteOnly,
            registration_start: None,
            registration_end: None,
            check_in_required: false,
            check_in_start: None,
            check_in_end: None,
            scheduling_mode: SchedulingMode::Live,
            starts_at: Some(Utc::now()),
            default_match_format: pug.match_format,
            // Deliberately None: the check-in lifecycle job must NOT create
            // a veto session — this flow creates it explicitly below.
            default_map_veto_format: None,
            withdrawal_policy: WithdrawalPolicy::Forfeit,
            rules_url: None,
            settings,
            created_by: pug.created_by_user_id,
        })
        .await?;

    let stage = state
        .tournament_stage_repo
        .create(CreateTournamentStage {
            tournament_id: tournament.id,
            name: "Match".to_string(),
            stage_order: 1,
            format: StageFormat::SingleElimination,
            format_settings: json!({}),
            advancement_count: None,
            advancement_rule: portal_core::types::AdvancementRule::default(),
            match_format: Some(pug.match_format),
            map_veto_format: None,
            starts_at: None,
            ends_at: None,
        })
        .await?;

    let bracket = state
        .tournament_bracket_repo
        .create(CreateTournamentBracket {
            stage_id: stage.id,
            tournament_id: tournament.id,
            name: "PUG".to_string(),
            bracket_type: BracketType::SingleElim,
            total_rounds: 1,
            group_number: None,
        })
        .await?;

    let adhoc_members =
        |players: &[portal_domain::entities::PugPlayer]| -> Vec<(PlayerId, bool)> {
            players.iter().map(|p| (p.player_id, p.is_captain)).collect()
        };
    let team1 = state
        .adhoc_team_repo
        .create(tournament.id, &team1_name, &adhoc_members(&plan.team1))
        .await?;
    let team2 = state
        .adhoc_team_repo
        .create(tournament.id, &team2_name, &adhoc_members(&plan.team2))
        .await?;

    let make_registration = |adhoc_id: portal_core::AdhocTeamId, name: &str| {
        CreateTournamentRegistration {
            tournament_id: tournament.id,
            team_season_id: None,
            player_id: None,
            adhoc_team_id: Some(adhoc_id.as_uuid()),
            participant_name: name.to_string(),
            participant_logo_url: None,
            registered_by: pug.created_by_user_id,
            seed_rating: None,
            status: TournamentRegistrationStatus::Approved,
        }
    };
    let reg1 = state
        .tournament_registration_repo
        .create(make_registration(team1.id, &team1_name))
        .await?;
    let reg2 = state
        .tournament_registration_repo
        .create(make_registration(team2.id, &team2_name))
        .await?;

    state
        .tournament_map_pool_repo
        .upsert(UpsertTournamentMapPool {
            tournament_id: tournament.id,
            stage_id: None,
            maps: map_pool.to_vec(),
            veto_format_id: Some(plan.veto_format_id.clone()),
        })
        .await?;

    let match_ = state
        .tournament_match_repo
        .create(CreateTournamentMatch {
            bracket_id: bracket.id,
            stage_id: stage.id,
            tournament_id: tournament.id,
            round: 1,
            match_number: 1,
            bracket_position: "PUG".to_string(),
            participant1_registration_id: Some(reg1.id),
            participant2_registration_id: Some(reg2.id),
            participant1_name: Some(team1_name.clone()),
            participant1_logo_url: None,
            participant1_seed: None,
            participant2_name: Some(team2_name.clone()),
            participant2_logo_url: None,
            participant2_seed: None,
            participant1_source: None,
            participant2_source: None,
            match_format: pug.match_format,
            maps_required: pug.match_format.game_count(),
            winner_progresses_to: None,
            loser_progresses_to: None,
        })
        .await?;

    // Walk the audited lifecycle to pick_ban.
    let system = || TransitionTrigger::System {
        job_name: "pug".to_string(),
    };
    for status in [
        TournamentMatchStatus::Ready,
        TournamentMatchStatus::Scheduled,
    ] {
        state
            .match_lifecycle_service
            .transition(match_.id, status, system(), Some("PUG locked".to_string()))
            .await?;
    }

    // Veto session: standard formats coin-flip a starter; wheel formats go
    // straight to in_progress awaiting the first spin (no team actions).
    let format = state
        .veto_service
        .resolve_format(&plan.veto_format_id)
        .map_err(|e| DomainError::InvalidState(format!("veto format: {e}")))?;
    let session = state
        .veto_service
        .create_session(
            match_.id,
            &format,
            map_pool.to_vec(),
            None,
            pug.side_selection_mode,
        )
        .await?;
    state.veto_service.start_session(session.id).await?;

    if format.has_team_actions() {
        let winner = if rand::random::<bool>() { reg1.id } else { reg2.id };
        state
            .veto_service
            .record_coin_flip(session.id, winner, true)
            .await?;
    }

    state
        .match_lifecycle_service
        .transition(
            match_.id,
            TournamentMatchStatus::PickBan,
            system(),
            Some("PUG map selection".to_string()),
        )
        .await?;

    Ok((tournament.id, match_.id))
}

// =============================================================================
// WHEEL SPIN
// =============================================================================

/// Outcome returned to the spin endpoint.
pub struct SpinOutcome {
    pub draw: WheelDraw,
    pub game_number: i32,
    pub result: VetoActionResult,
}

/// Spin the wheel: weighted draw over remaining nominated maps, recorded as
/// the session's current `random` action, broadcast to every lobby client.
pub async fn spin_wheel(
    state: &AppState,
    pug_id: PugId,
    actor_user: UserId,
    actor_player: PlayerId,
) -> Result<SpinOutcome, DomainError> {
    let pug = state.pug_service.get(pug_id).await?;

    if pug.map_selection_mode != PugMapSelectionMode::Wheel {
        return Err(DomainError::InvalidState(
            "This PUG uses map veto, not the wheel".to_string(),
        ));
    }
    let Some(match_id) = pug.match_id else {
        return Err(DomainError::InvalidState(
            "Lock the lobby before spinning".to_string(),
        ));
    };

    // Creator or any captain spins.
    let players = state.pug_service.players(pug_id).await?;
    let is_captain = players
        .iter()
        .any(|p| p.player_id == actor_player && p.is_captain);
    if pug.created_by_user_id != actor_user && !is_captain {
        return Err(DomainError::NotAuthorized(
            "Only the creator or a captain can spin the wheel".to_string(),
        ));
    }

    let veto = state.veto_service.get_session_state(match_id).await?;
    let game_number = i32::try_from(veto.session.selected_maps.len() + 1).unwrap_or(i32::MAX);

    // Weighted segments over the maps still in play.
    let entries = state.pug_service.wheel_entries(pug_id).await?;
    let segments = PugService::build_segments(&entries, &veto.session.remaining_maps);
    let draw = PugService::draw(segments)?;
    let segments_json = serde_json::to_value(&draw.segments)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Audit row first (idempotent on (pug, game_number)); the veto action is
    // the authoritative state change.
    state
        .pug_service
        .repo()
        .record_spin(CreateWheelSpin {
            pug_id,
            game_number,
            entries: segments_json.clone(),
            winner_map_id: draw.winner_map_id.clone(),
            spin_seed: draw.spin_seed,
            spun_by_player_id: Some(actor_player),
        })
        .await?;

    let result = state
        .veto_service
        .perform_wheel_action(veto.session.id, &draw.winner_map_id)
        .await?;

    // Broadcast: the animation payload first, then the standard state
    // frames (clients queue state behind the animation locally).
    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
        lobby.broadcast(LobbyBroadcast::WheelSpin(WheelSpinBroadcast {
            game_number,
            segments: segments_json,
            winner_map_id: draw.winner_map_id.clone(),
            spin_seed: draw.spin_seed,
            duration_ms: WHEEL_SPIN_DURATION_MS,
        }));
        if result.veto_complete {
            lobby.broadcast(LobbyBroadcast::VetoComplete(VetoCompleteBroadcast {
                session: VetoSessionResponse::from(result.session.clone()),
                selected_maps: result.session.selected_maps.clone(),
            }));
        } else {
            lobby.broadcast(LobbyBroadcast::VetoActionPerformed(Box::new(
                VetoActionBroadcast {
                    session: VetoSessionResponse::from(result.session.clone()),
                    action: VetoActionResponse::from(result.action.clone()),
                    is_complete: false,
                },
            )));
        }
    }

    if result.veto_complete {
        let _ = state.server_assignment_tx.send(match_id);
    }

    Ok(SpinOutcome {
        draw,
        game_number,
        result,
    })
}

// =============================================================================
// CANCEL
// =============================================================================

/// Cancel a PUG. Pre-lock this just kills the lobby; post-lock it also
/// cancels the match and releases any server reservation.
pub async fn cancel_pug(
    state: &AppState,
    pug_id: PugId,
    actor_user: UserId,
    is_admin: bool,
) -> Result<(), DomainError> {
    let pug = state.pug_service.get(pug_id).await?;
    if pug.created_by_user_id != actor_user && !is_admin {
        return Err(DomainError::NotAuthorized(
            "Only the PUG creator can cancel it".to_string(),
        ));
    }
    if pug.status.is_terminal() {
        return Ok(());
    }
    cancel_materialized_side(state, &pug, "PUG cancelled by creator").await;
    state
        .pug_service
        .repo()
        .set_status(pug_id, PugStatus::Cancelled)
        .await?;
    Ok(())
}

/// Best-effort teardown of the match/reservation half of a cancelled pug.
async fn cancel_materialized_side(state: &AppState, pug: &Pug, reason: &str) {
    let Some(match_id) = pug.match_id else { return };

    if let Err(e) = crate::game_server_flow::cancel_assignment(state, match_id, reason).await {
        tracing::debug!(pug_id = %pug.id, %match_id, error = %e,
            "no reservation to cancel for pug");
    }
    let transition = state
        .match_lifecycle_service
        .transition(
            match_id,
            TournamentMatchStatus::Cancelled,
            TransitionTrigger::System {
                job_name: "pug".to_string(),
            },
            Some(reason.to_string()),
        )
        .await;
    if let Err(e) = transition {
        tracing::warn!(pug_id = %pug.id, %match_id, error = %e,
            "failed to cancel pug match");
    }
}

// =============================================================================
// STATUS HOOKS (called by veto completion / server events / sweeper)
// =============================================================================

/// Veto or wheel finished → the pug is waiting on a server.
pub async fn on_veto_completed(state: &AppState, match_id: TournamentMatchId) {
    if let Ok(Some(pug)) = state.pug_service.repo().find_by_match(match_id).await {
        let _ = state
            .pug_service
            .repo()
            .transition_status(pug.id, PugStatus::MapSelection, PugStatus::AwaitingServer)
            .await;
    }
}

/// Server went live.
pub async fn on_match_live(state: &AppState, match_id: TournamentMatchId) {
    if let Ok(Some(pug)) = state.pug_service.repo().find_by_match(match_id).await
        && !pug.status.is_terminal()
    {
        let _ = state
            .pug_service
            .repo()
            .set_status(pug.id, PugStatus::Live)
            .await;
    }
}

/// Series ended → denormalize the result onto the pug row.
pub async fn on_series_end(
    state: &AppState,
    match_id: TournamentMatchId,
    team1_score: i32,
    team2_score: i32,
) {
    if let Ok(Some(pug)) = state.pug_service.repo().find_by_match(match_id).await {
        let winner_team: i16 = if team1_score > team2_score { 1 } else { 2 };
        if let Err(e) = state
            .pug_service
            .repo()
            .set_result(pug.id, winner_team, team1_score, team2_score)
            .await
        {
            tracing::warn!(pug_id = %pug.id, error = %e, "failed to record pug result");
        }
    }
}

/// Tag a demo `pug` when it belongs to a PUG container match, keeping PUG
/// demos out of the tournament stats surfaces.
pub async fn tag_demo_if_pug(state: &AppState, demo_id: DemoId, match_id: TournamentMatchId) {
    let owner = async {
        let match_ = state.tournament_match_repo.find_by_id(match_id).await?;
        let Some(match_) = match_ else {
            return Ok::<Option<UserId>, DomainError>(None);
        };
        let tournament = state
            .tournament_service
            .get_tournament(match_.tournament_id)
            .await?;
        Ok((tournament.kind == TournamentKind::Pug).then_some(tournament.created_by))
    }
    .await;

    match owner {
        Ok(Some(by_user)) => {
            if let Err(e) = state
                .demo_service
                .categorize_demo(demo_id, portal_core::DemoCategory::Pug, by_user)
                .await
            {
                tracing::warn!(%demo_id, error = %e, "failed to tag pug demo");
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::debug!(%demo_id, error = %e, "pug demo tag lookup failed");
        }
    }
}

// =============================================================================
// SWEEPER (called from the background lifecycle pass)
// =============================================================================

/// Expire stale gathering lobbies and cancel materialized pugs that never
/// went live. Also plugs the general "nothing releases a reservation when a
/// match dies pre-live" gap for pug matches.
pub async fn sweep_pugs(state: &AppState) {
    let now = Utc::now();

    match state
        .pug_service
        .repo()
        .list_expired_gathering(now, 50)
        .await
    {
        Ok(expired) => {
            for pug in expired {
                tracing::info!(pug_id = %pug.id, "expiring stale gathering pug");
                let _ = state
                    .pug_service
                    .repo()
                    .transition_status(pug.id, PugStatus::Gathering, PugStatus::Expired)
                    .await;
            }
        }
        Err(e) => tracing::warn!(error = %e, "pug expiry sweep failed"),
    }

    let cutoff = now - chrono::Duration::minutes(STALLED_MATERIALIZED_MINUTES);
    match state
        .pug_service
        .repo()
        .list_stalled_materialized(cutoff, 50)
        .await
    {
        Ok(stalled) => {
            for pug in stalled {
                tracing::info!(pug_id = %pug.id, status = %pug.status,
                    "cancelling stalled pug (never went live)");
                cancel_materialized_side(state, &pug, "PUG stalled — auto-cancelled").await;
                let _ = state
                    .pug_service
                    .repo()
                    .set_status(pug.id, PugStatus::Cancelled)
                    .await;
            }
        }
        Err(e) => tracing::warn!(error = %e, "pug stall sweep failed"),
    }
}
