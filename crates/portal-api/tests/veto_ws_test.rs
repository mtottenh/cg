//! WebSocket veto lobby integration tests.

mod common;

use common::ws::{connect_veto_ws, ws_authenticate, ws_ban_map, ws_next_message, ServerMessage, WsStream};
use common::TestApp;
use portal_test::prelude::*;
use uuid::Uuid;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Drain initial messages that arrive after connection (chat history, broadcasts).
async fn drain_initial_messages(ws: &mut WsStream) {
    // Try to receive messages with short timeout, stop when no more messages
    for _ in 0..5 {
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            ws_next_message(ws),
        )
        .await;

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
        } else {
            break;
        }
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

    let delegate_token = create_test_token(delegate_user.id, delegate_user.id, "ws_veto_delegate", TEST_JWT_SECRET);

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
        other => panic!("Expected AuthSuccess, got: {:?}", other),
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
        other => panic!("Expected AuthSuccess, got: {:?}", other),
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
        other => panic!("Expected AuthSuccess, got: {:?}", other),
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
        other => panic!("Expected AuthSuccess, got: {:?}", other),
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
        other => panic!("Expected AuthSuccess, got: {:?}", other),
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
        other => panic!("Expected AuthSuccess, got: {:?}", other),
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
                "Should indicate invalid token: {}",
                error
            );
        }
        other => panic!("Expected AuthError, got: {:?}", other),
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
                "Should indicate match not found: {}",
                error
            );
        }
        other => panic!("Expected AuthError, got: {:?}", other),
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
        other => panic!("Expected VetoActionAck or VetoActionPerformed, got: {:?}", other),
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
                "Should mention participants: {}",
                message
            );
        }
        other => panic!("Expected Error, got: {:?}", other),
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
                "Should be turn-related error: {} - {}",
                code,
                message
            );
        }
        other => panic!("Expected Error, got: {:?}", other),
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
        if let Some(msg) = ws_next_message(&mut spectator_ws).await {
            if let ServerMessage::PlayerConnected {
                registration_id,
                team_name,
                username,
            } = msg
            {
                assert!(!registration_id.is_empty(), "Should have registration ID");
                assert!(
                    team_name.contains("Alpha"),
                    "Should be Team Alpha: {}",
                    team_name
                );
                assert!(
                    username.contains("captain"),
                    "Should be captain: {}",
                    username
                );
                found_player_connected = true;
                break;
            }
        }
    }

    assert!(found_player_connected, "Should have received PlayerConnected broadcast");

    // Clean up participant connection
    drop(participant_ws);
}
