//! WebSocket veto lobby integration tests.

use crate::common::TestApp;
use crate::common::ws::{
    ServerMessage, WsStream, connect_veto_ws, ws_authenticate, ws_ban_map, ws_next_message,
    ws_pick_map, ws_select_side, ws_send_chat,
};
use axum::http::StatusCode;
use portal_test::prelude::*;
use std::net::SocketAddr;
use uuid::Uuid;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Drain initial messages that arrive after connection (chat history, broadcasts).
async fn drain_initial_messages(ws: &mut WsStream) {
    // Try to receive messages with short timeout, stop when no more messages
    for _ in 0..5 {
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), ws_next_message(ws)).await;

        if result.is_err() {
            // Timeout - no more messages waiting
            break;
        }
    }
}

/// Wait for a veto-related response message, skipping broadcast messages.
async fn wait_for_veto_response(ws: &mut WsStream) -> Option<ServerMessage> {
    for _ in 0..10 {
        if let Some(msg) = ws_next_message(ws).await {
            match &msg {
                ServerMessage::VetoActionAck { .. }
                | ServerMessage::VetoActionPerformed { .. }
                | ServerMessage::Error { .. }
                | ServerMessage::VetoComplete { .. } => return Some(msg),
                // Skip other broadcast messages
                _ => continue,
            }
        }
        break;
    }
    None
}

// ============================================================================
// TEST SETUP
// ============================================================================

/// Setup data for WebSocket tests.
struct WsVetoTestSetup {
    match_id: Uuid,
    team_a_captain_token: String,
    team_a_owner_token: String,
    team_a_member_token: String,
    team_a_delegate_token: String,
    team_b_captain_token: String,
    spectator_token: String,
    admin_token: String,
}

/// Set up a complete veto test scenario with two teams and a match.
/// Uses TwoTeamMatchFixture for most setup, adds delegate separately.
async fn setup_ws_veto_scenario(app: &TestApp) -> WsVetoTestSetup {
    // Use TwoTeamMatchFixture for the base setup (with veto session)
    let fixture = TwoTeamMatchFixture::with_veto(app.pool(), TEST_JWT_SECRET).await;

    // Create delegate user and add to Team A
    let delegate_user = UserBuilder::new()
        .username("ws_veto_delegate")
        .build_persisted(app.pool())
        .await;

    // Add delegate as team member
    LeagueTeamMemberBuilder::new()
        .team_season_id(fixture.team_a.team_season_id)
        .player_id(delegate_user.id)
        .player() // Regular member who will be granted delegate rights
        .build_persisted(app.pool())
        .await;

    // Grant veto delegation rights
    VetoDelegateBuilder::new()
        .team_season_id(fixture.team_a.team_season_id)
        .player_id(delegate_user.id)
        .delegated_by_user_id(fixture.team_a.captain.user_id)
        .by_captain()
        .for_tournament(fixture.tournament_id)
        .build_persisted(app.pool())
        .await;

    let delegate_token = create_test_token(
        delegate_user.id,
        delegate_user.id,
        "ws_veto_delegate",
        TEST_JWT_SECRET,
    );

    WsVetoTestSetup {
        match_id: fixture.match_id,
        team_a_captain_token: fixture.team_a.captain.token.clone(),
        team_a_owner_token: fixture.team_a.owner.token.clone(),
        team_a_member_token: fixture.team_a.member.token.clone(),
        team_a_delegate_token: delegate_token,
        team_b_captain_token: fixture.team_b.captain.token.clone(),
        spectator_token: fixture.tokens.spectator.clone(),
        admin_token: fixture.tokens.admin.clone(),
    }
}

// ============================================================================
// E2E VETO SETUP AND HELPERS
// ============================================================================

/// Setup data for E2E veto flow tests.
struct E2eVetoSetup {
    match_id: Uuid,
    team_a_captain_token: String,
    team_b_captain_token: String,
}

/// Set up a veto scenario with a specific format (bo1/bo3/bo5).
async fn setup_veto_e2e_with_format(app: &TestApp, format: &str) -> E2eVetoSetup {
    setup_veto_e2e_with_format_and_mode(app, format, portal_core::SideSelectionMode::Knife).await
}

