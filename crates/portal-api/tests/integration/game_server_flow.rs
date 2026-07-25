//! MatchZy Phase 2/3 integration tests: allocation predicate, the event
//! webhook (auth, dedupe, full series pipeline), and connect-info gating.
//!
//! Reservations are seeded through the same repositories the flow uses
//! (via an AppState sharing the TestApp database); MatchZy's side of the
//! conversation is replayed as raw webhook POSTs — the §2.2 payload
//! quirks (scores, "3"/"2" sides) are baked into the fixtures.

use crate::common::TestApp;
use crate::tournaments::create_tournament_with_matches;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use portal_api::state::AppState;
use portal_core::ids::{GameServerId, ServerReservationId, TournamentMatchId};
use portal_core::types::{AgentGamestate, ReservationStatus};
use portal_domain::entities::HeartbeatUpdate;
use portal_domain::repositories::{
    CreateGameServer, CreateServerBooking, CreateServerReservation, ServerReservationRepository,
};
use portal_domain::services::game_server::hash_token;
use portal_test::prelude::*;
use serde_json::json;

async fn state_for(app: &TestApp) -> AppState {
    AppState::new(app.pool().clone(), "test-jwt-secret").await
}

/// Register a server and give it a fresh idle heartbeat so the §6.7
/// allocation predicate accepts it.
async fn seed_available_server(state: &AppState, game_id: &str, name: &str) -> GameServerId {
    let server = state
        .game_server_registry
        .register(CreateGameServer {
            id: GameServerId::new(),
            name: name.to_string(),
            game_id: game_id.parse().unwrap(),
            ip_address: "203.0.113.50".parse().unwrap(),
            port: 27015,
            gotv_port: Some(27020),
            region: "test".to_string(),
        })
        .await
        .unwrap();
    state
        .game_server_registry
        .record_heartbeat(
            server.id,
            HeartbeatUpdate {
                agent_version: "test".into(),
                rcon_ok: true,
                gamestate: Some(AgentGamestate::None),
                reported_matchzy_id: None,
            },
            false,
        )
        .await
        .unwrap();
    server.id
}

/// Create a pending reservation with a known event token, then allocate.
async fn seed_allocated_reservation(
    state: &AppState,
    match_id: &str,
    tournament_id: &str,
    game_id: &str,
) -> (portal_domain::entities::ServerReservation, String) {
    let event_token = "cgm_test_event_token".to_string();
    let match_id: TournamentMatchId = match_id.parse().unwrap();
    let reservation = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id,
            connect_password: "pw12345678".into(),
            gotv_password: Some("gotv123456".into()),
            config_token_hash: hash_token("cgm_test_config_token"),
            event_token_hash: hash_token(&event_token),
            config_token_expires_at: Utc::now() + Duration::minutes(15),
        })
        .await
        .unwrap();
    let (reservation, _server) = state
        .server_reservation_repo
        .allocate(
            reservation.id,
            game_id.parse().unwrap(),
            tournament_id.parse().unwrap(),
            None,
            Utc::now() - Duration::seconds(90),
            Utc::now(),
        )
        .await
        .unwrap()
        .expect("allocation succeeds with a fresh idle server");
    (reservation, event_token)
}

/// Drive the match to `pick_ban` via admin transitions (the state veto
/// completion would leave it in).
async fn match_to_pick_ban(app: &TestApp, tournament_id: &str, match_id: &str) {
    for to in ["scheduled", "pick_ban"] {
        let response = app
            .post_json(
                &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
                &json!({ "to_status": to, "override_reason": "test setup" }),
            )
            .await;
        response.assert_status(StatusCode::OK);
    }
}

fn event_headers(token: &str) -> (&'static str, String) {
    ("authorization", format!("Bearer {token}"))
}

async fn post_event(
    app: &TestApp,
    token: &str,
    payload: &serde_json::Value,
) -> axum::http::StatusCode {
    let (name, value) = event_headers(token);
    let response = app
        .post_json_with_header("/v1/gameserver/events", payload, name, &value)
        .await;
    response.status
}