async fn setup_veto_e2e_with_format_and_mode(
    app: &TestApp,
    format: &str,
    mode: portal_core::SideSelectionMode,
) -> E2eVetoSetup {
    let fixture = TwoTeamMatchFixture::new(app.pool(), TEST_JWT_SECRET).await;

    let session_builder = match format {
        "bo1" => VetoSessionBuilder::new()
            .match_id_from_uuid(fixture.match_id)
            .bo1(),
        "bo3" => VetoSessionBuilder::new()
            .match_id_from_uuid(fixture.match_id)
            .bo3(),
        "bo5" => VetoSessionBuilder::new()
            .match_id_from_uuid(fixture.match_id)
            .bo5(),
        _ => panic!("Unknown format: {format}"),
    };
    let session_builder = session_builder.side_selection_mode(mode);

    let session = session_builder.build_persisted(app.pool()).await;

    // Update session to in_progress with Team A going first
    sqlx::query(
        r"UPDATE veto_sessions SET
            first_action_registration_id = $1,
            current_team_turn = $1,
            status = 'in_progress',
            current_action_number = 1,
            started_at = NOW()
         WHERE id = $2",
    )
    .bind(fixture.reg_a_id)
    .bind(session.id.as_uuid())
    .execute(app.pool())
    .await
    .expect("Failed to update veto session");

    E2eVetoSetup {
        match_id: fixture.match_id,
        team_a_captain_token: fixture.team_a.captain.token.clone(),
        team_b_captain_token: fixture.team_b.captain.token.clone(),
    }
}

/// Connect and authenticate both team captains, draining initial messages.
async fn connect_both_teams(
    addr: SocketAddr,
    match_id: &str,
    team_a_token: &str,
    team_b_token: &str,
) -> (WsStream, WsStream) {
    let mut ws_a = connect_veto_ws(addr, match_id).await;
    let auth_a = ws_authenticate(&mut ws_a, team_a_token).await;
    assert!(
        matches!(auth_a, ServerMessage::AuthSuccess { .. }),
        "Team A auth failed: {auth_a:?}"
    );
    drain_initial_messages(&mut ws_a).await;

    let mut ws_b = connect_veto_ws(addr, match_id).await;
    let auth_b = ws_authenticate(&mut ws_b, team_b_token).await;
    assert!(
        matches!(auth_b, ServerMessage::AuthSuccess { .. }),
        "Team B auth failed: {auth_b:?}"
    );
    drain_initial_messages(&mut ws_b).await;

    // Drain any connection broadcasts from Team B joining that Team A received
    drain_initial_messages(&mut ws_a).await;

    (ws_a, ws_b)
}

/// Perform a veto action (ban/pick) and drain responses from both connections.
/// Returns any VetoComplete message if the veto finished.
async fn do_veto_action(
    actor: &mut WsStream,
    observer: &mut WsStream,
    map: &str,
    is_pick: bool,
) -> Option<ServerMessage> {
    if is_pick {
        ws_pick_map(actor, map).await;
    } else {
        ws_ban_map(actor, map).await;
    }

    let mut veto_complete = None;

    // Drain actor messages (ack + broadcast)
    for _ in 0..5 {
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            ws_next_message(actor),
        )
        .await;
        match result {
            Ok(Some(msg @ ServerMessage::VetoComplete { .. })) => {
                veto_complete = Some(msg);
            }
            Ok(Some(ServerMessage::VetoActionAck { success, .. })) => {
                assert!(success, "Veto action should succeed for map {map}");
            }
            Ok(Some(_)) => {} // VetoActionPerformed or other broadcast
            _ => break,
        }
    }

    // Drain observer messages (broadcast)
    for _ in 0..5 {
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            ws_next_message(observer),
        )
        .await;
        match result {
            Ok(Some(msg @ ServerMessage::VetoComplete { .. })) if veto_complete.is_none() => {
                veto_complete = Some(msg);
            }
            Ok(Some(_)) => {}
            _ => break,
        }
    }

    veto_complete
}