#[tokio::test]
async fn test_allocation_predicate_and_exclusivity() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "alloc-pred").await;
    let game_id = get_game_id_str(&app).await;

    // No server at all → allocation returns None, reservation stays pending.
    let reservation = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id: match_id.parse().unwrap(),
            connect_password: "pw".into(),
            gotv_password: None,
            config_token_hash: hash_token("a"),
            event_token_hash: hash_token("b"),
            config_token_expires_at: Utc::now(),
        })
        .await
        .unwrap();
    let allocated = state
        .server_reservation_repo
        .allocate(
            reservation.id,
            game_id.parse().unwrap(),
            tournament_id.parse().unwrap(),
            None,
            Utc::now() - Duration::seconds(90),
            Utc::now(),
        )
        .await
        .unwrap();
    assert!(allocated.is_none(), "no server should match");

    // Fresh idle server → allocation succeeds and marks it reserved.
    let server_id = seed_available_server(&state, &game_id, "Alloc box").await;
    let (reservation, server) = state
        .server_reservation_repo
        .allocate(
            reservation.id,
            game_id.parse().unwrap(),
            tournament_id.parse().unwrap(),
            None,
            Utc::now() - Duration::seconds(90),
            Utc::now(),
        )
        .await
        .unwrap()
        .expect("fresh idle server allocates");
    assert_eq!(server.id, server_id);
    assert_eq!(reservation.server_id, Some(server_id));

    // The server is now taken: a second match cannot allocate it.
    let (_t2, match2_id, _r3, _r4) = create_tournament_with_matches(&app, "alloc-pred-b").await;
    let reservation2 = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id: match2_id.parse().unwrap(),
            connect_password: "pw".into(),
            gotv_password: None,
            config_token_hash: hash_token("c"),
            event_token_hash: hash_token("d"),
            config_token_expires_at: Utc::now(),
        })
        .await
        .unwrap();
    let allocated2 = state
        .server_reservation_repo
        .allocate(
            reservation2.id,
            game_id.parse().unwrap(),
            tournament_id.parse().unwrap(),
            None,
            Utc::now() - Duration::seconds(90),
            Utc::now(),
        )
        .await
        .unwrap();
    assert!(allocated2.is_none(), "busy server must not double-book");
}

#[tokio::test]
async fn test_hard_hold_booking_blocks_allocation() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "alloc-booked").await;
    let game_id = get_game_id_str(&app).await;
    let server_id = seed_available_server(&state, &game_id, "Held box").await;

    // Hard hold (no tournament) covering now.
    let dev_user = "00000000-0000-0000-0000-000000000001".parse().unwrap();
    state
        .game_server_registry
        .create_booking(CreateServerBooking {
            id: portal_core::ids::ServerBookingId::new(),
            server_id,
            tournament_id: None,
            reason: Some("maintenance".into()),
            starts_at: Utc::now() - Duration::hours(1),
            ends_at: Utc::now() + Duration::hours(1),
            created_by: dev_user,
        })
        .await
        .unwrap();

    let reservation = state
        .server_reservation_repo
        .create_pending(CreateServerReservation {
            id: ServerReservationId::new(),
            match_id: match_id.parse().unwrap(),
            connect_password: "pw".into(),
            gotv_password: None,
            config_token_hash: hash_token("e"),
            event_token_hash: hash_token("f"),
            config_token_expires_at: Utc::now(),
        })
        .await
        .unwrap();
    let allocated = state
        .server_reservation_repo
        .allocate(
            reservation.id,
            game_id.parse().unwrap(),
            tournament_id.parse().unwrap(),
            None,
            Utc::now() - Duration::seconds(90),
            Utc::now(),
        )
        .await
        .unwrap();
    assert!(allocated.is_none(), "hard-held server must not allocate");
}