// ============================================================================
// CONNECTION AND AUTHENTICATION TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_connect_and_auth_as_captain() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    // Start the server
    let addr = app.start_server().await;

    // Connect to WebSocket
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    // Authenticate
    let response = ws_authenticate(&mut ws, &setup.team_a_captain_token).await;

    match response {
        ServerMessage::AuthSuccess {
            role,
            registration_id,
            team_name,
            ..
        } => {
            assert_eq!(role, "participant", "Captain should be a participant");
            assert!(registration_id.is_some(), "Should have registration ID");
            assert!(team_name.is_some(), "Should have team name");
        }
        other => panic!("Expected AuthSuccess, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_connect_and_auth_as_owner() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    let response = ws_authenticate(&mut ws, &setup.team_a_owner_token).await;

    match response {
        ServerMessage::AuthSuccess {
            role,
            registration_id,
            ..
        } => {
            assert_eq!(role, "participant", "Owner should be a participant");
            assert!(registration_id.is_some(), "Should have registration ID");
        }
        other => panic!("Expected AuthSuccess, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_connect_and_auth_as_delegate() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    let response = ws_authenticate(&mut ws, &setup.team_a_delegate_token).await;

    match response {
        ServerMessage::AuthSuccess {
            role,
            registration_id,
            ..
        } => {
            assert_eq!(role, "participant", "Delegate should be a participant");
            assert!(registration_id.is_some(), "Should have registration ID");
        }
        other => panic!("Expected AuthSuccess, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_connect_and_auth_as_regular_member() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    let response = ws_authenticate(&mut ws, &setup.team_a_member_token).await;

    match response {
        ServerMessage::AuthSuccess {
            role,
            registration_id,
            ..
        } => {
            // Regular member (not captain/owner/delegate) becomes spectator
            assert_eq!(role, "spectator", "Regular member should be a spectator");
            assert!(
                registration_id.is_none(),
                "Spectator should not have registration ID"
            );
        }
        other => panic!("Expected AuthSuccess, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_connect_and_auth_as_spectator() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    let response = ws_authenticate(&mut ws, &setup.spectator_token).await;

    match response {
        ServerMessage::AuthSuccess {
            role,
            registration_id,
            ..
        } => {
            assert_eq!(role, "spectator", "Spectator should be a spectator");
            assert!(
                registration_id.is_none(),
                "Spectator should not have registration ID"
            );
        }
        other => panic!("Expected AuthSuccess, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_connect_and_auth_as_admin() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    let response = ws_authenticate(&mut ws, &setup.admin_token).await;

    match response {
        ServerMessage::AuthSuccess { role, .. } => {
            assert_eq!(role, "admin", "Admin should have admin role");
        }
        other => panic!("Expected AuthSuccess, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_auth_invalid_token() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    let response = ws_authenticate(&mut ws, "invalid-token").await;

    match response {
        ServerMessage::AuthError { error } => {
            assert!(
                error.contains("Invalid token"),
                "Should indicate invalid token: {error}"
            );
        }
        other => panic!("Expected AuthError, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_connect_to_nonexistent_match() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;

    // Connect to a non-existent match
    let fake_match_id = Uuid::now_v7();
    let mut ws = connect_veto_ws(addr, &fake_match_id.to_string()).await;

    let response = ws_authenticate(&mut ws, &setup.team_a_captain_token).await;

    match response {
        ServerMessage::AuthError { error } => {
            assert!(
                error.contains("not found"),
                "Should indicate match not found: {error}"
            );
        }
        other => panic!("Expected AuthError, got: {other:?}"),
    }
}

// ============================================================================
// VETO ACTION TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_veto_action_as_participant() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    // Authenticate as captain (participant)
    let auth_response = ws_authenticate(&mut ws, &setup.team_a_captain_token).await;
    assert!(
        matches!(auth_response, ServerMessage::AuthSuccess { .. }),
        "Should authenticate successfully"
    );

    // Drain any initial messages (chat history, connection broadcasts)
    drain_initial_messages(&mut ws).await;

    // Perform a ban action
    ws_ban_map(&mut ws, "de_dust2").await;

    // Wait for acknowledgment or veto action performed
    let response = wait_for_veto_response(&mut ws).await;

    match response {
        Some(ServerMessage::VetoActionAck { success, .. }) => {
            assert!(success, "Veto action should succeed");
        }
        Some(ServerMessage::VetoActionPerformed { .. }) => {
            // This is also acceptable - broadcast received
        }
        other => panic!("Expected VetoActionAck or VetoActionPerformed, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_veto_action_as_spectator_denied() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    // Authenticate as spectator
    let auth_response = ws_authenticate(&mut ws, &setup.spectator_token).await;
    assert!(
        matches!(auth_response, ServerMessage::AuthSuccess { role, .. } if role == "spectator"),
        "Should authenticate as spectator"
    );

    // Drain initial messages
    drain_initial_messages(&mut ws).await;

    // Try to perform a ban action
    ws_ban_map(&mut ws, "de_dust2").await;

    // Should receive error
    let response = wait_for_veto_response(&mut ws).await;

    match response {
        Some(ServerMessage::Error { code, message }) => {
            assert_eq!(code, "not_authorized", "Should be not_authorized error");
            assert!(
                message.contains("participant"),
                "Should mention participants: {message}"
            );
        }
        other => panic!("Expected Error, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_veto_action_wrong_turn_denied() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;
    let mut ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;

    // Authenticate as Team B captain (but it's Team A's turn)
    let auth_response = ws_authenticate(&mut ws, &setup.team_b_captain_token).await;
    assert!(
        matches!(auth_response, ServerMessage::AuthSuccess { role, .. } if role == "participant"),
        "Should authenticate as participant"
    );

    // Drain initial messages
    drain_initial_messages(&mut ws).await;

    // Try to perform a ban action (should fail - not Team B's turn)
    ws_ban_map(&mut ws, "de_dust2").await;

    // Should receive error
    let response = wait_for_veto_response(&mut ws).await;

    match response {
        Some(ServerMessage::Error { code, message }) => {
            assert!(
                code == "not_your_turn" || code == "veto_error",
                "Should be turn-related error: {code} - {message}"
            );
        }
        other => panic!("Expected Error, got: {other:?}"),
    }
}

// ============================================================================
// PARTICIPANT CONNECTION BROADCAST TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_participant_connected_broadcast() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;

    let addr = app.start_server().await;

    // Connect first user as spectator to receive broadcasts
    let mut spectator_ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;
    let _ = ws_authenticate(&mut spectator_ws, &setup.spectator_token).await;

    // Drain initial messages for spectator
    drain_initial_messages(&mut spectator_ws).await;

    // Connect second user as participant
    let mut participant_ws = connect_veto_ws(addr, &setup.match_id.to_string()).await;
    let _ = ws_authenticate(&mut participant_ws, &setup.team_a_captain_token).await;

    // Spectator should receive player connected broadcast
    // Try multiple messages in case there are other broadcasts first
    let mut found_player_connected = false;
    for _ in 0..5 {
        if let Some(msg) = ws_next_message(&mut spectator_ws).await
            && let ServerMessage::PlayerConnected {
                registration_id,
                team_name,
                username,
            } = msg
        {
            assert!(!registration_id.is_empty(), "Should have registration ID");
            assert!(
                team_name.contains("Alpha"),
                "Should be Team Alpha: {team_name}"
            );
            assert!(
                username.contains("captain"),
                "Should be captain: {username}"
            );
            found_player_connected = true;
            break;
        }
    }

    assert!(
        found_player_connected,
        "Should have received PlayerConnected broadcast"
    );

    // Clean up participant connection
    drop(participant_ws);
}

// ============================================================================
// E2E VETO FLOW TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_full_bo1_veto_flow() {
    let mut app = TestApp::new().await;
    let setup = setup_veto_e2e_with_format(&app, "bo1").await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let (mut ws_a, mut ws_b) = connect_both_teams(
        addr,
        &match_id_str,
        &setup.team_a_captain_token,
        &setup.team_b_captain_token,
    )
    .await;

    // Bo1: 6 alternating bans (the decider is auto-resolved as the remaining map)
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_dust2", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_b, &mut ws_a, "de_nuke", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_mirage", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_b, &mut ws_a, "de_inferno", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_ancient", false)
            .await
            .is_none()
    );

    // 6th ban should trigger VetoComplete
    let complete = do_veto_action(&mut ws_b, &mut ws_a, "de_anubis", false).await;
    assert!(
        complete.is_some(),
        "Should receive VetoComplete after final ban"
    );

    if let Some(ServerMessage::VetoComplete { session, .. }) = complete {
        assert_eq!(session["status"].as_str().unwrap(), "completed");
        // The remaining map (de_vertigo) is the decider
        let remaining = session["remaining_maps"]
            .as_array()
            .expect("Session should have remaining_maps");
        assert_eq!(
            remaining.len(),
            1,
            "Should have exactly 1 remaining map (decider)"
        );
        assert_eq!(remaining[0].as_str().unwrap(), "de_vertigo");
    }

    // Verify via REST
    let response = app
        .get(&format!("/v1/matches/{}/veto", setup.match_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["session"]["status"].as_str().unwrap(),
        "completed"
    );
}

#[tokio::test]
async fn test_ws_full_bo3_veto_flow() {
    let mut app = TestApp::new().await;
    let setup = setup_veto_e2e_with_format(&app, "bo3").await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let (mut ws_a, mut ws_b) = connect_both_teams(
        addr,
        &match_id_str,
        &setup.team_a_captain_token,
        &setup.team_b_captain_token,
    )
    .await;

    // Bo3: Ban-Ban-Pick-Pick-Ban-Ban (decider auto-resolved)
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_dust2", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_b, &mut ws_a, "de_nuke", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_mirage", true)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_b, &mut ws_a, "de_inferno", true)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_ancient", false)
            .await
            .is_none()
    );

    // 6th action (last ban) should trigger VetoComplete
    let complete = do_veto_action(&mut ws_b, &mut ws_a, "de_anubis", false).await;
    assert!(
        complete.is_some(),
        "Should receive VetoComplete after final ban"
    );

    if let Some(ServerMessage::VetoComplete {
        selected_maps,
        session,
    }) = complete
    {
        assert_eq!(session["status"].as_str().unwrap(), "completed");
        // 2 picks in selected_maps
        assert_eq!(selected_maps.len(), 2, "Bo3 should have 2 picked maps");
        assert!(selected_maps.contains(&"de_mirage".to_string()));
        assert!(selected_maps.contains(&"de_inferno".to_string()));
        // The decider is the remaining map
        let remaining = session["remaining_maps"]
            .as_array()
            .expect("Session should have remaining_maps");
        assert_eq!(remaining.len(), 1, "Should have 1 remaining map (decider)");
        assert_eq!(remaining[0].as_str().unwrap(), "de_vertigo");
    }

    // Verify via REST
    let response = app
        .get(&format!("/v1/matches/{}/veto", setup.match_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["session"]["status"].as_str().unwrap(),
        "completed"
    );
}

#[tokio::test]
async fn test_ws_full_bo5_veto_flow() {
    let mut app = TestApp::new().await;
    let setup = setup_veto_e2e_with_format(&app, "bo5").await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let (mut ws_a, mut ws_b) = connect_both_teams(
        addr,
        &match_id_str,
        &setup.team_a_captain_token,
        &setup.team_b_captain_token,
    )
    .await;

    // Bo5: Ban-Ban-Pick-Pick-Pick-Pick (decider auto-resolved)
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_dust2", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_b, &mut ws_a, "de_nuke", false)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_mirage", true)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_b, &mut ws_a, "de_inferno", true)
            .await
            .is_none()
    );
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_ancient", true)
            .await
            .is_none()
    );

    // 6th action (last pick) should trigger VetoComplete
    let complete = do_veto_action(&mut ws_b, &mut ws_a, "de_anubis", true).await;
    assert!(
        complete.is_some(),
        "Should receive VetoComplete after final pick"
    );

    if let Some(ServerMessage::VetoComplete {
        selected_maps,
        session,
    }) = complete
    {
        assert_eq!(session["status"].as_str().unwrap(), "completed");
        // 4 picks in selected_maps
        assert_eq!(selected_maps.len(), 4, "Bo5 should have 4 picked maps");
        assert!(selected_maps.contains(&"de_mirage".to_string()));
        assert!(selected_maps.contains(&"de_inferno".to_string()));
        assert!(selected_maps.contains(&"de_ancient".to_string()));
        assert!(selected_maps.contains(&"de_anubis".to_string()));
        // The decider is the remaining map
        let remaining = session["remaining_maps"]
            .as_array()
            .expect("Session should have remaining_maps");
        assert_eq!(remaining.len(), 1, "Should have 1 remaining map (decider)");
        assert_eq!(remaining[0].as_str().unwrap(), "de_vertigo");
    }

    // Verify via REST
    let response = app
        .get(&format!("/v1/matches/{}/veto", setup.match_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["session"]["status"].as_str().unwrap(),
        "completed"
    );
}

// ============================================================================
// CHAT VISIBILITY TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_team_chat_only_visible_to_same_team() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    // Connect Team A captain
    let mut ws_a = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a, &setup.team_a_captain_token).await;
    drain_initial_messages(&mut ws_a).await;

    // Connect Team B captain
    let mut ws_b = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_b, &setup.team_b_captain_token).await;
    drain_initial_messages(&mut ws_b).await;

    // Connect spectator
    let mut ws_spec = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_spec, &setup.spectator_token).await;
    drain_initial_messages(&mut ws_spec).await;

    // Drain connection broadcasts
    drain_initial_messages(&mut ws_a).await;
    drain_initial_messages(&mut ws_b).await;

    // Team A sends team chat
    ws_send_chat(&mut ws_a, "team", "secret plan").await;

    // Team A captain should receive the team chat (own team)
    let msg_a = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ws_next_message(&mut ws_a),
    )
    .await;
    match msg_a {
        Ok(Some(ServerMessage::Chat {
            chat_type, content, ..
        })) => {
            assert_eq!(chat_type, "team");
            assert_eq!(content, "secret plan");
        }
        other => panic!("Team A should receive own team chat, got: {other:?}"),
    }

    // Team B captain should NOT receive team chat
    let msg_b = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        ws_next_message(&mut ws_b),
    )
    .await;
    match msg_b {
        Ok(Some(ServerMessage::Chat { chat_type, .. })) if chat_type == "team" => {
            panic!("Team B should NOT receive Team A's team chat");
        }
        _ => {} // Timeout or non-chat message — expected
    }

    // Spectator should NOT receive team chat
    let msg_spec = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        ws_next_message(&mut ws_spec),
    )
    .await;
    match msg_spec {
        Ok(Some(ServerMessage::Chat { chat_type, .. })) if chat_type == "team" => {
            panic!("Spectator should NOT receive team chat");
        }
        _ => {} // Timeout or non-chat message — expected
    }
}

#[tokio::test]
async fn test_ws_all_chat_visible_to_everyone() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    // Connect all three
    let mut ws_a = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a, &setup.team_a_captain_token).await;
    drain_initial_messages(&mut ws_a).await;

    let mut ws_b = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_b, &setup.team_b_captain_token).await;
    drain_initial_messages(&mut ws_b).await;

    let mut ws_spec = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_spec, &setup.spectator_token).await;
    drain_initial_messages(&mut ws_spec).await;

    // Drain connection broadcasts
    drain_initial_messages(&mut ws_a).await;
    drain_initial_messages(&mut ws_b).await;

    // Team A sends all chat
    ws_send_chat(&mut ws_a, "all", "gg").await;

    // Team B should receive it
    let msg_b = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ws_next_message(&mut ws_b),
    )
    .await;
    match msg_b {
        Ok(Some(ServerMessage::Chat {
            chat_type, content, ..
        })) => {
            assert_eq!(chat_type, "all");
            assert_eq!(content, "gg");
        }
        other => panic!("Team B should receive all chat, got: {other:?}"),
    }

    // Spectator should receive it
    let msg_spec = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ws_next_message(&mut ws_spec),
    )
    .await;
    match msg_spec {
        Ok(Some(ServerMessage::Chat {
            chat_type, content, ..
        })) => {
            assert_eq!(chat_type, "all");
            assert_eq!(content, "gg");
        }
        other => panic!("Spectator should receive all chat, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_spectator_can_send_all_chat() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    // Connect participant and spectator
    let mut ws_a = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a, &setup.team_a_captain_token).await;
    drain_initial_messages(&mut ws_a).await;

    let mut ws_spec = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_spec, &setup.spectator_token).await;
    drain_initial_messages(&mut ws_spec).await;

    // Drain connection broadcasts
    drain_initial_messages(&mut ws_a).await;

    // Spectator sends all chat
    ws_send_chat(&mut ws_spec, "all", "nice play").await;

    // Participant should receive it
    let msg = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ws_next_message(&mut ws_a),
    )
    .await;
    match msg {
        Ok(Some(ServerMessage::Chat {
            chat_type, content, ..
        })) => {
            assert_eq!(chat_type, "all");
            assert_eq!(content, "nice play");
        }
        other => panic!("Participant should receive spectator's all chat, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_spectator_cannot_send_team_chat() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let mut ws_spec = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_spec, &setup.spectator_token).await;
    drain_initial_messages(&mut ws_spec).await;

    // Spectator sends team chat — should silently fail (error is logged, not sent)
    ws_send_chat(&mut ws_spec, "team", "should fail").await;

    // Verify no chat message is received back
    let msg = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        ws_next_message(&mut ws_spec),
    )
    .await;
    // Timeout or no message is the expected behavior
    if let Ok(Some(ServerMessage::Chat { .. })) = msg {
        panic!("Spectator should NOT receive echo of team chat");
    }
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_ban_already_banned_map() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let (mut ws_a, mut ws_b) = connect_both_teams(
        addr,
        &match_id_str,
        &setup.team_a_captain_token,
        &setup.team_b_captain_token,
    )
    .await;

    // Team A bans de_dust2 successfully
    assert!(
        do_veto_action(&mut ws_a, &mut ws_b, "de_dust2", false)
            .await
            .is_none()
    );

    // Team B tries to ban de_dust2 again — should fail (map not available)
    ws_ban_map(&mut ws_b, "de_dust2").await;
    let response = wait_for_veto_response(&mut ws_b).await;

    match response {
        Some(ServerMessage::Error { code, message }) => {
            assert!(
                code == "veto_error" || code == "not_your_turn",
                "Should be veto error, got code: {code}, message: {message}"
            );
        }
        other => panic!("Expected Error for already banned map, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_action_on_completed_session() {
    let mut app = TestApp::new().await;
    let setup = setup_veto_e2e_with_format(&app, "bo1").await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let (mut ws_a, mut ws_b) = connect_both_teams(
        addr,
        &match_id_str,
        &setup.team_a_captain_token,
        &setup.team_b_captain_token,
    )
    .await;

    // Run full Bo1 flow to completion
    do_veto_action(&mut ws_a, &mut ws_b, "de_dust2", false).await;
    do_veto_action(&mut ws_b, &mut ws_a, "de_nuke", false).await;
    do_veto_action(&mut ws_a, &mut ws_b, "de_mirage", false).await;
    do_veto_action(&mut ws_b, &mut ws_a, "de_inferno", false).await;
    do_veto_action(&mut ws_a, &mut ws_b, "de_ancient", false).await;
    let complete = do_veto_action(&mut ws_b, &mut ws_a, "de_anubis", false).await;
    assert!(complete.is_some(), "Veto should complete");

    // Try to ban after completion — should fail
    ws_ban_map(&mut ws_a, "de_vertigo").await;
    let response = wait_for_veto_response(&mut ws_a).await;

    match response {
        Some(ServerMessage::Error { code, message }) => {
            assert!(
                code == "veto_error",
                "Should be veto_error for completed session, got code: {code}, message: {message}"
            );
            assert!(
                message.contains("status")
                    || message.contains("completed")
                    || message.contains("Cannot perform"),
                "Error should mention status: {message}"
            );
        }
        other => panic!("Expected Error for action on completed session, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_pick_map_not_in_pool() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let mut ws_a = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a, &setup.team_a_captain_token).await;
    drain_initial_messages(&mut ws_a).await;

    // Try to ban a map that doesn't exist in the pool
    ws_ban_map(&mut ws_a, "de_nonexistent").await;
    let response = wait_for_veto_response(&mut ws_a).await;

    match response {
        Some(ServerMessage::Error { code, message }) => {
            assert!(
                code == "veto_error",
                "Should be veto_error, got code: {code}, message: {message}"
            );
            assert!(
                message.contains("not available"),
                "Should mention map not available: {message}"
            );
        }
        other => panic!("Expected Error for nonexistent map, got: {other:?}"),
    }
}

// ============================================================================
// RECONNECTION & DISCONNECT TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_reconnect_receives_chat_history() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    // Connect Team A captain
    let mut ws_a = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a, &setup.team_a_captain_token).await;
    drain_initial_messages(&mut ws_a).await;

    // Send all chat
    ws_send_chat(&mut ws_a, "all", "hello everyone").await;

    // Wait for the chat message echo
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        ws_next_message(&mut ws_a),
    )
    .await;

    // Disconnect
    drop(ws_a);

    // Small delay for server cleanup
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Reconnect
    let mut ws_a2 = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a2, &setup.team_a_captain_token).await;

    // After auth, should receive ChatHistory containing the "hello" message
    let mut found_chat_history = false;
    for _ in 0..5 {
        if let Some(msg) = ws_next_message(&mut ws_a2).await
            && let ServerMessage::ChatHistory { messages } = msg
        {
            let has_hello = messages
                .iter()
                .any(|m| m["content"].as_str() == Some("hello everyone"));
            assert!(has_hello, "Chat history should contain 'hello everyone'");
            found_chat_history = true;
            break;
        }
    }

    assert!(
        found_chat_history,
        "Should have received ChatHistory on reconnect"
    );
}

#[tokio::test]
async fn test_ws_disconnect_broadcast_received() {
    let mut app = TestApp::new().await;
    let setup = setup_ws_veto_scenario(&app).await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    // Connect Team A
    let mut ws_a = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a, &setup.team_a_captain_token).await;
    drain_initial_messages(&mut ws_a).await;

    // Connect Team B
    let mut ws_b = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_b, &setup.team_b_captain_token).await;
    drain_initial_messages(&mut ws_b).await;

    // Drain connection broadcasts
    drain_initial_messages(&mut ws_a).await;

    // Drop Team A connection
    drop(ws_a);

    // Team B should receive PlayerDisconnected
    let mut found_disconnect = false;
    for _ in 0..5 {
        if let Some(msg) = ws_next_message(&mut ws_b).await
            && let ServerMessage::PlayerDisconnected {
                team_name,
                username,
                ..
            } = msg
        {
            assert!(
                team_name.contains("Alpha"),
                "Should be Team Alpha: {team_name}"
            );
            assert!(
                username.contains("captain"),
                "Should be captain: {username}"
            );
            found_disconnect = true;
            break;
        }
    }
    assert!(found_disconnect, "Team B should receive PlayerDisconnected");

    // Reconnect Team A
    let mut ws_a2 = connect_veto_ws(addr, &match_id_str).await;
    let _ = ws_authenticate(&mut ws_a2, &setup.team_a_captain_token).await;

    // Team B should receive PlayerConnected
    let mut found_reconnect = false;
    for _ in 0..5 {
        if let Some(msg) = ws_next_message(&mut ws_b).await
            && let ServerMessage::PlayerConnected { team_name, .. } = msg
        {
            assert!(
                team_name.contains("Alpha"),
                "Should be Team Alpha: {team_name}"
            );
            found_reconnect = true;
            break;
        }
    }
    assert!(
        found_reconnect,
        "Team B should receive PlayerConnected on reconnect"
    );

    drop(ws_a2);
}