#[tokio::test]
async fn test_event_pipeline_full_series() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "series-flow").await;
    let _ = &reg1;
    let game_id = get_game_id_str(&app).await;
    seed_available_server(&state, &game_id, "Series box").await;
    match_to_pick_ban(&app, &tournament_id, &match_id).await;

    let (reservation, token) =
        seed_allocated_reservation(&state, &match_id, &tournament_id, &game_id).await;
    state
        .server_reservation_repo
        .set_status(reservation.id, ReservationStatus::Configuring)
        .await
        .unwrap();
    let mid = reservation.matchzy_id;

    // Wrong token → 401; unknown matchid → 404.
    assert_eq!(
        post_event(
            &app,
            "cgm_wrong",
            &json!({"event": "series_start", "matchid": mid})
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_event(
            &app,
            &token,
            &json!({"event": "series_start", "matchid": 999_999})
        )
        .await,
        StatusCode::NOT_FOUND
    );

    // series_start → ready.
    assert_eq!(
        post_event(
            &app,
            &token,
            &json!({"event": "series_start", "matchid": mid})
        )
        .await,
        StatusCode::OK
    );
    let r = state
        .server_reservation_repo
        .find_by_id(reservation.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.status, ReservationStatus::Ready);

    // going_live → live + match pick_ban → in_progress.
    assert_eq!(
        post_event(
            &app,
            &token,
            &json!({"event": "going_live", "matchid": mid, "map_number": 0})
        )
        .await,
        StatusCode::OK
    );
    let r = state
        .server_reservation_repo
        .find_by_id(reservation.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.status, ReservationStatus::Live);
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "in_progress");

    // round_end with §2.2-quirky payload (winner.side "3", leading team).
    let round_end = json!({
        "event": "round_end", "matchid": mid, "map_number": 0, "round_number": 5,
        "reason": 8, "round_time": 0,
        "winner": {"side": "3", "team": "team1"},
        "team1": {"id": reg1, "name": "Player1", "series_score": 0, "score": 3},
        "team2": {"id": "", "name": "Player2", "series_score": 0, "score": 2},
    });
    assert_eq!(post_event(&app, &token, &round_end).await, StatusCode::OK);
    // Replay is a dedupe no-op, still 200.
    assert_eq!(post_event(&app, &token, &round_end).await, StatusCode::OK);

    // map_result + series_end (2-0 for participant 1 in a bo3).
    for map_number in [0, 1] {
        assert_eq!(
            post_event(
                &app,
                &token,
                &json!({
                    "event": "map_result", "matchid": mid, "map_number": map_number,
                    "winner": {"side": "2", "team": "team1"},
                    "team1": {"id": reg1, "name": "Player1", "series_score": map_number, "score": 13},
                    "team2": {"id": "", "name": "Player2", "series_score": 0, "score": 7},
                })
            )
            .await,
            StatusCode::OK
        );
    }
    assert_eq!(
        post_event(
            &app,
            &token,
            &json!({
                "event": "series_end", "matchid": mid,
                "winner": {"side": "2", "team": "team1"},
                "team1_series_score": 2, "team2_series_score": 0,
                "time_until_restore": 10,
            })
        )
        .await,
        StatusCode::OK
    );

    // Reservation completed + server released back to available.
    let r = state
        .server_reservation_repo
        .find_by_id(reservation.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.status, ReservationStatus::Completed);
    let server = state
        .game_server_registry
        .get(r.server_id.unwrap())
        .await
        .unwrap();
    assert_eq!(
        server.status,
        portal_core::types::GameServerStatus::Available
    );
    assert!(server.current_match_id.is_none());

    // A server-sourced claim exists with the §2.2-derived winner: MatchZy
    // team1 maps to the MATCH's participant1 (seeding is random, so resolve
    // it from the match rather than assuming reg1).
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await;
    let body: serde_json::Value = response.json();
    let participant1 = body["data"]["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/result"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["source"], "server");
    assert_eq!(body["data"]["claimed_winner_registration_id"], participant1);
    assert_eq!(body["data"]["claimed_participant1_score"], 2);
    assert_eq!(body["data"]["claimed_participant2_score"], 0);
    assert!(body["data"]["submitted_by_registration_id"].is_null());

    // Late events after completion are acknowledged and ignored.
    assert_eq!(
        post_event(
            &app,
            &token,
            &json!({"event": "round_end", "matchid": mid, "map_number": 1, "round_number": 30})
        )
        .await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_connect_info_gating() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _r1, _r2) =
        create_tournament_with_matches(&app, "connect-gate").await;
    let game_id = get_game_id_str(&app).await;
    seed_available_server(&state, &game_id, "Gate box").await;

    // No reservation yet → 404.
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/server"))
        .await;
    response.assert_status(StatusCode::NOT_FOUND);

    let (reservation, _token) =
        seed_allocated_reservation(&state, &match_id, &tournament_id, &game_id).await;

    // Reserved-but-not-ready: no connect info even for admins.
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/server"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "pending");
    assert!(body["data"]["connect_password"].is_null());

    // Ready: the admin (dev-token) view carries the password.
    state
        .server_reservation_repo
        .set_status(reservation.id, ReservationStatus::Ready)
        .await
        .unwrap();
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/server"))
        .await;
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "ready");
    assert_eq!(body["data"]["connect_password"], "pw12345678");
    assert_eq!(body["data"]["ip_address"], "203.0.113.50");
    assert_eq!(body["data"]["is_participant"], true);

    // Unauthenticated → 401.
    let response = app.get(&format!("/v1/matches/{match_id}/server")).await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

/// The `games` seed for TestApp registers cs2; resolve its UUID.
async fn get_game_id_str(app: &TestApp) -> String {
    portal_test::helpers::get_game_id(app.pool(), "cs2")
        .await
        .to_string()
}