// ============================================================================
// SIDE SELECTION TESTS
// ============================================================================

#[tokio::test]
async fn test_ws_side_selection_after_pick() {
    let mut app = TestApp::new().await;
    // Side selection over WS only exists in picker_choice mode (knife and
    // coin_flip both reject manual selection).
    let setup = setup_veto_e2e_with_format_and_mode(
        &app,
        "bo3",
        portal_core::SideSelectionMode::PickerChoice,
    )
    .await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let (mut ws_a, mut ws_b) = connect_both_teams(
        addr,
        &match_id_str,
        &setup.team_a_captain_token,
        &setup.team_b_captain_token,
    )
    .await;

    // Bo3: Ban, Ban, then Team A picks de_mirage (action 3)
    do_veto_action(&mut ws_a, &mut ws_b, "de_dust2", false).await;
    do_veto_action(&mut ws_b, &mut ws_a, "de_nuke", false).await;
    do_veto_action(&mut ws_a, &mut ws_b, "de_mirage", true).await;

    // Team A (the picker) selects side "ct" for action 3
    ws_select_side(&mut ws_a, 3, "ct").await;

    // Team A should receive ack
    let response_a = wait_for_veto_response(&mut ws_a).await;
    match response_a {
        Some(ServerMessage::VetoActionAck { success, .. }) => {
            assert!(success, "Side selection should succeed");
        }
        Some(ServerMessage::VetoActionPerformed { action, .. }) => {
            // Broadcast received — check that it includes side selection
            assert!(
                action["side_selection"].as_str().is_some(),
                "Action should have side_selection"
            );
        }
        other => panic!("Expected ack or performed for side selection, got: {other:?}"),
    }

    // Team B should receive VetoActionPerformed with side selection
    let response_b = wait_for_veto_response(&mut ws_b).await;
    match response_b {
        Some(ServerMessage::VetoActionPerformed { action, .. }) => {
            assert_eq!(
                action["side_selection"].as_str(),
                Some("ct"),
                "Side selection should be 'ct'"
            );
        }
        other => panic!("Expected VetoActionPerformed with side selection, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ws_opponent_cannot_select_picker_side() {
    let mut app = TestApp::new().await;
    // picker_choice mode: only the team that picked the map may choose its
    // starting side.
    let setup = setup_veto_e2e_with_format_and_mode(
        &app,
        "bo3",
        portal_core::SideSelectionMode::PickerChoice,
    )
    .await;
    let addr = app.start_server().await;
    let match_id_str = setup.match_id.to_string();

    let (mut ws_a, mut ws_b) = connect_both_teams(
        addr,
        &match_id_str,
        &setup.team_a_captain_token,
        &setup.team_b_captain_token,
    )
    .await;

    // Bo3: Ban, Ban, then Team A picks de_mirage (action 3)
    do_veto_action(&mut ws_a, &mut ws_b, "de_dust2", false).await;
    do_veto_action(&mut ws_b, &mut ws_a, "de_nuke", false).await;
    do_veto_action(&mut ws_a, &mut ws_b, "de_mirage", true).await;

    // Team B (not the picker) tries to select side — should fail
    ws_select_side(&mut ws_b, 3, "ct").await;

    let response = wait_for_veto_response(&mut ws_b).await;
    match response {
        Some(ServerMessage::Error { code, message }) => {
            assert_eq!(code, "side_select_error", "Should be side_select_error");
            assert!(
                message.contains("picker"),
                "Error should mention the picker: {message}"
            );
        }
        other => panic!("Expected Error for non-picker selecting side, got: {other:?}"),
    }
}
